//! These acceptance cases use the real Go adapter and trusted temporary Go sources.
mod go_support;
use go_support::{adapter, Lab};
use serde_json::{json, Value};
use std::{
    fs,
    process::{Command, Stdio},
};

fn ingest(lab: &Lab, patterns: Value, extra: &[&str]) -> (i32, Value) {
    let path = lab.dir.path().join("profile.json");
    fs::write(&path, json!({"patterns":patterns}).to_string()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_ske"))
        .args(["ingest", "go", "--repo"])
        .arg(lab.dir.path().join("repo"))
        .arg("--adapter")
        .arg(adapter())
        .arg("--yes-run-adapter")
        .arg("--profile")
        .arg(path)
        .arg("--db")
        .arg(&lab.db)
        .args(["--format", "json"])
        .args(extra)
        .stdin(Stdio::null())
        .env_remove("SKE_DB")
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        serde_json::from_slice(&out.stdout).unwrap(),
    )
}
fn project(lab: &Lab) {
    let root = lab.dir.path().join("repo");
    fs::create_dir_all(root.join("bad")).unwrap();
    fs::write(root.join("go.mod"), "module failurelab\n\ngo 1.22\n").unwrap();
    fs::write(
        root.join("ok.go"),
        "package failurelab\nfunc Answer() int { return 42 }\n",
    )
    .unwrap();
    fs::write(
        root.join("bad/bad.go"),
        "package bad\nfunc Broken() int { return missing }\n",
    )
    .unwrap();
}
#[test]
fn no_matching_patterns_abort_with_failed_unit_and_exit_three() {
    let lab = Lab::empty();
    project(&lab);
    let (code, value) = ingest(&lab, json!(["./absent/..."]), &[]);
    assert_eq!(code, 3, "{value}");
    assert_eq!(value["status"], "ABORTED");
    assert!(value["units"]
        .as_array()
        .unwrap()
        .iter()
        .all(|u| u["inventory_status"] == "FAILED"));
    assert_eq!(lab.ok(&["db", "check"])["ok"], true);
}
#[test]
fn one_failed_one_successful_closes_with_diagnostics_and_require_complete() {
    let lab = Lab::empty();
    project(&lab);
    for (extra, expected) in [(vec![], 0), (vec!["--require-complete", "--redact"], 4)] {
        let (code, value) = ingest(&lab, json!(["./..."]), &extra);
        assert_eq!(code, expected, "{value}");
        assert_eq!(value["status"], "CLOSED");
        let units = value["units"].as_array().unwrap();
        assert_eq!(units.len(), 2, "{value}");
        assert!(units.iter().any(|u| u["inventory_status"] == "FAILED"));
        assert!(units.iter().any(|u| u["inventory_status"] == "COMPLETE"));
        let conn = lab.conn();
        let id = value["snapshot_id"].as_i64().unwrap();
        let diagnostics: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM diagnostics WHERE snapshot_id=?1 AND code='UNIT_FAILED'",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(diagnostics, 1);
        if !extra.is_empty() {
            let messages: String = conn
                .query_row(
                    "SELECT group_concat(message) FROM diagnostics WHERE snapshot_id=?1",
                    [id],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(
                !messages.contains(lab.dir.path().to_str().unwrap()),
                "{messages}"
            );
            assert!(messages.contains("redacted"));
        }
        assert_eq!(lab.ok(&["db", "check"])["ok"], true);
    }
}
