mod common;
use common::Harness;

#[test]
fn runs_mark_failed() {
    let h = Harness::new();
    h.ingest("decide");
    h.conn()
        .execute_batch(
            "INSERT INTO analysis_runs(snapshot_id,analysis_kind,engine_version,
         config_fingerprint,assumptions_fingerprint,status)
         VALUES(1,'BRANCH_ANALYSIS','test','config','assumptions','RUNNING');",
        )
        .unwrap();
    let listed = h.ok(&["runs", "list"]);
    assert_eq!(listed["runs"][0]["run_id"], 1);
    assert_eq!(listed["runs"][0]["run_status"], "RUNNING");
    let checked = h.ok(&["db", "check"]);
    assert!(
        checked["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "CHK-19" && c["violations"] == 1 && c["severity"] == "WARNING"),
        "{checked}"
    );
    assert_eq!(
        h.ok(&["runs", "mark-failed", "--run", "1"])["run_status"],
        "FAILED"
    );
    assert_eq!(h.ok(&["runs", "list"])["runs"][0]["run_status"], "FAILED");
    let (exit, error) = h.run(&["runs", "mark-failed", "--run", "1"]);
    assert_eq!(exit, 2, "{error}");
    assert_eq!(error["error"]["code"], "E_USAGE");
    assert_eq!(h.run(&["runs", "mark-failed", "--run", "99"]).0, 7);
    let checked = h.ok(&["db", "check"]);
    assert!(checked["checks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["violations"] == 0));
}
