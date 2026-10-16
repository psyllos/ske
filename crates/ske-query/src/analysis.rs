//! Application orchestration: stream exploration facts, finalize, then verify.
use super::{
    analysis_config::{Budget, Context},
    sql,
};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use ske_common::{sha256_hex, Error, Result};
use ske_solver::{CheckResult, ModelValue, Solver, TimeoutKind, Z3Solver};
use ske_symbolic::{
    engine::{self, Classification, Event, Soundness},
    BinaryOp, Constant, ExprArena, ExprId, ExprKind, Node, Sort, Type,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

const VERSION: &str = "ske-symbolic-0.3";

fn precision(s: Soundness) -> &'static str {
    match s {
        Soundness::Exact => "EXACT",
        Soundness::Overapprox => "OVERAPPROX",
        Soundness::Opaque => "OPAQUE",
        Soundness::Unsupported => "UNSUPPORTED",
    }
}
fn answer(r: &CheckResult) -> &'static str {
    match r {
        CheckResult::Sat { .. } => "SAT",
        CheckResult::Unsat => "UNSAT",
        CheckResult::Unknown { reason } if reason.starts_with("SOLVER_UNAVAILABLE") => "ERROR",
        CheckResult::Unknown { .. } => "UNKNOWN",
        CheckResult::Timeout { .. } => "TIMEOUT",
    }
}
fn timeout(r: &CheckResult) -> Option<&'static str> {
    if let CheckResult::Timeout { kind } = r {
        Some(match kind {
            TimeoutKind::Rlimit => "RLIMIT",
            TimeoutKind::WallClock => "WALL_CLOCK",
            TimeoutKind::Budget => "BUDGET",
        })
    } else {
        None
    }
}
fn reason(r: &CheckResult) -> Option<&str> {
    if let CheckResult::Unknown { reason } = r {
        Some(reason)
    } else {
        None
    }
}
fn diagnostic(db: &Connection, snapshot: i64, function: i64, run: i64, code: &str) -> Result<()> {
    let spec = ske_common::diagnostics::lookup(code).ok_or_else(|| {
        Error::new(
            "E_INTERNAL",
            format!("unregistered engine diagnostic {code}"),
        )
    })?;
    db.execute("INSERT INTO diagnostics(snapshot_id,function_id,analysis_run_id,severity,code,impact,message) VALUES(?1,?2,?3,?4,?5,?6,?5)", params![snapshot,function,run,spec.severity,code,spec.impact]).map_err(sql)?;
    Ok(())
}

fn check_context(program: &ske_symbolic::program::Program, context: &Context) -> Result<()> {
    for range in &context.param_range {
        let param = program
            .values
            .values()
            .find(|v| v.role == "PARAM" && v.name == range.param)
            .ok_or_else(|| Error::new("E_USAGE", format!("unknown parameter {}", range.param)))?;
        let ty = &program.types[&param.type_id];
        if ty.kind != "BITVECTOR" || ty.bit_width == 0 || ty.bit_width > 128 {
            return Err(Error::new(
                "E_USAGE",
                "param_range requires an integer parameter",
            ));
        }
        let valid = if ty.is_signed {
            ty.bit_width >= 64 || {
                let bound = 1_i64 << (ty.bit_width - 1);
                range.min >= -bound && range.max < bound
            }
        } else {
            range.min >= 0 && (ty.bit_width >= 63 || range.max < (1_i64 << ty.bit_width))
        };
        if !valid {
            return Err(Error::new(
                "E_USAGE",
                format!("range outside type of {}", range.param),
            ));
        }
    }
    Ok(())
}

pub fn run(
    db: &mut Connection,
    snapshot: i64,
    function: i64,
    budget: &Budget,
    context: &Context,
    cancel: &AtomicBool,
) -> Result<Value> {
    run_with_solver(
        db,
        snapshot,
        function,
        budget,
        context,
        cancel,
        &mut Z3Solver::new(),
    )
}

