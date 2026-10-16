//! Backend-independent SMT-LIB queries and real, per-query Z3 solving.
mod backend;
pub mod encoding;

pub use backend::{map_unknown_reason, ScriptedSolver, Z3Solver};
pub use encoding::{EncodeError, EncodedOp, Encoder, OpOptions, Sort, Term, Variable};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SolverBudget {
    pub rlimit: u64,
    pub wall_timeout_ms: u64,
    pub seed: u32,
}
impl Default for SolverBudget {
    fn default() -> Self {
        Self {
            rlimit: 5_000_000,
            wall_timeout_ms: 10_000,
            seed: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeoutKind {
    Rlimit,
    WallClock,
    Budget,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelValue {
    Bool(bool),
    BitVec { width: u32, bits: u128 },
    Uninterpreted(String),
}
pub type Model = BTreeMap<String, ModelValue>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckResult {
    Sat { model: Option<Model> },
    Unsat,
    Unknown { reason: String },
    Timeout { kind: TimeoutKind },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub smtlib: String,
    /// Only these declarations are projected into the returned model.
    pub variables: Vec<Variable>,
}
pub trait Solver {
    fn check(&mut self, query: &Query, budget: &SolverBudget) -> CheckResult;
    fn check_smtlib(&mut self, smtlib: &str, budget: &SolverBudget) -> CheckResult {
        self.check(
            &Query {
                smtlib: smtlib.into(),
                variables: vec![],
            },
            budget,
        )
    }
    /// Variant used by the engine: a stale solve may be abandoned when
    /// `cancelled` turns true (or the wall budget elapses), so a SIGINT/SIGTERM
    /// is observed promptly instead of waiting for Z3's own timeout.
    fn check_interruptible(
        &mut self,
        query: &Query,
        budget: &SolverBudget,
        _cancelled: &(dyn Fn() -> bool + Sync),
    ) -> CheckResult {
        self.check(query, budget)
    }
    fn name(&self) -> &str;
    fn version(&self) -> String;
}

/// §6.4: wall-clock safety timeout deliberately does not enter the fingerprint.
pub fn query_fingerprint(smtlib: &str, budget: &SolverBudget) -> String {
    let input = format!("{smtlib}|seed={}|rlimit={}", budget.seed, budget.rlimit);
    Sha256::digest(input.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
