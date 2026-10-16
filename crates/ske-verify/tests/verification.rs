use rusqlite::params;
use ske_verify::{verify_unreachability, ReplayResult};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

fn replay(text: &str, seed: u32) -> ReplayResult {
    assert_eq!(seed, 1);
    let mut child = Command::new("z3")
        .args(["-in", "-smt2"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("real Z3 required");
    write!(
        child.stdin.take().unwrap(),
        "(set-option :random-seed {seed})\n{text}"
    )
    .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    match String::from_utf8(out.stdout).unwrap().trim() {
        "unsat" => ReplayResult::Unsat,
        "sat" => ReplayResult::Sat,
        other => panic!("unexpected solver output {other}"),
    }
}

// Manual Decide path formulas, not produced by the verifier or engine.
const UNSAT: &str = "(set-logic QF_BV)\n(declare-fun x () (_ BitVec 64))\n(assert (not (bvslt x (_ bv0 64))))\n(assert (bvslt x (_ bv18446744073709551606 64)))\n(check-sat)\n";
const SAT: &str = "(set-logic QF_BV)\n(declare-fun x () (_ BitVec 64))\n(assert (bvslt x (_ bv0 64)))\n(check-sat)\n";

fn database() -> (tempfile::TempDir, ske_storage::Storage, i64) {
    let dir = tempfile::tempdir().unwrap();
    let storage = ske_storage::Storage::open(dir.path().join("test.db")).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    ske_ingest::ingest_many(
        &storage,
        &[root.join("tests/fixtures/cir/decide.cir.pb")],
        Some(&root.join("examples/go-branch-lab")),
        None,
    )
    .unwrap();
    let db = &storage.conn;
    db.execute_batch(
        "INSERT INTO analysis_runs(id,snapshot_id,entry_function_id,analysis_kind,
        engine_version,config_fingerprint,assumptions_fingerprint,status,exploration_closed)
        VALUES(1,1,1,'BRANCH_ANALYSIS','test','test','test','COMPLETED',1);
        INSERT INTO analysis_assumptions(analysis_run_id,kind,description) VALUES
        (1,'SEQUENTIAL_EXECUTION','test'),(1,'CLOSED_ENTRY_CONTEXT','test');",
    )
    .unwrap();
    let outcome = db
        .query_row(
            "SELECT o.id FROM ir_outcomes o JOIN ir_decisions d ON d.id=o.decision_id
        ORDER BY d.id DESC,o.ordinal LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    for id in 1..=4 {
        db.execute(
            "INSERT INTO solver_queries(id,analysis_run_id,query_fingerprint,smtlib,result)
            VALUES(?1,1,?2,?3,?4)",
            params![
                id,
                id.to_string(),
                if id == 4 { UNSAT } else { SAT },
                if id == 4 { "UNSAT" } else { "SAT" }
            ],
        )
        .unwrap();
    }
    db.execute(
        "INSERT INTO outcome_arrivals(analysis_run_id,ir_outcome_id,arrival_seq,
        query_id,result,soundness) VALUES(1,?1,0,4,'UNSAT','EXACT')",
        [outcome],
    )
    .unwrap();
    (dir, storage, outcome)
}

#[test]
fn verifier_accepts_decide_proof_row() {
    let (_dir, mut s, o) = database();
    // Test-only guard establishes insertion ordering in addition to normative triggers.
    s.conn
        .execute_batch(
            "CREATE TEMP TRIGGER check_order BEFORE INSERT ON branch_analysis_results
        WHEN NEW.status='PROVEN_UNREACHABLE' AND NOT EXISTS
        (SELECT 1 FROM proof_records WHERE id=NEW.proof_id AND verdict='ACCEPTED')
        BEGIN SELECT RAISE(ABORT,'proof not first'); END;",
        )
        .unwrap();
    let v = verify_unreachability(&mut s.conn, 1, o, replay).unwrap();
    assert!(v.accepted, "{}", v.reason);
    let row: (String, i64, String) = s
        .conn
        .query_row(
            "SELECT verdict,residual_frontier,model_soundness FROM proof_records",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(row, ("ACCEPTED".into(), 0, "EXACT".into()));
    assert_eq!(
        s.conn
            .query_row("SELECT count(*) FROM solver_queries", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        4
    );
    for sql in [
        "DELETE FROM proof_records",
        "UPDATE proof_records SET reason='bad'",
        "DELETE FROM outcome_arrivals",
        "DELETE FROM solver_queries",
        "DELETE FROM analysis_assumptions",
        "UPDATE analysis_runs SET status='FAILED'",
    ] {
        assert!(s.conn.execute(sql, []).is_err(), "{sql}");
    }
}

#[test]
fn verifier_rejects_scripted_false_unsat() {
    let (_dir, mut s, o) = database();
    // Scripted producer lied about a satisfiable formula. Real solver catches it.
    s.conn
        .execute("UPDATE solver_queries SET smtlib=?1 WHERE id=4", [SAT])
        .unwrap();
    let v = verify_unreachability(&mut s.conn, 1, o, replay).unwrap();
    assert!(!v.accepted);
    let row: (String, String) = s
        .conn
        .query_row(
            "SELECT status,explanation FROM branch_analysis_results",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(row.0, "UNKNOWN");
    assert!(row.1.starts_with("PROOF_REJECTED:"));
    assert_eq!(
        s.conn
            .query_row(
                "SELECT count(*) FROM proof_records WHERE verdict='REJECTED'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn verifier_replays_all_queries_even_without_arrivals() {
    let (_dir, mut s, o) = database();
    s.conn.execute("DELETE FROM outcome_arrivals", []).unwrap();
    s.conn
        .execute("UPDATE solver_queries SET result='UNSAT' WHERE id=1", [])
        .unwrap();
    let mut calls = 0;
    let v = verify_unreachability(&mut s.conn, 1, o, |text, seed| {
        calls += 1;
        replay(text, seed)
    })
    .unwrap();
    assert_eq!(calls, 2);
    assert!(!v.accepted);
}

#[test]
fn verifier_rereads_integrity_conditions() {
    for sql in [
        "UPDATE analysis_runs SET exploration_closed=0",
        "UPDATE analysis_runs SET cut_count=1",
        "UPDATE analysis_runs SET status='PARTIAL'",
        "DELETE FROM analysis_assumptions WHERE kind='CLOSED_ENTRY_CONTEXT'",
        "UPDATE outcome_arrivals SET soundness='UNSUPPORTED'",
        "UPDATE outcome_arrivals SET result='SAT'",
        "UPDATE outcome_arrivals SET query_id=NULL",
    ] {
        let (_dir, mut s, o) = database();
        s.conn.execute(sql, []).unwrap();
        assert!(
            !verify_unreachability(&mut s.conn, 1, o, replay)
                .unwrap()
                .accepted,
            "{sql}"
        );
    }
}

#[test]
fn verifier_preserves_worst_soundness_and_empty_arrivals() {
    for precision in ["EXACT", "OVERAPPROX", "OPAQUE"] {
        let (_dir, mut s, o) = database();
        s.conn
            .execute("UPDATE outcome_arrivals SET soundness=?1", [precision])
            .unwrap();
        let v = verify_unreachability(&mut s.conn, 1, o, replay).unwrap();
        assert!(v.accepted);
        assert_eq!(v.model_soundness, precision);
    }
    let (_dir, mut s, o) = database();
    s.conn.execute("DELETE FROM outcome_arrivals", []).unwrap();
    let v = verify_unreachability(&mut s.conn, 1, o, replay).unwrap();
    assert!(v.accepted);
    assert_eq!(v.model_soundness, "EXACT");
    assert_eq!(v.reason, "NO_ARRIVALS_CLOSED_EXPLORATION");
}

#[test]
fn verifier_rejects_unknown_and_timeout_replay() {
    for result in [ReplayResult::Unknown, ReplayResult::Timeout] {
        let (_dir, mut s, o) = database();
        assert!(
            !verify_unreachability(&mut s.conn, 1, o, |_, seed| {
                assert_eq!(seed, 1);
                result
            })
            .unwrap()
            .accepted
        );
    }
}

#[test]
fn verifier_has_no_classification_dependency() {
    let manifest = include_str!("../Cargo.toml");
    for forbidden in ["ske-symbolic", "ske-classification", "ske-engine"] {
        assert!(!manifest.contains(forbidden));
    }
    // Keep the actual production implementation independently inspectable.
    assert!(!include_str!("../src/verify.rs").contains("ske_symbolic"));
}