fn run_with_solver(
    db: &mut Connection,
    snapshot: i64,
    function: i64,
    budget: &Budget,
    context: &Context,
    cancel: &AtomicBool,
    solver: &mut dyn Solver,
) -> Result<Value> {
    let program = super::analysis_load::load(db, function)?;
    check_context(&program, context)?;
    let effective = engine::Budget {
        max_wall_time_ms: budget.max_wall_time_ms,
        max_states: budget.max_states,
        max_instructions: budget.max_instructions,
        max_call_depth: budget.max_call_depth,
        loop_unroll: budget.loop_unroll,
        loop_mode: budget.loop_mode.clone(),
        max_recursion_unfold: budget.max_recursion_unfold,
        max_solver_queries: budget.max_solver_queries,
        solver_rlimit: budget.solver_rlimit,
        solver_wall_timeout_ms: budget.solver_wall_timeout_ms,
        solver_seed: budget.solver_seed,
        max_memory_mb: budget.max_memory_mb,
    };
    let budget_json = budget.canonical()?;
    let context_json = context.canonical()?;
    let snapshot_config: String = db
        .query_row(
            "SELECT config_fingerprint FROM snapshots WHERE id=?1",
            [snapshot],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let config = sha256_hex(format!(
        "{snapshot_config}|{budget_json}|{}|{VERSION}|{}@{}",
        sha256_hex(&context_json),
        solver.name(),
        solver.version()
    ));
    db.execute("INSERT INTO analysis_runs(snapshot_id,entry_function_id,analysis_kind,engine_version,solver_name,solver_version,context_name,context_json,budget_json,config_fingerprint,assumptions_fingerprint,status) VALUES(?1,?2,'BRANCH_ANALYSIS',?3,?4,?5,?6,?7,?8,?9,?10,'PENDING')",
        params![snapshot,function,VERSION,solver.name(),solver.version(),context.name,context_json,budget_json,config,sha256_hex("")]).map_err(sql)?;
    let run = db.last_insert_rowid();
    for range in &context.param_range {
        let payload = ske_common::canonical_json(
            &json!({"param":range.param,"min":range.min,"max":range.max}),
        )?;
        db.execute("INSERT INTO analysis_assumptions(analysis_run_id,kind,description,payload) VALUES(?1,'PARAM_RANGE','Entry parameter range',?2)",params![run,payload]).map_err(sql)?;
    }
    let has_mapping:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM source_outcomes o JOIN source_decisions d ON d.id=o.decision_id WHERE d.function_id=?1 AND o.activation_mapping_complete=1)",[function],|r|r.get(0)).map_err(sql)?;
    if has_mapping {
        db.execute("INSERT INTO analysis_assumptions(analysis_run_id,kind,description) VALUES(?1,'ADAPTER_SOURCE_MAPPING_TRUSTED','Source activation cases rely on adapter mapping claims')",[run]).map_err(sql)?;
    }
    db.execute(
        "UPDATE analysis_runs SET status='RUNNING' WHERE id=?1",
        [run],
    )
    .map_err(sql)?;
    let query_ids = RefCell::new(BTreeMap::<usize, i64>::new());
    let sequences = RefCell::new(BTreeMap::<String, u64>::new());
    let persistence_error = RefCell::new(None::<Error>);
    let had_error = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let solver_failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let start = Instant::now();
    let report = {
        let connection: &Connection = db;
        let mut emit = |event: &Event| {
            if persistence_error.borrow().is_some() {
                return;
            }
            let result = (|| -> Result<()> {
                match event {
                    Event::Query(q) => {
                        let result = answer(&q.result);
                        connection.execute("INSERT INTO solver_queries(analysis_run_id,query_fingerprint,smtlib,result,timeout_kind,reason,solver_seed,solver_rlimit) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                            params![run,q.fingerprint,q.smtlib,result,timeout(&q.result),reason(&q.result),budget.solver_seed,budget.solver_rlimit as i64]).map_err(sql)?;
                        query_ids
                            .borrow_mut()
                            .insert(q.id, connection.last_insert_rowid());
                        if result == "ERROR" {
                            solver_failed.store(true, Ordering::Relaxed);
                            diagnostic(connection, snapshot, function, run, "SOLVER_UNAVAILABLE")?;
                        }
                    }
                    Event::Arrival(a) => {
                        let mut sequences = sequences.borrow_mut();
                        let seq = sequences.entry(a.outcome_id.clone()).or_default();
                        let result = if a.soundness == Soundness::Unsupported {
                            "UNSUPPORTED"
                        } else if answer(&a.result) == "ERROR" {
                            "UNKNOWN"
                        } else {
                            answer(&a.result)
                        };
                        let query_id = a
                            .query_id
                            .and_then(|id| query_ids.borrow().get(&id).copied());
                        let path = a
                            .taken_outcomes
                            .ordered()
                            .iter()
                            .map(|id| {
                                id.parse::<i64>()
                                    .map_err(|e| Error::new("E_INTERNAL", e.to_string()))
                            })
                            .collect::<Result<Vec<_>>>()?;
                        connection.execute("INSERT INTO outcome_arrivals(analysis_run_id,ir_outcome_id,arrival_seq,query_id,result,soundness,path_json) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                            params![run,a.outcome_id,*seq as i64,query_id,result,precision(a.soundness),serde_json::to_string(&path).unwrap()]).map_err(sql)?;
                        *seq += 1;
                        // Retain already validated possibilities even if a later solver call fails.
                        if result == "SAT" {
                            connection.execute("INSERT INTO branch_analysis_results(analysis_run_id,ir_outcome_id,status,soundness,explanation) VALUES(?1,?2,'POSSIBLE',?3,'SAT_ARRIVAL') ON CONFLICT(analysis_run_id,ir_outcome_id) DO NOTHING", params![run,a.outcome_id,precision(a.soundness)]).map_err(sql)?;
                        }
                    }
                    Event::Diagnostic(code) => {
                        diagnostic(connection, snapshot, function, run, code)?
                    }
                    Event::Assumption(kind) => {
                        connection.execute("INSERT INTO analysis_assumptions(analysis_run_id,kind,description) VALUES(?1,?2,?2)",params![run,kind]).map_err(sql)?;
                    }
                    Event::Exit(_) => {}
                }
                Ok(())
            })();
            if let Err(error) = result {
                *persistence_error.borrow_mut() = Some(error);
                had_error.store(true, Ordering::Relaxed);
            }
        };
        let constraints = |arena: &mut ExprArena, env: &BTreeMap<String, ExprId>| {
            let mut constraints = Vec::new();
            for range in &context.param_range {
                let param = program
                    .values
                    .values()
                    .find(|v| v.role == "PARAM" && v.name == range.param)
                    .ok_or("missing context parameter")?;
                let value = env[&param.id];
                let ty = arena.get(value).ok_or("missing parameter")?.ty.clone();
                let width = match ty.sort {
                    Sort::BitVec { width, .. } => width,
                    _ => return Err("noninteger context".into()),
                };
                let mask = if width == 128 {
                    u128::MAX
                } else {
                    (1_u128 << width) - 1
                };
                let low = arena
                    .constant(
                        ty.clone(),
                        Constant::Bits((range.min as i128 as u128) & mask),
                    )
                    .map_err(|e| format!("{e:?}"))?;
                let high = arena
                    .constant(ty, Constant::Bits((range.max as i128 as u128) & mask))
                    .map_err(|e| format!("{e:?}"))?;
                let bool_ty = Type {
                    local_key: program
                        .types
                        .values()
                        .find(|t| t.kind == "BOOL")
                        .map(|t| t.id.clone())
                        .unwrap_or_else(|| "engine:bool".into()),
                    sort: Sort::Bool,
                };
                constraints.push(
                    arena
                        .binary(bool_ty.clone(), BinaryOp::Le, low, value)
                        .map_err(|e| format!("{e:?}"))?,
                );
                constraints.push(
                    arena
                        .binary(bool_ty, BinaryOp::Le, value, high)
                        .map_err(|e| format!("{e:?}"))?,
                );
            }
            Ok(constraints)
        };
        engine::analyze(
            &program,
            &effective,
            solver,
            constraints,
            &|| {
                cancel.load(Ordering::Relaxed)
                    || had_error.load(Ordering::Relaxed)
                    || solver_failed.load(Ordering::Relaxed)
            },
            &mut emit,
        )
    };
    if let Some(error) = persistence_error.into_inner() {
        db.execute("UPDATE analysis_runs SET status='FAILED',error_message=?2,finished_at=CURRENT_TIMESTAMP WHERE id=?1",params![run,error.to_string()]).map_err(sql)?;
        return Err(error);
    }
    let mut report = match report {
        Ok(report) => report,
        Err(error) => {
            db.execute("UPDATE analysis_runs SET status='FAILED',error_message=?2,finished_at=CURRENT_TIMESTAMP WHERE id=?1",params![run,error]).map_err(sql)?;
            return Err(Error::new("E_INTERNAL", error));
        }
    };
    if solver_failed.load(Ordering::Relaxed) {
        report.status = engine::RunStatus::Failed;
        report.exploration_closed = false;
    }
    let status = match report.status {
        engine::RunStatus::Completed => "COMPLETED",
        engine::RunStatus::Partial => "PARTIAL",
        engine::RunStatus::Cancelled => "CANCELLED",
        engine::RunStatus::Failed => "FAILED",
    };
    let assumptions: Vec<String> = {
        let mut stmt=db.prepare("SELECT DISTINCT kind FROM analysis_assumptions WHERE analysis_run_id=?1 ORDER BY kind").map_err(sql)?;
        let values = stmt
            .query_map([run], |r| r.get(0))
            .map_err(sql)?
            .collect::<rusqlite::Result<_>>()
            .map_err(sql)?;
        values
    };
    let mut query_counts =
        json!({"total":report.queries.len(),"sat":0,"unsat":0,"unknown":0,"timeout":0});
    for q in &report.queries {
        let key = match answer(&q.result) {
            "SAT" => "sat",
            "UNSAT" => "unsat",
            "TIMEOUT" => "timeout",
            _ => "unknown",
        };
        query_counts[key] = json!(query_counts[key].as_u64().unwrap() + 1);
    }
    let metrics = json!({"states_created":report.states_created,"instructions":report.instructions,"solver_queries":query_counts,"wall_ms":start.elapsed().as_millis() as u64});
    db.execute("UPDATE analysis_runs SET status=?2,exploration_closed=?3,cut_count=?4,metrics_json=?5,assumptions_fingerprint=?6,finished_at=CURRENT_TIMESTAMP WHERE id=?1",params![run,status,report.exploration_closed,report.cut_count as i64,metrics.to_string(),sha256_hex(assumptions.join("\n"))]).map_err(sql)?;
    // Exploration support facts are complete before any accepted proof freezes them.
    for (id, result) in &report.outcomes {
        if result.status == Classification::UnreachableCandidate {
            continue;
        }
        let status = match result.status {
            Classification::Possible => "POSSIBLE",
            Classification::Unsupported => "UNSUPPORTED",
            Classification::Timeout => "TIMEOUT",
            Classification::Unknown => "UNKNOWN",
            Classification::Unanalyzed => "UNANALYZED",
            Classification::UnreachableCandidate => unreachable!(),
        };
        db.execute("INSERT INTO branch_analysis_results(analysis_run_id,ir_outcome_id,status,soundness,explanation) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(analysis_run_id,ir_outcome_id) DO UPDATE SET status=excluded.status,soundness=excluded.soundness,explanation=excluded.explanation",params![run,id,status,precision(result.soundness),result.explanation]).map_err(sql)?;
    }
    let cancelled = cancel.load(Ordering::Relaxed) || report.status == engine::RunStatus::Cancelled;
    for arrival in &report.arrivals {
        if cancelled {
            // Arrivals and SAT possibilities were persisted during streaming;
            // a cancelled run exits promptly without unbounded witness rows.
            break;
        }
        if let CheckResult::Sat { model } = &arrival.result {
            let result_id: i64 = db.query_row("SELECT id FROM branch_analysis_results WHERE analysis_run_id=?1 AND ir_outcome_id=?2",params![run,arrival.outcome_id],|r|r.get(0)).map_err(sql)?;
            let mut guard = None;
            for &term in &arrival.constraints {
                guard = Some(match guard {
                    None => term,
                    Some(previous) => {
                        let ty = report
                            .arena
                            .get(previous)
                            .ok_or_else(|| Error::new("E_INTERNAL", "missing guard expression"))?
                            .ty
                            .clone();
                        let term = if report.arena.get(term).map(|n| &n.ty) != Some(&ty) {
                            // Engine-created BOOL sorts and CIR BOOL types have the
                            // same semantics but different provenance keys. Preserve
                            // both types through an explicit Boolean ITE conversion.
                            let yes = report
                                .arena
                                .constant(ty.clone(), Constant::Bool(true))
                                .map_err(|e| Error::new("E_INTERNAL", format!("{e:?}")))?;
                            let no = report
                                .arena
                                .constant(ty.clone(), Constant::Bool(false))
                                .map_err(|e| Error::new("E_INTERNAL", format!("{e:?}")))?;
                            report
                                .arena
                                .ite(term, yes, no)
                                .map_err(|e| Error::new("E_INTERNAL", format!("{e:?}")))?
                        } else {
                            term
                        };
                        report
                            .arena
                            .binary(ty, BinaryOp::And, previous, term)
                            .map_err(|e| Error::new("E_INTERNAL", format!("{e:?}")))?
                    }
                });
            }
            let guard_id = guard
                .map(|id| {
                    persist_expr(
                        db,
                        snapshot,
                        function,
                        &report.arena,
                        id,
                        &mut BTreeMap::new(),
                    )
                })
                .transpose()?
                .map(|(id, _)| id);
            let witness = model.as_ref().map(|model| {
                let mut values = serde_json::Map::new();
                for param_id in &program.function.parameter_value_ids {
                    let param = &program.values[param_id];
                    let name = ske_symbolic::encode::symbol_name(&format!(
                        "param:{}:{}",
                        program.function.id,
                        param.param_index.unwrap()
                    ));
                    if let Some(value) = model.get(&name) {
                        let value = match value {
                            ModelValue::Bool(b) => json!(b.to_string()),
                            ModelValue::BitVec { width, bits } => {
                                let signed = program.types[&param.type_id].is_signed;
                                let text = if signed && *width == 128 {
                                    (*bits as i128).to_string()
                                } else if signed && bits & (1_u128 << (width - 1)) != 0 {
                                    ((*bits as i128) - (1_i128 << width)).to_string()
                                } else {
                                    bits.to_string()
                                };
                                json!(text)
                            }
                            ModelValue::Uninterpreted(token) => {
                                json!({"kind":"UNINTERPRETED","token":token})
                            }
                        };
                        values.insert(param.name.clone(), value);
                    }
                }
                Value::Object(values).to_string()
            });
            db.execute("INSERT INTO branch_cases(result_id,guard_expression_id,witness_json,validation_method) VALUES(?1,?2,?3,?4)",params![result_id,guard_id,witness,witness.as_ref().map(|_|"SMT_MODEL")]).map_err(sql)?;
        }
    }
    for (id, result) in &report.outcomes {
        if result.status != Classification::UnreachableCandidate {
            continue;
        }
        if cancelled {
            break;
        }
        if cancel.load(Ordering::Relaxed) {
            db.execute("INSERT INTO branch_analysis_results(analysis_run_id,ir_outcome_id,status,soundness,explanation) VALUES(?1,?2,'UNKNOWN',?3,'VERIFICATION_CANCELLED')",params![run,id,precision(result.soundness)]).map_err(sql)?;
            continue;
        }
        let outcome = id
            .parse()
            .map_err(|e| Error::new("E_INTERNAL", format!("{e}")))?;
        let budget = effective.solver_budget();
        ske_verify::verify_unreachability(db, run, outcome, |text, seed| {
            let mut fresh = Z3Solver::new();
            let mut budget = budget.clone();
            budget.seed = seed;
            match fresh.check_smtlib(text, &budget) {
                CheckResult::Unsat => ske_verify::ReplayResult::Unsat,
                CheckResult::Sat { .. } => ske_verify::ReplayResult::Sat,
                CheckResult::Unknown { .. } => ske_verify::ReplayResult::Unknown,
                CheckResult::Timeout { .. } => ske_verify::ReplayResult::Timeout,
            }
        })
        .map_err(sql)?;
    }
    super::analysis_read::summary(db, run)
}

fn persist_expr(
    db: &Connection,
    snapshot: i64,
    function: i64,
    arena: &ExprArena,
    id: ExprId,
    cache: &mut BTreeMap<usize, (i64, String)>,
) -> Result<(i64, String)> {
    if let Some(value) = cache.get(&id.0) {
        return Ok(value.clone());
    }
    let Node { ty, kind } = arena
        .get(id)
        .ok_or_else(|| Error::new("E_INTERNAL", "invalid expression"))?;
    let (name, payload, children) = match kind {
        ExprKind::Const(c) => (
            "CONST",
            match c {
                Constant::Bool(b) => json!(b),
                Constant::Bits(bits) => json!(bits.to_string()),
                Constant::Null => Value::Null,
            },
            vec![],
        ),
        ExprKind::Symbol(name) => ("SYMBOL", json!(name), vec![]),
        ExprKind::Unary { op, arg } => ("UNARY", json!(format!("{op:?}")), vec![*arg]),
        ExprKind::Binary { op, left, right } => {
            ("BINARY", json!(format!("{op:?}")), vec![*left, *right])
        }
        ExprKind::Ite {
            condition,
            then_expr,
            else_expr,
        } => ("ITE", Value::Null, vec![*condition, *then_expr, *else_expr]),
        ExprKind::Field { base, index } => ("FIELD", json!(index), vec![*base]),
        ExprKind::Uninterpreted { name, args } => ("UNINTERPRETED", json!(name), args.clone()),
        ExprKind::Havoc(n) => ("HAVOC", json!(n), vec![]),
    };
    let children = children
        .into_iter()
        .map(|id| persist_expr(db, snapshot, function, arena, id, cache))
        .collect::<Result<Vec<_>>>()?;
    let payload = payload.to_string();
    let hashes = children
        .iter()
        .map(|(_, hash)| hash.as_str())
        .collect::<Vec<_>>()
        .join("|");
    let hash = sha256_hex(format!("{name}|{}|{payload}|{hashes}", ty.local_key));
    let type_id:Option<i64>=db.query_row("SELECT t.id FROM cir_types t JOIN symbols s ON s.unit_id=t.unit_id JOIN functions f ON f.symbol_id=s.id WHERE f.id=?1 AND t.local_key=(SELECT u.unit_key||'|'||?2 FROM compilation_units u WHERE u.id=s.unit_id)",params![function,ty.local_key],|r|r.get(0)).optional().map_err(sql)?;
    db.execute("INSERT INTO symbolic_expressions(snapshot_id,expression_hash,kind,type_id,payload) VALUES(?1,?2,?3,?4,?5) ON CONFLICT DO NOTHING",params![snapshot,hash,name,type_id,payload.as_bytes()]).map_err(sql)?;
    let (stored,stored_kind,stored_ty,stored_payload):(i64,String,Option<i64>,Vec<u8>)=db.query_row("SELECT id,kind,type_id,payload FROM symbolic_expressions WHERE snapshot_id=?1 AND expression_hash=?2",params![snapshot,hash],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(sql)?;
    if stored_kind != name || stored_ty != type_id || stored_payload != payload.as_bytes() {
        return Err(Error::new(
            "E_HASH_COLLISION",
            "expression structural mismatch",
        ));
    }
    for (ordinal, (child, _)) in children.iter().enumerate() {
        db.execute("INSERT INTO expression_operands(expression_id,ordinal,operand_expression_id) VALUES(?1,?2,?3) ON CONFLICT DO NOTHING",params![stored,ordinal as i64,child]).map_err(sql)?;
    }
    let actual: Vec<i64> = {
        let mut stmt=db.prepare("SELECT operand_expression_id FROM expression_operands WHERE expression_id=?1 ORDER BY ordinal").map_err(sql)?;
        let result = stmt
            .query_map([stored], |r| r.get(0))
            .map_err(sql)?
            .collect::<rusqlite::Result<_>>()
            .map_err(sql)?;
        result
    };
    if actual != children.iter().map(|(id, _)| *id).collect::<Vec<_>>() {
        return Err(Error::new(
            "E_HASH_COLLISION",
            "expression operands mismatch",
        ));
    }
    cache.insert(id.0, (stored, hash.clone()));
    Ok((stored, hash))
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, ske_storage::Storage, i64) {
        let dir = tempfile::tempdir().unwrap();
        let storage = ske_storage::Storage::open(dir.path().join("analysis.sqlite")).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        ske_ingest::ingest_many(
            &storage,
            &[root.join("tests/fixtures/cir/decide.cir.pb")],
            None,
            None,
        )
        .unwrap();
        let function = storage
            .conn
            .query_row("SELECT id FROM functions LIMIT 1", [], |r| r.get(0))
            .unwrap();
        (dir, storage, function)
    }

    #[test]
    fn solver_fail_preserves_validated_results() {
        let (_dir, mut storage, function) = fixture();
        let mut solver = ske_solver::ScriptedSolver::new([
            CheckResult::Sat { model: None },
            CheckResult::Unknown {
                reason: "SOLVER_UNAVAILABLE: injected backend failure".into(),
            },
        ]);
        let result = run_with_solver(
            &mut storage.conn,
            1,
            function,
            &Budget::default(),
            &Context::default(),
            &AtomicBool::new(false),
            &mut solver,
        );
        assert!(result.is_err());
        let status: String = storage
            .conn
            .query_row("SELECT status FROM analysis_runs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "FAILED");
        let possible: i64 = storage
            .conn
            .query_row(
                "SELECT COUNT(*) FROM branch_analysis_results WHERE status='POSSIBLE'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(possible, 1);
        let errors: i64 = storage
            .conn
            .query_row(
                "SELECT COUNT(*) FROM diagnostics WHERE code='SOLVER_UNAVAILABLE'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(errors >= 1);
        assert!(storage.db_check().unwrap()["ok"].as_bool().unwrap());
    }

    #[test]
    fn prop_no_complete_on_error() {
        solver_fail_preserves_validated_results();
    }

    #[test]
    fn verifier_rejects_scripted_false_eliminations_e2e() {
        let (_dir, mut storage, function) = fixture();
        let mut solver = ske_solver::ScriptedSolver::new([CheckResult::Unsat, CheckResult::Unsat]);
        let summary = run_with_solver(
            &mut storage.conn,
            1,
            function,
            &Budget::default(),
            &Context::default(),
            &AtomicBool::new(false),
            &mut solver,
        )
        .unwrap();
        assert_eq!(summary["run_status"], "COMPLETED");
        assert_eq!(summary["counts"]["UNKNOWN"], 4);
        assert_eq!(summary["counts"]["PROVEN_UNREACHABLE"], 0);
        let rejected: i64 = storage
            .conn
            .query_row(
                "SELECT COUNT(*) FROM proof_records WHERE verdict='REJECTED'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rejected, 4);
        let queries: i64 = storage
            .conn
            .query_row("SELECT COUNT(*) FROM solver_queries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(queries, 2, "verifier replays must not be persisted");
        assert!(storage.db_check().unwrap()["ok"].as_bool().unwrap());
    }
}
