#![cfg(unix)]
mod go_support;
use go_support::{count, Lab};
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Process(Option<Child>);
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn cancel_sigint_preserves_streamed_arrivals() {
    let lab = Lab::new();
    let budget = lab.dir.path().join("long.toml");
    std::fs::write(
        &budget,
        "[budget]\nloop_unroll=1000000\nmax_states=1000000\nmax_solver_queries=1000000\nmax_instructions=1000000\nmax_wall_time_ms=60000\nsolver_wall_timeout_ms=500\n",
    )
    .unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_ske"))
        .env_remove("SKE_DB")
        .args([
            "branches",
            "analyze",
            "--entry",
            "example.Sum",
            "--format",
            "json",
            "--db",
        ])
        .arg(&lab.db)
        .arg("--budget")
        .arg(&budget)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut process = Process(Some(child));
    let started = Instant::now();
    loop {
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "analyzer never persisted arrivals"
        );
        assert!(
            process.0.as_mut().unwrap().try_wait().unwrap().is_none(),
            "analysis finished before cancellation"
        );
        if count(&lab.conn(), "SELECT COUNT(*) FROM outcome_arrivals") >= 3 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let before = count(&lab.conn(), "SELECT COUNT(*) FROM outcome_arrivals");
    let pid = process.0.as_ref().unwrap().id().to_string();
    assert!(Command::new("kill")
        .args(["-INT", &pid])
        .status()
        .unwrap()
        .success());
    let signalled = Instant::now();
    let mut sampled = false;
    while process.0.as_mut().unwrap().try_wait().unwrap().is_none() {
        if !sampled && signalled.elapsed() > Duration::from_secs(45) {
            // Capture a stack while still alive: macOS `sample` is best-effort.
            sampled = true;
            let path = lab.dir.path().join(format!("hang-{pid}.txt"));
            let _ = Command::new("sample")
                .args([&pid, "3", "-file"])
                .arg(&path)
                .status();
            eprintln!(
                "cancel_sigint: analyzer still alive after 45s; sample at {:?}",
                path
            );
        }
        assert!(
            signalled.elapsed() < Duration::from_secs(120),
            "SIGINT did not stop analyzer within solver safety timeout"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = process.0.take().unwrap().wait_with_output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(130),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["run_status"], "CANCELLED");
    assert_eq!(value["exploration_closed"], false);
    assert_eq!(
        count(
            &lab.conn(),
            "SELECT COUNT(*) FROM analysis_runs WHERE status='CANCELLED'"
        ),
        1
    );
    assert!(count(&lab.conn(), "SELECT COUNT(*) FROM outcome_arrivals") >= before);
    assert_eq!(
        count(
            &lab.conn(),
            "SELECT COUNT(*) FROM proof_records WHERE verdict='ACCEPTED'"
        ),
        0
    );
    lab.ok(&["db", "check"]);
}
