use ske_solver::{CheckResult, Solver, SolverBudget, Z3Solver};
use std::time::Instant;

#[test]
fn chunked_parsing_preserves_declarations_and_assertions() {
    let mut smt = String::from("(declare-const x Int)\n(assert (= x 1))\n");
    for _ in 0..17_000 {
        smt.push_str("(assert (= x x))\n");
    }
    smt.push_str("(assert (= x 2))\n");
    assert_eq!(
        Z3Solver::new().check_interruptible(
            &ske_solver::Query {
                smtlib: smt,
                variables: vec![],
            },
            &SolverBudget {
                rlimit: 5_000_000,
                wall_timeout_ms: 0,
                seed: 0,
            },
            &|| false,
        ),
        CheckResult::Unsat
    );
}

#[test]
fn large_scripts_preserve_definitions_multiline_terms_and_push_pop() {
    for (prefix, suffix) in [
        (
            "(declare-const x Int)\n(define-fun y () Int (+ x 1))\n(assert (= x 1))\n",
            "(assert (= y 3))\n",
        ),
        (
            "(declare-const x Int)\n(assert\n (= x 1))\n",
            "(assert (= x 2))\n",
        ),
        (
            "(declare-const x Int)\n(push 1)\n(assert (= x 1))\n",
            "(pop 1)\n(assert (= x 2))\n(assert (= x 3))\n",
        ),
    ] {
        let text = format!("{prefix}{}{suffix}", "(assert true)\n".repeat(17_000));
        let query = ske_solver::Query {
            smtlib: text,
            variables: vec![],
        };
        let budget = SolverBudget {
            rlimit: 5_000_000,
            wall_timeout_ms: 0,
            seed: 0,
        };
        assert_eq!(Z3Solver::new().check(&query, &budget), CheckResult::Unsat);
        assert_eq!(
            Z3Solver::new().check_interruptible(&query, &budget, &|| false),
            CheckResult::Unsat
        );
    }
}

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

// The watchdog must cover the whole native call, not only solver.check():
// parsing a large SMT-LIB input is neither interruptible nor bounded by Z3's
// own timeout, and the engine's SMT text grows with each loop unrolling. A
// SIGINT arriving during that parse previously left the analyzer unresponsive
// until the parse finished on its own (here ~6s for 1.5M assertions).
#[test]
fn cancellation_interrupts_long_parse() {
    let mut smt = String::from("(set-logic ALL)\n(declare-fun x () (_ BitVec 64))\n");
    for i in 0..1_500_000 {
        smt.push_str(&format!("(assert (= (bvadd x (_ bv{} 64)) x))\n", i % 16));
    }
    smt.push_str("(check-sat)\n");
    let mut solver = Z3Solver::new();
    let started = Instant::now();
    // Cancel 50ms in: parsing has begun but still has seconds to go on the
    // uncancelled path. A prompt return proves the parser itself was aborted
    // rather than its check afterwards.
    let cancelled = || started.elapsed() >= std::time::Duration::from_millis(50);
    let result = solver.check_interruptible(
        &ske_solver::Query {
            smtlib: smt,
            variables: vec![],
        },
        &SolverBudget {
            rlimit: 5_000_000,
            wall_timeout_ms: 0,
            seed: 0,
        },
        &cancelled,
    );
    let elapsed = started.elapsed();
    println!("result={result:?} elapsed={elapsed:?}");
    assert!(
        elapsed.as_secs() < 5,
        "cancellation must not wait for the parser: {elapsed:?}"
    );
    match result {
        CheckResult::Unknown { .. } | CheckResult::Timeout { .. } => {}
        other => panic!("expected interrupt/unknown, got {other:?}"),
    }
}
