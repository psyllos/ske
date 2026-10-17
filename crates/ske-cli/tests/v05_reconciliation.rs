//! Regressions ported to main's existing v0.5 API, not a second implementation.
mod go_support;
use go_support::{adapter, profile, root, Lab};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn ingest(lab: &Lab, repo: &Path) -> i64 {
    let config = lab.dir.path().join("profile.json");
    std::fs::write(&config, profile("linux", "amd64").to_string()).unwrap();
    lab.ok(&[
        "ingest",
        "go",
        "--repo",
        repo.to_str().unwrap(),
        "--profile",
        config.to_str().unwrap(),
        "--adapter",
        adapter().to_str().unwrap(),
        "--yes-run-adapter",
        "--repository",
        "reconcile-lab",
    ])["snapshot_id"]
        .as_i64()
        .unwrap()
}

#[test]
fn stale_check_invalidates_transitive_callee_but_not_unrelated_rules() {
    let lab = Lab::empty();
    let repo = lab.dir.path().join("lab");
    copy_tree(&root().join("examples/go-branch-lab"), &repo);
    let a = ingest(&lab, &repo);
    let q = ske_query::Query::open(&lab.db).unwrap();
    for entry in ["example.Entry", "example.Check"] {
        q.reconstruct_behaviors(&a.to_string(), None, entry, 100, 0)
            .unwrap();
    }
    let path = repo.join("interproc.go");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(path, text.replacen("return 1", "return 2", 1)).unwrap();
    let b = ingest(&lab, &repo);
    let changed = q
        .rules_stale_check("reconcile-lab", &a.to_string(), &b.to_string())
        .unwrap();
    assert_eq!(changed["rules_marked_stale"], 2);
    for rule in q.list_rules(&a.to_string(), None, true, 100, 0).unwrap()["rules"]
        .as_array()
        .unwrap()
    {
        let expected = if rule["rule_key"]
            .as_str()
            .unwrap()
            .starts_with("example.Entry")
        {
            "STALE"
        } else {
            "UNVALIDATED"
        };
        assert_eq!(rule["validation_status"], expected);
    }
    assert!(
        q.list_rules(&b.to_string(), None, true, 100, 0).unwrap()["rules"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

fn effect_lab(lab: &Lab) -> PathBuf {
    let repo = lab.dir.path().join("effects");
    std::fs::create_dir(&repo).unwrap();
    std::fs::write(repo.join("go.mod"), "module effects\n").unwrap();
    // Trusted source authored here, type-checked/extracted, never executed.
    std::fs::write(repo.join("effects.go"),
        "package effects\nvar Hook func()\nfunc Paths(x int) int {\n if x < 0 { return 0 }\n Hook()\n return 0\n}\n").unwrap();
    repo
}

#[test]
fn path_effects_survive_persistence_and_copy_without_false_case_merge() {
    let lab = Lab::empty();
    let repo = effect_lab(&lab);
    let q = ske_query::Query::open(&lab.db).unwrap();
    for _ in 0..2 {
        let snapshot = ingest(&lab, &repo);
        let result = q
            .reconstruct_behaviors(&snapshot.to_string(), None, "effects.Paths", 100, 0)
            .unwrap();
        let cases = result["cases"].as_array().unwrap();
        assert_eq!(
            cases.len(),
            2,
            "equal returns with different effects must not merge"
        );
        assert!(cases.iter().all(|case| case["then"] == "return 0"));
        assert_eq!(result["scenario"]["external_boundary"], "EXTERNAL_UNKNOWN");
        let db = lab.conn();
        let mut stmt = db.prepare("SELECT c.effects_text FROM behavior_cases c
            JOIN behavior_scenarios b ON b.id=c.behavior_id JOIN analysis_runs r ON r.id=b.analysis_run_id
            WHERE r.snapshot_id=?1 ORDER BY c.case_ordinal").unwrap();
        let effects: Vec<String> = stmt
            .query_map([snapshot], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let early: Value = serde_json::from_str(&effects[0]).unwrap();
        let later: Value = serde_json::from_str(&effects[1]).unwrap();
        assert!(
            early.as_array().unwrap().is_empty(),
            "effect leaks into the early return"
        );
        assert_eq!(later.as_array().unwrap().len(), 1);
        assert_eq!(q.db_check().unwrap()["ok"], true);
    }
    let db = lab.conn();
    let reused: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM function_summaries WHERE reused_from_summary_id IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        reused, 1,
        "second snapshot must exercise main's copy-on-reuse path"
    );
}

#[test]
fn behavior_evidence_names_the_actual_summary_producer() {
    let lab = Lab::new();
    let q = ske_query::Query::open(&lab.db).unwrap();
    let summary = q
        .summarize_scalar("latest", None, "example.Check", &Default::default())
        .unwrap();
    let producer = summary["run_id"].as_i64().unwrap();
    for _ in 0..2 {
        let result = q
            .reconstruct_behaviors("latest", None, "example.Check", 100, 0)
            .unwrap();
        assert_ne!(
            result["run_id"], producer,
            "main retains distinct BEHAVIOR runs"
        );
        assert_eq!(result["summary_run_id"], producer);
        for case in result["cases"].as_array().unwrap() {
            for e in case["evidence"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["kind"] == "summary_case")
            {
                assert_eq!(e["producer_run_id"], producer);
                assert!(e["id"]
                    .as_str()
                    .unwrap()
                    .starts_with(&format!("example.Check@run{producer}#case")));
            }
        }
    }
    for rule in q.list_rules("latest", None, true, 100, 0).unwrap()["rules"]
        .as_array()
        .unwrap()
    {
        for e in rule["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "summary_case")
        {
            assert_eq!(e["producer_run_id"], producer);
        }
    }
    assert_eq!(q.db_check().unwrap()["ok"], true);
}

#[test]
fn legacy_summary_cache_is_not_reinterpreted_as_path_effects() {
    let lab = Lab::empty();
    let repo = effect_lab(&lab);
    ingest(&lab, &repo);
    let q = ske_query::Query::open(&lab.db).unwrap();
    let old = q
        .summarize_scalar("latest", None, "effects.Paths", &Default::default())
        .unwrap();
    let db = lab.conn();
    db.execute(
        "UPDATE analysis_runs SET engine_version='ske-summary-scalar-0.5' WHERE id=?1",
        [old["run_id"].as_i64().unwrap()],
    )
    .unwrap();
    db.execute(
        "UPDATE function_summaries SET summary_fingerprint='legacy-function-effects' WHERE id=?1",
        [old["summary_id"].as_i64().unwrap()],
    )
    .unwrap();
    let result = q
        .reconstruct_behaviors("latest", None, "effects.Paths", 100, 0)
        .unwrap();
    assert_ne!(
        result["summary_run_id"], old["run_id"],
        "must compute with the path-effect version"
    );
    assert_eq!(result["cases"].as_array().unwrap().len(), 2);
    let old_version: String = db
        .query_row(
            "SELECT engine_version FROM analysis_runs WHERE id=?1",
            [old["run_id"].as_i64().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        old_version, "ske-summary-scalar-0.5",
        "historical producer must not be rewritten"
    );
}
