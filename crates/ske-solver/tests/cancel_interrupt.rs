use ske_solver::{CheckResult, Solver, SolverBudget, Z3Solver};
use std::time::Instant;

fn hard_smt() -> String {
    let mut s = String::from("(set-logic ALL)\n(declare-fun x () (_ BitVec 64))\n");
    for i in 0..200 {
        s.push_str(&format!(
            "(assert (not (= (bvmul x (_ bv{} 64)) (_ bv1 64))))\n",
            i
        ));
    }
    s.push_str("(check-sat)\n");
    s
}

// §6.4/§6.5: a cancelled run must abandon a stale solve promptly instead of
// waiting for Z3's own timeout, which is only honored at tactic boundaries.
#[test]
fn cancel_interrupts_solver_promptly() {
    let mut solver = Z3Solver::new();
    let budget = SolverBudget {
        rlimit: 5_000_000,
        wall_timeout_ms: 0,
        seed: 0,
    };
    let start = Instant::now();
    let result = solver.check_interruptible(
        &ske_solver::Query {
            smtlib: hard_smt(),
            variables: vec![],
        },
        &budget,
        &|| true, // immediately cancelled
    );
    let elapsed = start.elapsed();
    println!("result={result:?} elapsed={elapsed:?}");
    assert!(
        elapsed.as_millis() < 500,
        "interrupt did not fire: {elapsed:?}"
    );
    match result {
        CheckResult::Unknown { .. } | CheckResult::Timeout { .. } => {}
        other => panic!("expected interrupt/unknown, got {other:?}"),
    }
}
