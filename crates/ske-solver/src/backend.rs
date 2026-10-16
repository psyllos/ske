use crate::{CheckResult, Model, ModelValue, Query, Solver, SolverBudget, Sort, TimeoutKind};
use std::{
    collections::VecDeque,
    ffi::CStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use z3::{ast::Dynamic, Config, Context, Params, SatResult};

// Module parameters are global in Z3, not Solver::set_params parameters.
// Serialize our checks and restore defaults before releasing the lock.
static Z3_LOCK: Mutex<()> = Mutex::new(());
struct Seeds {
    smt: String,
    sat: String,
}
impl Seeds {
    fn set(seed: u32) -> Self {
        let old = Self {
            smt: z3::get_global_param("smt.random_seed").unwrap_or_else(|| "0".into()),
            sat: z3::get_global_param("sat.random_seed").unwrap_or_else(|| "0".into()),
        };
        z3::set_global_param("smt.random_seed", &seed.to_string());
        z3::set_global_param("sat.random_seed", &seed.to_string());
        old
    }
}
impl Drop for Seeds {
    fn drop(&mut self) {
        z3::set_global_param("smt.random_seed", &self.smt);
        z3::set_global_param("sat.random_seed", &self.sat);
    }
}

pub fn map_unknown_reason(reason: &str) -> CheckResult {
    let lower = reason.to_ascii_lowercase();
    if lower.contains("resource") || lower.contains("rlimit") {
        CheckResult::Timeout {
            kind: TimeoutKind::Rlimit,
        }
    } else if lower.contains("timeout") {
        CheckResult::Timeout {
            kind: TimeoutKind::WallClock,
        }
    } else if lower.contains("canceled") || lower.contains("cancelled") {
        // Z3's bare "canceled" does not identify which limit fired.
        CheckResult::Timeout {
            kind: TimeoutKind::Budget,
        }
    } else {
        CheckResult::Unknown {
            reason: reason.into(),
        }
    }
}

#[derive(Default)]
pub struct Z3Solver;
impl Z3Solver {
    pub fn new() -> Self {
        Self
    }
}
fn unknown(reason: impl Into<String>) -> CheckResult {
    CheckResult::Unknown {
        reason: reason.into(),
    }
}
impl Z3Solver {
    fn run_check(
        &mut self,
        query: &Query,
        budget: &SolverBudget,
        cancel: Option<&(dyn Fn() -> bool + Sync)>,
    ) -> CheckResult {
        // Z3 exposes unsigned (32-bit) limit parameters. Never truncate u64.
        let (Ok(rlimit), Ok(timeout)) = (
            u32::try_from(budget.rlimit),
            u32::try_from(budget.wall_timeout_ms),
        ) else {
            return unknown("SOLVER_UNAVAILABLE: budget exceeds Z3 unsigned parameter range");
        };
        if query.smtlib.contains('\0') {
            return unknown("SOLVER_UNAVAILABLE: SMT-LIB contains NUL");
        }
        for variable in &query.variables {
            if variable.sort.validate().is_err() || crate::encoding::symbol(&variable.name).is_err()
            {
                return unknown("SOLVER_UNAVAILABLE: invalid model variable");
            }
        }
        // New context as well as a new solver: no accumulated rlimit, options,
        // assertions, or randomized state from an earlier query.
        let Ok(_lock) = Z3_LOCK.lock() else {
            return unknown("SOLVER_UNAVAILABLE: poisoned Z3 lock");
        };
        let _seeds = Seeds::set(budget.seed);
        z3::with_z3_config(&Config::new(), || {
            let solver = z3::Solver::new();
            let ctx = Context::thread_local();
            let deadline = (budget.wall_timeout_ms != 0)
                .then(|| Instant::now() + Duration::from_millis(budget.wall_timeout_ms));
            // z3.rs disables the default aborting error handler, but its
            // from_string API does not expose parser failure. Inspect the error
            // code after each parse, otherwise malformed text could look
            // satisfiable.
            let native_error = || -> Option<String> {
                unsafe {
                    let raw = ctx.get_z3_context();
                    let code = z3_sys::Z3_get_error_code(raw);
                    if code == z3_sys::ErrorCode::Ok {
                        None
                    } else {
                        Some(
                            CStr::from_ptr(z3_sys::Z3_get_error_msg(raw, code))
                                .to_string_lossy()
                                .into_owned(),
                        )
                    }
                }
            };
            // `native` runs the whole native section (parse, parameters, check).
            // When the engine is cancellable, the SMT-LIB text is fed in bounded
            // chunks so a SIGINT landing mid-parse is serviced promptly. Z3's
            // SMT-LIB parser is neither interruptible nor bounded by the solver's
            // timeout parameters, and the engine's text grows with each loop
            // unrolling; a single from_string over a large input could otherwise
            // swallow the cancel flag for seconds to minutes. Each line is one
            // complete command (guaranteed by crate::encode and by this crate's
            // tests), so splitting at line boundaries never feeds a partial
            // command.
            const FEED_CHUNK_LINES: usize = 16_384;
            let native = |cancel: Option<&(dyn Fn() -> bool + Sync)>,
                          deadline: Option<Instant>|
             -> Result<SatResult, CheckResult> {
                match cancel {
                    Some(cancel) => {
                        let mut chunk = String::new();
                        let mut fed = 0usize;
                        for line in query.smtlib.split('\n') {
                            if line.trim().is_empty() {
                                continue;
                            }
                            if cancel() || deadline.is_some_and(|d| Instant::now() >= d) {
                                // Stop feeding rather than checking a partial
                                // assertion set; the caller's cancellation
                                // handling takes over from here.
                                return Err(unknown("interrupted"));
                            }
                            chunk.push_str(line);
                            chunk.push('\n');
                            fed += 1;
                            if fed >= FEED_CHUNK_LINES {
                                solver.from_string(chunk.as_bytes());
                                if let Some(e) = native_error() {
                                    return Err(unknown(format!("SOLVER_UNAVAILABLE: {e}")));
                                }
                                chunk.clear();
                                fed = 0;
                            }
                        }
                        if fed != 0 {
                            solver.from_string(chunk.as_bytes());
                            if let Some(e) = native_error() {
                                return Err(unknown(format!("SOLVER_UNAVAILABLE: {e}")));
                            }
                        }
                    }
                    None => {
                        solver.from_string(query.smtlib.as_bytes());
                        if let Some(e) = native_error() {
                            return Err(unknown(format!("SOLVER_UNAVAILABLE: {e}")));
                        }
                    }
                }
                let mut params = Params::new();
                params.set_u32("random_seed", budget.seed);
                params.set_u32("rlimit", rlimit);
                params.set_u32("timeout", timeout);
                // SKE owns SIGINT through signal-hook. Z3's default ctrl_c
                // handler consumes SIGINT during check without setting SKE's
                // cancellation flag. The watchdog handles native interruption.
                params.set_bool("ctrl_c", false);
                // Apply after parsing so persisted set-options cannot override budget.
                solver.set_params(&params);
                if let Some(e) = native_error() {
                    return Err(unknown(format!("SOLVER_UNAVAILABLE: parameters: {e}")));
                }
                Ok(solver.check())
            };
            // A watchdog interrupts the native context when the wall budget runs
            // out or the engine has been cancelled. `ContextHandle::interrupt`
            // aborts the current native operation promptly and is Send+Sync for
            // cross-thread use. It keeps re-arming until `native` returns: an
            // interrupt delivered before the active operation (or between parser
            // commands) can otherwise be lost, leaving the check uninterrupted
            // even though the engine cancelled.
            let sat = match cancel {
                Some(cancel) => {
                    let handle = ctx.handle();
                    let done = AtomicBool::new(false);
                    thread::scope(|scope| {
                        scope.spawn(|| loop {
                            if done.load(Ordering::Relaxed) {
                                return;
                            }
                            let overdue = deadline.is_some_and(|d| Instant::now() >= d);
                            if overdue || cancel() {
                                handle.interrupt();
                            }
                            thread::sleep(Duration::from_millis(10));
                        });
                        let sat = native(Some(cancel), deadline);
                        done.store(true, Ordering::Relaxed);
                        sat
                    })
                }
                None => native(None, deadline),
            };
            let result = match sat {
                Err(check_result) => return check_result,
                Ok(sat) => sat,
            };
            match result {
                SatResult::Unsat => CheckResult::Unsat,
                SatResult::Unknown => map_unknown_reason(
                    &solver
                        .get_reason_unknown()
                        .unwrap_or_else(|| "unknown".into()),
                ),
                SatResult::Sat => {
                    let Some(model) = solver.get_model() else {
                        return CheckResult::Sat { model: None };
                    };
                    let mut values = Model::new();
                    for v in &query.variables {
                        let sort = match &v.sort {
                            Sort::Bool => z3::Sort::bool(),
                            Sort::BitVec(w) => z3::Sort::bitvector(*w),
                            Sort::Uninterpreted(_) => z3::Sort::uninterpreted(v.sort.smt().into()),
                        };
                        let var = Dynamic::new_const(v.name.clone(), &sort);
                        let Some(value) = model.eval(&var, true) else {
                            return unknown("SOLVER_UNAVAILABLE: model evaluation failed");
                        };
                        let value = match &v.sort {
                            Sort::Bool => match value.as_bool().and_then(|b| b.as_bool()) {
                                Some(b) => ModelValue::Bool(b),
                                None => return unknown("invalid Bool model value"),
                            },
                            Sort::BitVec(w) => {
                                // SMT-LIB numerals can be 128 bits, unlike as_u64.
                                let text = value.to_string();
                                let bits = if let Some(hex) = text.strip_prefix("#x") {
                                    u128::from_str_radix(hex, 16).ok()
                                } else if let Some(bin) = text.strip_prefix("#b") {
                                    u128::from_str_radix(bin, 2).ok()
                                } else {
                                    text.strip_prefix("(_ bv")
                                        .and_then(|s| s.split_whitespace().next())
                                        .and_then(|s| s.parse().ok())
                                };
                                match bits {
                                    Some(bits) => ModelValue::BitVec { width: *w, bits },
                                    None => {
                                        return unknown(format!("invalid BV model value: {text}"))
                                    }
                                }
                            }
                            Sort::Uninterpreted(_) => ModelValue::Uninterpreted(value.to_string()),
                        };
                        values.insert(v.name.clone(), value);
                    }
                    CheckResult::Sat {
                        model: Some(values),
                    }
                }
            }
        })
    }
}
impl Solver for Z3Solver {
    fn name(&self) -> &str {
        "Z3"
    }
    fn version(&self) -> String {
        z3::full_version().into()
    }
    fn check(&mut self, query: &Query, budget: &SolverBudget) -> CheckResult {
        self.run_check(query, budget, None)
    }
    fn check_interruptible(
        &mut self,
        query: &Query,
        budget: &SolverBudget,
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> CheckResult {
        self.run_check(query, budget, Some(cancelled))
    }
}

/// Deterministic test backend. Exhaustion is explicit UNKNOWN, never guessed SAT.
pub struct ScriptedSolver {
    responses: VecDeque<CheckResult>,
    pub queries: Vec<Query>,
    pub budgets: Vec<SolverBudget>,
}
impl ScriptedSolver {
    pub fn new(responses: impl IntoIterator<Item = CheckResult>) -> Self {
        Self {
            responses: responses.into_iter().collect(),
            queries: vec![],
            budgets: vec![],
        }
    }
    pub fn remaining(&self) -> usize {
        self.responses.len()
    }
}
impl Solver for ScriptedSolver {
    fn name(&self) -> &str {
        "ScriptedSolver"
    }
    fn version(&self) -> String {
        "1".into()
    }
    fn check(&mut self, query: &Query, budget: &SolverBudget) -> CheckResult {
        self.queries.push(query.clone());
        self.budgets.push(budget.clone());
        self.responses
            .pop_front()
            .unwrap_or_else(|| unknown("script exhausted"))
    }
}
