#![cfg(unix)]

use ske_solver::{Query, Solver, SolverBudget, Z3Solver};
use std::{
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

fn sigint_handler() -> libc::sighandler_t {
    // Query only: do not change the process-wide signal disposition.
    unsafe {
        let mut action = std::mem::zeroed();
        assert_eq!(
            libc::sigaction(libc::SIGINT, std::ptr::null(), &mut action),
            0
        );
        action.sa_sigaction
    }
}

#[test]
fn solving_preserves_application_sigint_handler() {
    // Pigeonhole: keep a real native check active while the watchdog observes
    // the signal disposition. This test has its own integration-test process.
    let mut text = String::from("(set-logic QF_UF)\n");
    for pigeon in 0..25 {
        for hole in 0..24 {
            text.push_str(&format!("(declare-const p{pigeon}_{hole} Bool)\n"));
        }
    }
    for pigeon in 0..25 {
        let choices = (0..24)
            .map(|hole| format!("p{pigeon}_{hole}"))
            .collect::<Vec<_>>()
            .join(" ");
        text.push_str(&format!("(assert (or {choices}))\n"));
    }
    for hole in 0..24 {
        for left in 0..25 {
            for right in left + 1..25 {
                text.push_str(&format!(
                    "(assert (or (not p{left}_{hole}) (not p{right}_{hole})))\n"
                ));
            }
        }
    }
    let original = sigint_handler();
    let changed = AtomicBool::new(false);
    let observations = AtomicUsize::new(0);
    let start = Instant::now();
    let result = Z3Solver::new().check_interruptible(
        &Query {
            smtlib: text,
            variables: vec![],
        },
        &SolverBudget {
            rlimit: u32::MAX.into(),
            wall_timeout_ms: 2_000,
            seed: 0,
        },
        &|| {
            observations.fetch_add(1, Ordering::Relaxed);
            if sigint_handler() != original {
                changed.store(true, Ordering::Relaxed);
            }
            start.elapsed() >= Duration::from_millis(200)
        },
    );
    assert!(start.elapsed() >= Duration::from_millis(200), "{result:?}");
    assert!(observations.load(Ordering::Relaxed) > 5);
    assert!(
        !changed.load(Ordering::Relaxed),
        "Z3 replaced the application's SIGINT handler during solving"
    );
    assert_eq!(sigint_handler(), original);
}
