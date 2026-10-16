//! Deterministic FIFO exploration. This module never accepts an unreachability proof.
use crate::expression::{
    BinaryOp, Constant, ExprArena, ExprId, ExprKind, Node, Sort, Type, UnaryOp,
};
use crate::memory::{Loc, MemoryBackend, MemoryState, Pointer, Precision, StoreEffect};
use crate::program::{Edge, Program};
use ske_cir::proto::{self, operation_payload::Details};
use ske_solver::{CheckResult, Solver, SolverBudget, TimeoutKind};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Soundness {
    Exact,
    Overapprox,
    Opaque,
    Unsupported,
}

#[derive(Clone, Debug)]
pub struct Budget {
    pub max_wall_time_ms: u64,
    pub max_states: u64,
    pub max_instructions: u64,
    pub max_solver_queries: u64,
    pub max_memory_mb: u64,
    pub loop_unroll: u32,
    pub max_call_depth: u32,
    pub loop_mode: String,
    pub max_recursion_unfold: u32,
    pub solver_rlimit: u64,
    pub solver_wall_timeout_ms: u64,
    pub solver_seed: u32,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            max_wall_time_ms: 60000,
            max_states: 10000,
            max_instructions: 1000000,
            max_solver_queries: 5000,
            max_memory_mb: 1024,
            loop_unroll: 3,
            max_call_depth: 8,
            loop_mode: "unroll".into(),
            max_recursion_unfold: 2,
            solver_rlimit: 5000000,
            solver_wall_timeout_ms: 10000,
            solver_seed: 0,
        }
    }
}
impl Budget {
    pub fn solver_budget(&self) -> SolverBudget {
        SolverBudget {
            rlimit: self.solver_rlimit,
            wall_timeout_ms: self.solver_wall_timeout_ms,
            seed: self.solver_seed,
        }
    }
}

/// An immutable cons list: branching shares all prior outcome provenance.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Path(pub Option<Arc<PathNode>>);
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PathNode {
    pub previous: Path,
    pub outcome: String,
}
impl Path {
    fn push(&self, outcome: String) -> Self {
        Self(Some(Arc::new(PathNode {
            previous: self.clone(),
            outcome,
        })))
    }
    pub fn ordered(&self) -> Vec<String> {
        let mut result = Vec::new();
        let mut cursor = self;
        while let Some(node) = &cursor.0 {
            result.push(node.outcome.clone());
            cursor = &node.previous;
        }
        result.reverse();
        result
    }
}

#[derive(Clone, Debug)]
pub struct Arrival {
    pub outcome_id: String,
    pub query_id: Option<usize>,
    pub result: CheckResult,
    pub soundness: Soundness,
    pub constraints: Vec<ExprId>,
    pub taken_outcomes: Path,
}
#[derive(Clone, Debug)]
pub struct Query {
    pub id: usize,
    pub smtlib: String,
    pub fingerprint: String,
    pub result: CheckResult,
}
#[derive(Clone, Debug)]
pub struct Exit {
    pub kind: String,
    pub instruction_id: String,
    pub constraints: Vec<ExprId>,
    pub soundness: Soundness,
    pub taken_outcomes: Path,
    pub values: Vec<ExprId>,
    /// A residual recursive invocation was overapproximated on this path.
    pub partial: bool,
    pub memory_delta: Vec<(Loc, ExprId)>,
}
#[derive(Clone, Debug)]
pub enum Event {
    Query(Query),
    Arrival(Arrival),
    Exit(Exit),
    Diagnostic(String),
    Assumption(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunStatus {
    Completed,
    Partial,
    Cancelled,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Classification {
    Possible,
    Unsupported,
    Timeout,
    Unknown,
    Unanalyzed,
    UnreachableCandidate,
}
#[derive(Clone, Debug)]
pub struct OutcomeResult {
    pub status: Classification,
    pub soundness: Soundness,
    pub explanation: &'static str,
}
pub struct Report {
    pub arena: ExprArena,
    /// Closed scalar callees whose cases were actually substituted.
    pub composed_calls: BTreeSet<String>,
    pub global_reads: BTreeMap<ExprId, String>,
    pub tuple_extractions: BTreeSet<String>,
    pub summary_unknowns: BTreeSet<String>,
    pub arrivals: Vec<Arrival>,
    pub queries: Vec<Query>,
    pub exits: Vec<Exit>,
    pub diagnostics: BTreeSet<String>,
    pub assumptions: BTreeSet<String>,
    pub exploration_closed: bool,
    pub cut_count: u64,
    pub states_created: u64,
    pub instructions: u64,
    pub status: RunStatus,
    pub outcomes: BTreeMap<String, OutcomeResult>,
}
#[derive(Clone)]
struct State {
    block: String,
    instruction: usize,
    predecessor: Option<String>,
    env: Arc<BTreeMap<String, ExprId>>,
    tuples: Arc<BTreeMap<String, Vec<ExprId>>>,
    partial: bool,
    residual_return: bool,
    abstract_headers: Arc<BTreeSet<String>>,
    constraints: Arc<Vec<ExprId>>,
    phi_env: Arc<BTreeMap<String, ExprId>>,
    back_edges: Arc<BTreeMap<String, u32>>,
    path: Path,
    soundness: Soundness,
    memory: MemoryState<Backend>,
    pointers: Arc<BTreeMap<ExprId, Pointer>>,
    /// Arena identity of the immutable memory snapshot. Never a hash of memory.
    memory_id: u64,
}

struct Backend {
    arena: ExprArena,
    havoc: u64,
}
impl MemoryBackend for Backend {
    type Expr = ExprId;
    type Sort = Type;
    fn sort(&self, e: &ExprId) -> Type {
        self.arena
            .get(*e)
            .expect("valid memory expression")
            .ty
            .clone()
    }
    fn fresh(&mut self, ty: &Type, hint: &str) -> ExprId {
        let number = self.havoc;
        self.havoc += 1;
        self.arena
            .symbol(ty.clone(), format!("{hint}:havoc#{number}"))
            .expect("valid memory sort")
    }
    fn fresh_bool(&mut self, hint: &str) -> ExprId {
        self.fresh(&bool_ty(), hint)
    }
    fn ite(&mut self, g: ExprId, y: ExprId, n: ExprId) -> ExprId {
        self.arena.ite(g, y, n).expect("valid memory ITE")
    }
}

pub fn expression_type(p: &Program, id: &str) -> Result<Type, String> {
    let t = p
        .types
        .get(id)
        .ok_or_else(|| format!("missing type {id}"))?;
    Ok(Type {
        local_key: id.into(),
        sort: match t.kind.as_str() {
            "BOOL" => Sort::Bool,
            "BITVECTOR" => Sort::BitVec {
                width: t.bit_width,
                signed: t.is_signed,
            },
            _ => Sort::Uninterpreted,
        },
    })
}
fn intern(a: &mut ExprArena, ty: Type, kind: ExprKind) -> Result<ExprId, String> {
    a.intern(Node { ty, kind }).map_err(|e| format!("{e:?}"))
}
fn bool_ty() -> Type {
    Type {
        local_key: "engine:bool".into(),
        sort: Sort::Bool,
    }
}

// Implicit normal-execution checks do not add a duplicate record for a
// predicate already carried by a source edge. Bool type provenance may differ;
// operators and operand identities must be identical.
fn same_boolean_predicate(arena: &ExprArena, a: ExprId, b: ExprId) -> bool {
    let (Some(a), Some(b)) = (arena.get(a), arena.get(b)) else {
        return false;
    };
    if a.ty.sort != Sort::Bool || b.ty.sort != Sort::Bool {
        return false;
    }
    if a.kind == b.kind {
        return true;
    }
    match (&a.kind, &b.kind) {
        (
            ExprKind::Unary {
                op: UnaryOp::Not,
                arg: a,
            },
            ExprKind::Unary {
                op: UnaryOp::Not,
                arg: b,
            },
        ) => arena
            .get(*a)
            .zip(arena.get(*b))
            .is_some_and(|(a, b)| a.kind == b.kind),
        _ => false,
    }
}
fn binary(
    a: &mut ExprArena,
    op: BinaryOp,
    l: ExprId,
    r: ExprId,
    ty: Type,
) -> Result<ExprId, String> {
    intern(
        a,
        ty,
        ExprKind::Binary {
            op,
            left: l,
            right: r,
        },
    )
}
fn not(a: &mut ExprArena, arg: ExprId) -> Result<ExprId, String> {
    let ty = a.get(arg).ok_or("invalid negation")?.ty.clone();
    intern(
        a,
        ty,
        ExprKind::Unary {
            op: UnaryOp::Not,
            arg,
        },
    )
}
fn constant(a: &mut ExprArena, b: bool) -> Result<ExprId, String> {
    intern(a, bool_ty(), ExprKind::Const(Constant::Bool(b)))
}

struct Runner<'a> {
    program: &'a Program,
    budget: &'a Budget,
    solver: &'a mut dyn Solver,
    emit: &'a mut dyn FnMut(&Event),
    cancelled: &'a (dyn Fn() -> bool + Sync),
    report: Report,
    cache: BTreeMap<String, usize>,
    backend: Backend,
    field_addresses: BTreeMap<(ExprId, u32), ExprId>,
    next_memory_id: u64,
    intervals: BTreeMap<String, ske_abstract::Interval>,
    intervals_safe: bool,
}
impl Runner<'_> {
    fn diagnostic(&mut self, code: &str) {
        if self.report.diagnostics.insert(code.into()) {
            (self.emit)(&Event::Diagnostic(code.into()));
        }
    }
    fn assumption(&mut self, code: &str) {
        if self.report.assumptions.insert(code.into()) {
            (self.emit)(&Event::Assumption(code.into()));
        }
    }
    fn fresh(&mut self, ty: Type) -> Result<ExprId, String> {
        let n = self.backend.havoc;
        self.backend.havoc += 1;
        intern(&mut self.backend.arena, ty, ExprKind::Havoc(n))
    }
    fn query(&mut self, constraints: &[ExprId]) -> Result<(Option<usize>, CheckResult), String> {
        let text = crate::encode::encode(&self.backend.arena, constraints)?;
        let variables = self
            .program
            .values
            .values()
            .filter(|v| v.role == "PARAM")
            .map(|v| {
                let ty = expression_type(self.program, &v.type_id)?;
                let sort = match ty.sort {
                    Sort::Bool => ske_solver::Sort::Bool,
                    Sort::BitVec { width, .. } => ske_solver::Sort::BitVec(width),
                    Sort::Uninterpreted => ske_solver::Sort::Uninterpreted(ty.local_key),
                };
                Ok(ske_solver::Variable {
                    name: crate::encode::symbol_name(&format!(
                        "param:{}:{}",
                        self.program.function.id,
                        v.param_index.ok_or("missing parameter index")?
                    )),
                    sort,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        // Variables absent from constraints still need declarations for model projection.
        let mut text = text;
        for variable in &variables {
            let declaration = format!("(declare-fun {} () {})", variable.name, variable.sort.smt());
            if !text.contains(&declaration) {
                let mut prefix = String::new();
                if let ske_solver::Sort::Uninterpreted(_) = variable.sort {
                    let sort_decl = format!("(declare-sort {} 0)", variable.sort.smt());
                    if !text.contains(&sort_decl) {
                        prefix.push_str(&format!("{sort_decl}\n"));
                    }
                }
                prefix.push_str(&format!("{declaration}\n"));
                text = text.replacen("(check-sat)", &format!("{prefix}(check-sat)"), 1);
            }
        }
        let fp = crate::encode::query_fingerprint(
            &text,
            self.budget.solver_seed,
            self.budget.solver_rlimit,
        );
        if let Some(id) = self.cache.get(&fp) {
            let q = &self.report.queries[*id];
            if q.smtlib != text {
                return Err("E_HASH_COLLISION".into());
            }
            return Ok((Some(*id), q.result.clone()));
        }
        if self.report.queries.len() as u64 >= self.budget.max_solver_queries {
            self.report.exploration_closed = false;
            self.diagnostic("SOLVER_TIMEOUT");
            return Ok((
                None,
                CheckResult::Timeout {
                    kind: TimeoutKind::Budget,
                },
            ));
        }
        let result = self.solver.check_interruptible(
            &ske_solver::Query {
                smtlib: text.clone(),
                variables,
            },
            &self.budget.solver_budget(),
            self.cancelled,
        );
        let id = self.report.queries.len();
        let query = Query {
            id,
            smtlib: text,
            fingerprint: fp.clone(),
            result: result.clone(),
        };
        (self.emit)(&Event::Query(query.clone()));
        self.report.queries.push(query);
        self.cache.insert(fp, id);
        if matches!(&result,CheckResult::Unknown{reason} if reason.starts_with("SOLVER_UNAVAILABLE"))
        {
            self.report.exploration_closed = false;
            self.report.status = RunStatus::Failed;
            self.diagnostic("SOLVER_UNAVAILABLE");
            return Err("SOLVER_UNAVAILABLE".into());
        }
        self.observe(&result);
        Ok((Some(id), result))
    }
    fn observe(&mut self, r: &CheckResult) {
        match r {
            CheckResult::Unknown { .. } => {
                self.report.exploration_closed = false;
                self.diagnostic("SOLVER_UNKNOWN");
            }
            CheckResult::Timeout { .. } => {
                self.report.exploration_closed = false;
                self.diagnostic("SOLVER_TIMEOUT");
            }
            _ => {}
        }
    }
    fn exit(&mut self, s: &State, ins: &proto::InstructionDef, kind: &str) -> Result<(), String> {
        let mut values: Vec<_> = ins
            .operand_value_ids
            .iter()
            .filter_map(|v| s.env.get(v).copied())
            .collect();
        if s.residual_return {
            values = self
                .program
                .function
                .return_type_ids
                .iter()
                .map(|ty| self.fresh(expression_type(self.program, ty)?))
                .collect::<Result<_, _>>()?;
        }
        let e = Exit {
            kind: kind.into(),
            instruction_id: ins.id.clone(),
            constraints: (*s.constraints).clone(),
            taken_outcomes: s.path.clone(),
            soundness: s.soundness,
            values,
            partial: s.partial,
            memory_delta: s.memory.summary_delta(),
        };
        (self.emit)(&Event::Exit(e.clone()));
        self.report.exits.push(e);
        Ok(())
    }
    fn cut(&mut self, code: &str) {
        self.report.cut_count += 1;
        self.report.exploration_closed = false;
        self.diagnostic(code);
    }
    fn state_budget(&mut self) -> bool {
        if self.report.states_created >= self.budget.max_states {
            self.cut("BUDGET_EXHAUSTED");
            false
        } else {
            self.report.states_created += 1;
            true
        }
    }
    fn follow(
        &mut self,
        mut s: State,
        e: &Edge,
        retreating: &BTreeSet<String>,
        queue: &mut VecDeque<State>,
    ) {
        if retreating.contains(&e.definition.id) {
            let count = Arc::make_mut(&mut s.back_edges)
                .entry(e.definition.id.clone())
                .or_default();
            *count += 1;
            if *count > self.budget.loop_unroll {
                let header = &e.definition.target_block_id;
                if self.budget.loop_mode == "intervals" && self.intervals_safe {
                    if s.abstract_headers.contains(header) {
                        return;
                    }
                    Arc::make_mut(&mut s.abstract_headers).insert(header.clone());
                    let phis: Vec<_> = self.program.blocks[header]
                        .instructions
                        .iter()
                        .filter(|i| i.opcode == "PHI")
                        .cloned()
                        .collect();
                    for phi in phis {
                        let inputs = &phi.payload.as_ref().expect("validated PHI").phi_inputs;
                        if inputs.iter().all(|i| i.value_id == inputs[0].value_id) {
                            continue;
                        }
                        let ty = expression_type(
                            self.program,
                            &self.program.values[&phi.result_value_id].type_id,
                        )
                        .expect("validated PHI type");
                        let value = self
                            .backend
                            .arena
                            .symbol(
                                ty.clone(),
                                format!("loopvar:{}:{}", phi.result_value_id, self.next_memory_id),
                            )
                            .expect("valid loop symbol");
                        if let (
                            Sort::BitVec { width, .. },
                            Some(ske_abstract::Interval::Range { lo, hi }),
                        ) = (&ty.sort, self.intervals.get(&phi.result_value_id))
                        {
                            let mask = u128::MAX >> (128 - width);
                            for (bound, op) in [(*lo, BinaryOp::Ge), (*hi, BinaryOp::Le)] {
                                let constant = intern(
                                    &mut self.backend.arena,
                                    ty.clone(),
                                    ExprKind::Const(Constant::Bits(bound as u128 & mask)),
                                )
                                .expect("valid interval bound");
                                let guard =
                                    binary(&mut self.backend.arena, op, value, constant, bool_ty())
                                        .expect("valid interval constraint");
                                Arc::make_mut(&mut s.constraints).push(guard);
                            }
                        }
                        Arc::make_mut(&mut s.env).insert(phi.result_value_id.clone(), value);
                    }
                    s.soundness = s.soundness.max(Soundness::Overapprox);
                    s.memory.havoc_all_memory();
                    s.memory_id = self.next_memory_id;
                    self.next_memory_id += 1;
                    self.assumption("LOOP_INTERVAL_ABSTRACTION");
                    self.diagnostic("LOOP_ABSTRACTED");
                } else {
                    self.cut("LOOP_BOUND_REACHED");
                    return;
                }
            }
        }
        if let Some(outcome) = &e.outcome_id {
            if !self.state_budget() {
                return;
            }
            s.path = s.path.push(outcome.clone());
        }
        s.phi_env = s.env.clone();
        s.predecessor = Some(s.block);
        s.block = e.definition.target_block_id.clone();
        s.instruction = 0;
        queue.push_back(s);
    }
    fn guard(
        &mut self,
        s: &State,
        e: &Edge,
        all: &[Edge],
        ins: &proto::InstructionDef,
    ) -> Result<ExprId, String> {
        let a = &mut self.backend.arena;
        match e.definition.edge_kind.as_str() {
            "BRANCH_TRUE" | "BRANCH_FALSE" => {
                let id = *s
                    .env
                    .get(&ins.operand_value_ids[0])
                    .ok_or("missing guard")?;
                if e.definition.edge_kind == "BRANCH_FALSE" {
                    not(a, id)
                } else {
                    Ok(id)
                }
            }
            "SWITCH_CASE" | "SWITCH_DEFAULT" => {
                let default = e.definition.edge_kind == "SWITCH_DEFAULT";
                let mut result = constant(a, default)?;
                let tag = *s
                    .env
                    .get(&ins.operand_value_ids[0])
                    .ok_or("missing switch tag")?;
                let cases: Vec<_> = if default {
                    all.iter()
                        .flat_map(|e| e.definition.case_value_ids.iter())
                        .collect()
                } else {
                    e.definition.case_value_ids.iter().collect()
                };
                for case in cases {
                    let c = *s.env.get(case).ok_or("missing switch constant")?;
                    let eq = binary(
                        a,
                        if default { BinaryOp::Ne } else { BinaryOp::Eq },
                        tag,
                        c,
                        bool_ty(),
                    )?;
                    result = binary(
                        a,
                        if default { BinaryOp::And } else { BinaryOp::Or },
                        result,
                        eq,
                        bool_ty(),
                    )?;
                }
                Ok(result)
            }
            _ => constant(a, true),
        }
    }
}

/// The event callback runs before each corresponding record is retained in the report.
/// Cancellation is checked between instructions; a solver call obeys its own wall timeout.
pub fn analyze(
    program: &Program,
    budget: &Budget,
    solver: &mut dyn Solver,
    initial_constraints: impl FnOnce(
        &mut ExprArena,
        &BTreeMap<String, ExprId>,
    ) -> Result<Vec<ExprId>, String>,
    cancelled: &(dyn Fn() -> bool + Sync),
    emit: &mut dyn FnMut(&Event),
) -> Result<Report, String> {
    analyze_with_summaries(
        program,
        budget,
        solver,
        initial_constraints,
        cancelled,
        emit,
        None,
    )
}

/// Apply complete scalar callee summaries resolved by the application layer.
/// Keys are call instruction IDs, not guessed function names. `None` retains
/// the v0.3 call semantics exactly.
#[allow(clippy::too_many_arguments)]
pub fn analyze_with_summaries(
    program: &Program,
    budget: &Budget,
    solver: &mut dyn Solver,
    initial_constraints: impl FnOnce(
        &mut ExprArena,
        &BTreeMap<String, ExprId>,
    ) -> Result<Vec<ExprId>, String>,
    cancelled: &(dyn Fn() -> bool + Sync),
    emit: &mut dyn FnMut(&Event),
    summaries: Option<&BTreeMap<String, crate::summary::FunctionSummary>>,
) -> Result<Report, String> {
    if !matches!(budget.loop_mode.as_str(), "unroll" | "intervals") {
        return Err("loop_mode must be unroll or intervals".into());
    }
    let instructions: Vec<_> = program
        .blocks
        .values()
        .flat_map(|b| b.instructions.iter().cloned())
        .collect();
    let intervals_safe = !instructions.iter().any(|i| {
        matches!(
            i.opcode.as_str(),
            "ALLOC" | "NEW_OBJECT" | "ADDRESS_OF" | "FIELD_ADDR" | "INDEX_ADDR" | "MAKE_CLOSURE"
        )
    });
    let intervals = if budget.loop_mode == "intervals" {
        ske_abstract::cir::infer(&program.types, &program.values, &instructions)?
    } else {
        BTreeMap::new()
    };
    let mut r = Runner {
        program,
        budget,
        solver,
        emit,
        cancelled,
        cache: BTreeMap::new(),
        field_addresses: BTreeMap::new(),
        next_memory_id: 1,
        intervals,
        intervals_safe,
        backend: Backend {
            arena: ExprArena::new(),
            havoc: 0,
        },
        report: Report {
            arena: ExprArena::new(),
            composed_calls: BTreeSet::new(),
            global_reads: BTreeMap::new(),
            tuple_extractions: BTreeSet::new(),
            summary_unknowns: BTreeSet::new(),
            arrivals: vec![],
            queries: vec![],
            exits: vec![],
            diagnostics: BTreeSet::new(),
            assumptions: BTreeSet::new(),
            exploration_closed: true,
            cut_count: 0,
            states_created: 1,
            instructions: 0,
            status: RunStatus::Completed,
            outcomes: BTreeMap::new(),
        },
    };
    for a in [
        "SEQUENTIAL_EXECUTION",
        "CLOSED_ENTRY_CONTEXT",
        "CALLEE_PANIC_EXITS_NOT_EXPLORED",
    ] {
        r.assumption(a);
    }
    let mut env = BTreeMap::new();
    // Non-machine constants (strings, opaque bytes) and unencodable values
    // become fresh HAVOC; they are an overapproximation, never EXACT.
    let mut initial_soundness = Soundness::Exact;
    for (id, v) in &program.values {
        let ty = expression_type(program, &v.type_id)?;
        let expr = match v.role.as_str() {
            "PARAM" => intern(
                &mut r.backend.arena,
                ty,
                ExprKind::Symbol(format!(
                    "param:{}:{}",
                    program.function.id,
                    v.param_index.ok_or("parameter index missing")?
                )),
            )?,
            "GLOBAL" => intern(
                &mut r.backend.arena,
                ty,
                ExprKind::Symbol(format!(
                    "init:global:{}@e0",
                    program.symbols.get(&v.symbol_id).unwrap_or(&v.symbol_id)
                )),
            )?,
            "CONST" => {
                let representation = v
                    .literal
                    .as_ref()
                    .and_then(|l| l.representation.as_ref())
                    .ok_or("constant missing literal")?;
                let c = match representation {
                    proto::literal_value::Representation::BoolValue(b) => Constant::Bool(*b),
                    proto::literal_value::Representation::NullValue(true) => Constant::Null,
                    proto::literal_value::Representation::MachineBitsBe(bytes) => {
                        if !matches!(ty.sort, Sort::BitVec { .. }) {
                            // Floats and other non-bitvector MachineBitsBe
                            // literals are not §4.9 encodable: HAVOC + OPAQUE.
                            initial_soundness = initial_soundness.max(Soundness::Opaque);
                            r.diagnostic("OPAQUE_OP_EMITTED");
                            env.insert(id.clone(), r.fresh(ty)?);
                            continue;
                        }
                        if bytes.len() > 16 {
                            return Err("bitvector wider than 128 bits unsupported by arena".into());
                        }
                        Constant::Bits(bytes.iter().fold(0u128, |n, b| (n << 8) | u128::from(*b)))
                    }
                    _ => {
                        // StringBytes and other non-machine representations are
                        // not functionally encoded (spec §4.4/§6.4): HAVOC + OPAQUE.
                        initial_soundness = initial_soundness.max(Soundness::Opaque);
                        r.diagnostic("OPAQUE_OP_EMITTED");
                        let expr = r.fresh(ty)?;
                        env.insert(id.clone(), expr);
                        continue;
                    }
                };
                intern(&mut r.backend.arena, ty, ExprKind::Const(c))?
            }
            _ => continue,
        };
        env.insert(id.clone(), expr);
    }
    let constraints = initial_constraints(&mut r.backend.arena, &env)?;
    let mut pointers = BTreeMap::new();
    for (id, v) in &program.values {
        let t = &program.types[&v.type_id];
        if matches!(t.kind.as_str(), "POINTER" | "REFERENCE") {
            if let Some(expr) = env.get(id) {
                let loc = if v.role == "GLOBAL" {
                    Loc::Global(v.symbol_id.clone())
                } else {
                    Loc::ParamPointee(program.function.id.clone(), v.param_index.unwrap_or(0))
                };
                pointers.insert(*expr, Pointer::new(expr.0 as u64, [loc], Precision::MaySet));
            }
        }
    }
    let mut queue = VecDeque::from([State {
        block: program.function.entry_block_id.clone(),
        instruction: 0,
        phi_env: Arc::new(env.clone()),
        predecessor: None,
        env: Arc::new(env),
        tuples: Arc::new(BTreeMap::new()),
        partial: false,
        residual_return: false,
        abstract_headers: Arc::new(BTreeSet::new()),
        constraints: Arc::new(constraints),
        back_edges: Arc::new(BTreeMap::new()),
        path: Path(None),
        soundness: initial_soundness,
        memory: MemoryState::default(),
        pointers: Arc::new(pointers),
        memory_id: 0,
    }]);
    let retreating = program.retreating_edges();
    let start = Instant::now();
    let mut seen = BTreeSet::new();
    'exploration: while let Some(mut s) = queue.pop_front() {
        if cancelled() {
            r.report.exploration_closed = false;
            r.report.status = RunStatus::Cancelled;
            break;
        }
        // Conservative deterministic accounting includes all arena nodes and pending maps.
        let estimated = queue
            .iter()
            .map(|s| s.env.len() * 64 + s.constraints.len() * 8 + 256)
            .sum::<usize>()
            + r.backend.arena.iter().count() * 256;
        if start.elapsed().as_millis() >= u128::from(budget.max_wall_time_ms)
            || r.report.instructions >= budget.max_instructions
            || estimated as u64 > budget.max_memory_mb.saturating_mul(1024 * 1024)
        {
            // §6.5: a budget break cuts the whole remaining worklist (and
            // alone is the only cut assembled at once), so it must not also
            // bump cut_count a second time.
            r.report.cut_count += queue.len() as u64;
            r.report.exploration_closed = false;
            r.diagnostic("BUDGET_EXHAUSTED");
            break;
        }
        // Exact snapshots only. Provenance is retained as well: source activation
        // cases distinguish histories even when the current SSA state agrees.
        let key = (
            s.block.clone(),
            s.instruction,
            s.predecessor.clone(),
            s.env
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect::<Vec<_>>(),
            s.phi_env
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect::<Vec<_>>(),
            (*s.constraints).clone(),
            (*s.back_edges).clone(),
            s.memory_id,
            (s.soundness, s.partial, s.residual_return),
            s.path.ordered(),
            ((*s.tuples).clone(), (*s.abstract_headers).clone()),
        );
        if !seen.insert(key) {
            continue;
        }
        let block = program.blocks.get(&s.block).ok_or("missing block")?;
        let ins = block
            .instructions
            .get(s.instruction)
            .ok_or("missing instruction")?;
        r.report.instructions += 1;
        if ins.is_terminator {
            if ins.opcode == "INVOKE" {
                s.memory_id = r.next_memory_id;
                r.next_memory_id += 1;
                if let Some(v) = program.values.get(&ins.result_value_id) {
                    let expr = r.fresh(expression_type(program, &v.type_id)?)?;
                    Arc::make_mut(&mut s.env).insert(ins.result_value_id.clone(), expr);
                }
                let mutates = match ins.payload.as_ref().and_then(|p| p.details.as_ref()) {
                    Some(Details::Call(c)) => c.may_mutate_memory.unwrap_or(true),
                    _ => true,
                };
                if let Some(Details::Call(c)) =
                    ins.payload.as_ref().and_then(|p| p.details.as_ref())
                {
                    if c.may_mutate_memory == Some(false) {
                        r.assumption("ADAPTER_EFFECT_CLAIMS_TRUSTED");
                    }
                }
                if mutates {
                    s.memory.havoc_all_memory();
                }
            }
            match ins.opcode.as_str() {
                "RETURN" | "THROW" | "PANIC" | "UNREACHABLE" => {
                    r.exit(
                        &s,
                        ins,
                        if ins.opcode == "UNREACHABLE" {
                            "UNREACHABLE_EXIT"
                        } else {
                            &ins.opcode
                        },
                    )?;
                }
                "JUMP" => {
                    let e = block.edges.first().ok_or("missing jump edge")?;
                    r.follow(s, e, &retreating, &mut queue);
                }
                "BRANCH" | "SWITCH" | "INVOKE" => {
                    let mut edges = block.edges.clone();
                    edges.sort_by_key(|e| e.definition.outcome_ordinal);
                    if ins.opcode == "INVOKE" {
                        s.soundness = s.soundness.max(Soundness::Opaque);
                        r.diagnostic("EXCEPTION_FLOW_PARTIAL");
                    }
                    for e in &edges {
                        if e.definition.edge_kind == "EXCEPTION" {
                            if let Some(Details::Call(call)) =
                                ins.payload.as_ref().and_then(|p| p.details.as_ref())
                            {
                                if !call.may_raise.unwrap_or(true) {
                                    r.assumption("ADAPTER_EFFECT_CLAIMS_TRUSTED");
                                    continue;
                                }
                            }
                        }
                        let g = r.guard(&s, e, &edges, ins)?;
                        let (child, remainder) = s.memory.fork();
                        s.memory = remainder;
                        let mut t = s.clone();
                        t.memory = child;
                        t.memory_id = r.next_memory_id;
                        r.next_memory_id += 1;
                        Arc::make_mut(&mut t.constraints).push(g);
                        let (query_id, result) = r.query(&t.constraints)?;
                        if let Some(outcome) = &e.outcome_id {
                            let arrival = Arrival {
                                outcome_id: outcome.clone(),
                                query_id,
                                result: result.clone(),
                                soundness: s.soundness,
                                constraints: (*t.constraints).clone(),
                                taken_outcomes: s.path.push(outcome.clone()),
                            };
                            (r.emit)(&Event::Arrival(arrival.clone()));
                            r.report.arrivals.push(arrival);
                        }
                        if !matches!(result, CheckResult::Unsat) {
                            r.follow(t, e, &retreating, &mut queue);
                        }
                    }
                }
                _ => {
                    r.report.exploration_closed = false;
                    r.diagnostic("UNSUPPORTED_TERMINATOR");
                    for edge in &block.edges {
                        if let Some(outcome) = &edge.outcome_id {
                            let arrival = Arrival {
                                outcome_id: outcome.clone(),
                                query_id: None,
                                result: CheckResult::Unknown {
                                    reason: "UNSUPPORTED_TERMINATOR".into(),
                                },
                                soundness: Soundness::Unsupported,
                                constraints: (*s.constraints).clone(),
                                taken_outcomes: s.path.push(outcome.clone()),
                            };
                            (r.emit)(&Event::Arrival(arrival.clone()));
                            r.report.arrivals.push(arrival);
                        }
                    }
                }
            }
            continue;
        }
        if ins.opcode == "CALL" {
            let summary = summaries.and_then(|all| all.get(&ins.id));
            if let Some(summary) = summary {
                r.report
                    .summary_unknowns
                    .extend(summary.unknowns.iter().cloned());
                if !summary.composable && summary.precision == crate::summary::Precision::Partial {
                    s.partial = true;
                }
            }
            if let Some(summary) = summary.filter(|summary| {
                summary.covers_inputs
                    && summary.composable
                    && summary.cases.iter().all(|case| case.kind == "RETURN")
            }) {
                if !matches!(ins.payload.as_ref().and_then(|p| p.details.as_ref()),
                    Some(Details::Call(call)) if call.dispatch_kind == "STATIC" && call.world_closed)
                {
                    return Err("summary application requires closed static target".into());
                }
                if ins.operand_value_ids.len() != summary.parameter_symbols.len() + 1 {
                    return Err("summary argument arity mismatch".into());
                }
                let mut parameters: BTreeMap<String, ExprId> = summary
                    .parameter_symbols
                    .iter()
                    .cloned()
                    .zip(
                        ins.operand_value_ids[1..]
                            .iter()
                            .map(|id| {
                                s.env
                                    .get(id)
                                    .copied()
                                    .ok_or_else(|| format!("missing argument {id}"))
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    )
                    .collect();
                let namespace = format!("summary:{}:{}", ins.id, r.next_memory_id);
                r.next_memory_id += 1;
                r.report.composed_calls.insert(ins.id.clone());
                for (symbol, global, ty) in &summary.global_reads {
                    let value = program
                        .values
                        .values()
                        .find(|v| v.role == "GLOBAL" && &v.symbol_id == global)
                        .ok_or("callee global not present in caller unit")?;
                    let address = s.env.get(&value.id).ok_or("global address missing")?;
                    let pointer = s.pointers.get(address).ok_or("global pointer missing")?;
                    let loaded = s.memory.load(&mut r.backend, pointer, ty);
                    parameters.insert(symbol.clone(), loaded);
                    for (global, expression) in s.memory.summary_global_reads() {
                        r.report.global_reads.insert(expression, global);
                    }
                }
                let mut memo = BTreeMap::new();
                for assumption in &summary.assumptions {
                    r.assumption(assumption);
                }
                for case in &summary.cases {
                    if cancelled() {
                        r.report.exploration_closed = false;
                        r.report.status = RunStatus::Cancelled;
                        break 'exploration;
                    }
                    let (child, rest) = s.memory.fork();
                    s.memory = rest;
                    let mut t = s.clone();
                    t.memory = child;
                    t.memory_id = r.next_memory_id;
                    r.next_memory_id += 1;
                    for guard in &case.constraints {
                        let guard = crate::summary::substitute(
                            &summary.arena,
                            &mut r.backend.arena,
                            *guard,
                            &parameters,
                            &namespace,
                            &mut memo,
                        )?;
                        Arc::make_mut(&mut t.constraints).push(guard);
                    }
                    let (_, answer) = r.query(&t.constraints)?;
                    if matches!(answer, CheckResult::Unsat) {
                        continue;
                    }
                    if !r.state_budget() {
                        break;
                    }
                    t.soundness = t.soundness.max(case.soundness);
                    t.partial |= case.partial;
                    // At the recursive bound, the remainder of this activation
                    // is one residual return H. Outer activations still compose
                    // their arithmetic around that return (§19 Fact table).
                    t.residual_return |= summary.residual;
                    if !matches!(answer, CheckResult::Sat { .. }) {
                        t.soundness = t.soundness.max(Soundness::Opaque);
                    }
                    let returns = case
                        .values
                        .iter()
                        .map(|id| {
                            crate::summary::substitute(
                                &summary.arena,
                                &mut r.backend.arena,
                                *id,
                                &parameters,
                                &namespace,
                                &mut memo,
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    for (location, expression) in &case.memory_delta {
                        let Loc::Global(global) = location else {
                            return Err("unmodeled callee write".into());
                        };
                        let definition = program
                            .values
                            .values()
                            .find(|v| v.role == "GLOBAL" && &v.symbol_id == global)
                            .ok_or("callee write global absent")?;
                        let address = t
                            .env
                            .get(&definition.id)
                            .ok_or("callee write global address absent")?;
                        let pointer = t
                            .pointers
                            .get(address)
                            .ok_or("callee write global pointer absent")?;
                        let written = crate::summary::substitute(
                            &summary.arena,
                            &mut r.backend.arena,
                            *expression,
                            &parameters,
                            &namespace,
                            &mut memo,
                        )?;
                        t.memory.store(pointer, written);
                    }
                    if !ins.result_value_id.is_empty() {
                        let value = if returns.len() == 1 {
                            returns[0]
                        } else {
                            let ty = expression_type(
                                program,
                                &program.values[&ins.result_value_id].type_id,
                            )?;
                            let marker = r.fresh(ty)?;
                            Arc::make_mut(&mut t.tuples)
                                .insert(ins.result_value_id.clone(), returns);
                            marker
                        };
                        Arc::make_mut(&mut t.env).insert(ins.result_value_id.clone(), value);
                    }
                    for outcome in case.taken_outcomes.ordered() {
                        t.path = t.path.push(outcome);
                    }
                    t.instruction += 1;
                    queue.push_back(t);
                }
                continue;
            }
            if summaries.is_some()
                && summary.is_none()
                && matches!(ins.payload.as_ref().and_then(|p| p.details.as_ref()),
                    Some(Details::Call(call)) if call.dispatch_kind != "STATIC" || !call.world_closed)
            {
                r.diagnostic("CALL_TARGET_UNRESOLVED");
            }
        }
        let operands = ins
            .operand_value_ids
            .iter()
            .map(|id| {
                s.env
                    .get(id)
                    .copied()
                    .ok_or_else(|| format!("missing operand {id}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let result_ty = program
            .values
            .get(&ins.result_value_id)
            .map(|v| expression_type(program, &v.type_id))
            .transpose()?;
        let mut result = None;
        // M1 reads may extend the stable initial-read/alias cache, and all
        // effectful instructions may change memory; give their snapshots IDs.
        if matches!(
            ins.opcode.as_str(),
            "ALLOC"
                | "NEW_OBJECT"
                | "ADDRESS_OF"
                | "FIELD_ADDR"
                | "LOAD"
                | "STORE"
                | "CALL"
                | "DEFER_PUSH"
                | "RUN_DEFERS"
                | "RECOVER"
                | "UNKNOWN_EFFECT"
                | "OPAQUE_OP"
                | "SPAWN"
                | "JOIN"
                | "CHANNEL_SEND"
                | "CHANNEL_RECV"
                | "LOCK"
                | "UNLOCK"
                | "EMIT_EFFECT"
                | "INSERT_FIELD"
        ) {
            s.memory_id = r.next_memory_id;
            r.next_memory_id += 1;
        }
        // Implicit panic candidates are queried before the normal successor, just like IR edges.
        if matches!(ins.opcode.as_str(), "DIV" | "REM" | "SHL" | "SHR") {
            let rhs = operands[1];
            let ty = r
                .backend
                .arena
                .get(rhs)
                .ok_or("missing divisor")?
                .ty
                .clone();
            let negative = matches!(ins.opcode.as_str(), "SHL" | "SHR")
                && matches!(ty.sort, Sort::BitVec { signed: true, .. });
            if matches!(ins.opcode.as_str(), "DIV" | "REM") || negative {
                let zero = intern(&mut r.backend.arena, ty, ExprKind::Const(Constant::Bits(0)))?;
                let g = binary(
                    &mut r.backend.arena,
                    if negative { BinaryOp::Lt } else { BinaryOp::Eq },
                    rhs,
                    zero,
                    bool_ty(),
                )?;
                let mut panic = s.clone();
                Arc::make_mut(&mut panic.constraints).push(g);
                let (_, check) = r.query(&panic.constraints)?;
                if !matches!(check, CheckResult::Unsat) && r.state_budget() {
                    r.exit(&panic, ins, "PANIC")?;
                }
                let ng = not(&mut r.backend.arena, g)?;
                if !s
                    .constraints
                    .iter()
                    .any(|old| same_boolean_predicate(&r.backend.arena, *old, ng))
                {
                    Arc::make_mut(&mut s.constraints).push(ng);
                }
                let (_, check) = r.query(&s.constraints)?;
                if matches!(check, CheckResult::Unsat) || !r.state_budget() {
                    continue;
                }
            }
        }
        match ins.opcode.as_str() {
            "ALLOC" | "NEW_OBJECT" | "ADDRESS_OF" => {
                let ty = result_ty.clone().ok_or("allocation type")?;
                let address = r.fresh(ty)?;
                let site = program
                    .blocks
                    .values()
                    .flat_map(|b| &b.instructions)
                    .position(|i| i.id == ins.id)
                    .ok_or("allocation site missing")? as u64;
                let pointer = s.memory.allocate(site, address.0 as u64);
                if ins.opcode == "ADDRESS_OF" {
                    s.memory.store(&pointer, operands[0]);
                    s.memory.mark_escaped(&pointer);
                }
                Arc::make_mut(&mut s.pointers).insert(address, pointer);
                result = Some(address);
            }
            "FIELD_ADDR" => {
                let index = match ins.payload.as_ref().and_then(|p| p.details.as_ref()) {
                    Some(Details::Field(f)) => f.field_index,
                    _ => return Err("field payload missing".into()),
                };
                let key = (operands[0], index);
                let address = if let Some(address) = r.field_addresses.get(&key) {
                    *address
                } else {
                    let address = r.fresh(result_ty.clone().ok_or("field address type")?)?;
                    r.field_addresses.insert(key, address);
                    address
                };
                let parent = s.pointers.get(&operands[0]).cloned().unwrap_or_else(|| {
                    Pointer::new(operands[0].0 as u64, [Loc::Unknown], Precision::Unknown)
                });
                let pointer = parent.field(address.0 as u64, index);
                Arc::make_mut(&mut s.pointers).insert(address, pointer);
                result = Some(address);
            }
            "LOAD" | "STORE" => {
                let pointer = s.pointers.get(&operands[0]).cloned().unwrap_or_else(|| {
                    Pointer::new(operands[0].0 as u64, [Loc::Unknown], Precision::Unknown)
                });
                if ins.opcode == "LOAD" {
                    result = Some(s.memory.load(
                        &mut r.backend,
                        &pointer,
                        &result_ty.clone().ok_or("load type")?,
                    ));
                    for (global, expression) in s.memory.summary_global_reads() {
                        r.report.global_reads.insert(expression, global);
                    }
                    if pointer.precision == Precision::Unknown {
                        s.soundness = s.soundness.max(Soundness::Opaque);
                    }
                } else {
                    if s.memory.store(&pointer, operands[1]) == StoreEffect::Havoc {
                        s.soundness = s.soundness.max(Soundness::Opaque);
                        r.diagnostic("OPAQUE_OP_EMITTED");
                    }
                    if let Some(stored) = s.pointers.get(&operands[1]) {
                        s.memory.mark_escaped(stored);
                    }
                }
            }
            "EXTRACT_FIELD" => {
                let index = match ins.payload.as_ref().and_then(|p| p.details.as_ref()) {
                    Some(Details::Field(f)) => f.field_index,
                    _ => return Err("field payload missing".into()),
                };
                result = if let Some(values) = s.tuples.get(&ins.operand_value_ids[0]) {
                    r.report.tuple_extractions.insert(ins.id.clone());
                    Some(
                        *values
                            .get(index as usize)
                            .ok_or("summary tuple field out of range")?,
                    )
                } else {
                    Some(intern(
                        &mut r.backend.arena,
                        result_ty.clone().ok_or("field type")?,
                        ExprKind::Field {
                            base: operands[0],
                            index,
                        },
                    )?)
                };
            }
            "CALL" => {
                let call = match ins.payload.as_ref().and_then(|p| p.details.as_ref()) {
                    Some(Details::Call(c)) => c,
                    _ => return Err("call payload missing".into()),
                };
                let pure = call.is_referentially_transparent_proven
                    && !call.may_raise.unwrap_or(true)
                    && !call.may_mutate_memory.unwrap_or(true);
                if pure {
                    r.assumption("ADAPTER_EFFECT_CLAIMS_TRUSTED");
                    if let Some(ty) = result_ty.clone() {
                        let callee = ins
                            .operand_value_ids
                            .first()
                            .and_then(|v| program.values.get(v))
                            .map(|v| {
                                program
                                    .symbols
                                    .get(&v.symbol_id)
                                    .unwrap_or(&v.symbol_id)
                                    .clone()
                            })
                            .unwrap_or_else(|| "unresolved".into());
                        result = Some(intern(
                            &mut r.backend.arena,
                            ty,
                            ExprKind::Uninterpreted {
                                name: callee,
                                // Include function identity for indirect pure calls:
                                // different callees must not be conflated.
                                args: operands.clone(),
                            },
                        )?);
                    }
                } else {
                    s.soundness = s.soundness.max(Soundness::Opaque);
                    r.diagnostic("OPAQUE_OP_EMITTED");
                }
                if call.may_mutate_memory == Some(false) {
                    // We rely on the adapter's claim that this call does not
                    // mutate memory (§6.5), regardless of whether it is pure.
                    r.assumption("ADAPTER_EFFECT_CLAIMS_TRUSTED");
                }
                if call.may_mutate_memory.unwrap_or(true) {
                    s.memory.havoc_all_memory();
                }
            }
            "COPY" => result = Some(operands[0]),
            "BITCAST" => {
                result = Some(intern(
                    &mut r.backend.arena,
                    result_ty.clone().ok_or("bitcast type")?,
                    ExprKind::Unary {
                        op: UnaryOp::Cast,
                        arg: operands[0],
                    },
                )?)
            }
            "PHI" => {
                let predecessor = s.predecessor.as_ref().ok_or("PHI without predecessor")?;
                let inputs = &ins
                    .payload
                    .as_ref()
                    .ok_or("PHI payload missing")?
                    .phi_inputs;
                if s.abstract_headers.contains(&s.block)
                    && inputs.iter().any(|i| i.value_id != inputs[0].value_id)
                {
                    result = Some(
                        *s.env
                            .get(&ins.result_value_id)
                            .ok_or("abstract PHI missing")?,
                    );
                } else {
                    let input = ins
                        .payload
                        .as_ref()
                        .ok_or("PHI payload missing")?
                        .phi_inputs
                        .iter()
                        .find(|p| &p.predecessor_block_id == predecessor)
                        .ok_or("PHI predecessor missing")?;
                    result = Some(*s.phi_env.get(&input.value_id).ok_or("PHI value missing")?);
                }
            }
            "SELECT" => {
                result = Some(intern(
                    &mut r.backend.arena,
                    result_ty.clone().ok_or("select type")?,
                    ExprKind::Ite {
                        condition: operands[0],
                        then_expr: operands[1],
                        else_expr: operands[2],
                    },
                )?)
            }
            "LOGICAL_NOT" | "NEG" | "BIT_NOT" | "SIGN_EXTEND" | "ZERO_EXTEND" | "TRUNCATE" => {
                let op = match ins.opcode.as_str() {
                    "LOGICAL_NOT" => UnaryOp::Not,
                    "NEG" => UnaryOp::Neg,
                    "BIT_NOT" => UnaryOp::BitNot,
                    _ => UnaryOp::Cast,
                };
                let mut arg = operands[0];
                if matches!(ins.opcode.as_str(), "SIGN_EXTEND" | "ZERO_EXTEND") {
                    let mut input_ty = r
                        .backend
                        .arena
                        .get(arg)
                        .ok_or("extension input")?
                        .ty
                        .clone();
                    if let Sort::BitVec { signed, .. } = &mut input_ty.sort {
                        *signed = ins.opcode == "SIGN_EXTEND";
                    }
                    input_ty.local_key = format!("engine:extend:{}:{}", ins.id, ins.opcode);
                    arg = intern(
                        &mut r.backend.arena,
                        input_ty,
                        ExprKind::Unary {
                            op: UnaryOp::Cast,
                            arg,
                        },
                    )?;
                }
                result = Some(intern(
                    &mut r.backend.arena,
                    result_ty.clone().ok_or("unary type")?,
                    ExprKind::Unary { op, arg },
                )?);
            }
            "ADD" | "SUB" | "MUL" | "DIV" | "REM" | "EQ" | "NE" | "LT" | "LE" | "GT" | "GE"
            | "BIT_AND" | "BIT_OR" | "XOR" | "SHL" | "SHR" => {
                let unsupported = matches!(ins.opcode.as_str(), "ADD" | "SUB" | "MUL")
                    && !matches!(ins.payload.as_ref().and_then(|p|p.details.as_ref()),Some(Details::Binary(b)) if b.overflow_mode=="WRAP");
                if unsupported {
                    s.soundness = s.soundness.max(Soundness::Opaque);
                    r.diagnostic("UNSUPPORTED_OVERFLOW_MODE");
                } else {
                    let op = match ins.opcode.as_str() {
                        "ADD" => BinaryOp::Add,
                        "SUB" => BinaryOp::Sub,
                        "MUL" => BinaryOp::Mul,
                        "DIV" => BinaryOp::Div,
                        "REM" => BinaryOp::Rem,
                        "EQ" => BinaryOp::Eq,
                        "NE" => BinaryOp::Ne,
                        "LT" => BinaryOp::Lt,
                        "LE" => BinaryOp::Le,
                        "GT" => BinaryOp::Gt,
                        "GE" => BinaryOp::Ge,
                        "BIT_AND" => BinaryOp::BitAnd,
                        "BIT_OR" => BinaryOp::BitOr,
                        "XOR" => BinaryOp::Xor,
                        "SHL" => BinaryOp::Shl,
                        _ => BinaryOp::Shr,
                    };
                    result = Some(binary(
                        &mut r.backend.arena,
                        op,
                        operands[0],
                        operands[1],
                        result_ty.clone().ok_or("binary type")?,
                    )?);
                }
            }
            "MEMORY_BARRIER" => {}
            "UNDEF" | "HAVOC" | "LENGTH" | "GET_TAG" | "TYPE_TEST" | "MAKE_INTERFACE"
            | "MAKE_CLOSURE" => {
                s.soundness = s.soundness.max(Soundness::Opaque);
                r.diagnostic("OPAQUE_OP_EMITTED");
            }
            "INDEX" | "INDEX_ADDR" | "DOWNCAST" => {
                s.soundness = s.soundness.max(Soundness::Opaque);
                r.diagnostic("IMPLICIT_PANIC_UNMODELED");
            }
            "DEFER_PUSH" | "RUN_DEFERS" | "RECOVER" | "UNKNOWN_EFFECT" | "OPAQUE_OP" | "SPAWN"
            | "JOIN" | "CHANNEL_SEND" | "CHANNEL_RECV" | "LOCK" | "UNLOCK" | "EMIT_EFFECT"
            | "INSERT_FIELD" => {
                s.memory.havoc_all_memory();
                s.soundness = s.soundness.max(Soundness::Opaque);
                r.diagnostic(match ins.opcode.as_str() {
                    "DEFER_PUSH" | "RUN_DEFERS" => "DEFER_UNMODELED",
                    "RECOVER" => "RECOVER_UNMODELED",
                    "SPAWN" | "JOIN" | "CHANNEL_SEND" | "CHANNEL_RECV" | "LOCK" | "UNLOCK" => {
                        "CONCURRENCY_PARTIAL"
                    }
                    _ => "OPAQUE_OP_EMITTED",
                });
            }
            _ => return Err(format!("E_UNKNOWN_OPCODE: {}", ins.opcode)),
        }
        if let Some(ty) = result_ty {
            let expression = match result {
                Some(e) => e,
                None => r.fresh(ty)?,
            };
            Arc::make_mut(&mut s.env).insert(ins.result_value_id.clone(), expression);
        }
        s.instruction += 1;
        queue.push_back(s);
    }
    if r.report.exits.iter().any(|exit| exit.partial) {
        r.report.exploration_closed = false;
    }
    if r.report.status != RunStatus::Cancelled {
        r.report.status = if r.report.exploration_closed {
            RunStatus::Completed
        } else {
            RunStatus::Partial
        };
    }
    for outcome in program.outcomes() {
        let arrivals = r
            .report
            .arrivals
            .iter()
            .filter(|a| a.outcome_id == outcome)
            .collect::<Vec<_>>();
        let mut result = classify(&arrivals, r.report.exploration_closed);
        if result.status == Classification::UnreachableCandidate
            && r.report.assumptions.contains("LOOP_INTERVAL_ABSTRACTION")
        {
            result.soundness = result.soundness.max(Soundness::Overapprox);
        }
        r.report.outcomes.insert(outcome, result);
    }
    r.report.arena = r.backend.arena;
    Ok(r.report)
}

pub fn classify(arrivals: &[&Arrival], closed: bool) -> OutcomeResult {
    let worst = arrivals
        .iter()
        .map(|a| a.soundness)
        .max()
        .unwrap_or(Soundness::Exact);
    let sat = arrivals
        .iter()
        .filter(|a| matches!(a.result, CheckResult::Sat { .. }))
        .map(|a| a.soundness)
        .min();
    let (status, soundness, explanation) = if let Some(best) = sat {
        (Classification::Possible, best, "SAT_ARRIVAL")
    } else if arrivals
        .iter()
        .any(|a| a.soundness == Soundness::Unsupported)
    {
        (
            Classification::Unsupported,
            Soundness::Unsupported,
            "UNSUPPORTED_TERMINATOR",
        )
    } else if arrivals
        .iter()
        .any(|a| matches!(a.result, CheckResult::Timeout { .. }))
    {
        (Classification::Timeout, worst, "SOLVER_TIMEOUT")
    } else if arrivals
        .iter()
        .any(|a| matches!(a.result, CheckResult::Unknown { .. }))
    {
        (Classification::Unknown, worst, "SOLVER_UNKNOWN")
    } else if closed {
        (
            Classification::UnreachableCandidate,
            worst,
            if arrivals.is_empty() {
                "NO_ARRIVALS_CLOSED_EXPLORATION"
            } else {
                "ALL_ARRIVALS_UNSAT"
            },
        )
    } else if arrivals.is_empty() {
        (
            Classification::Unanalyzed,
            Soundness::Opaque,
            "NOT_REACHED_BUDGET_OR_CUT",
        )
    } else {
        (Classification::Unknown, worst, "EXPLORATION_NOT_CLOSED")
    };
    OutcomeResult {
        status,
        soundness,
        explanation,
    }
}
