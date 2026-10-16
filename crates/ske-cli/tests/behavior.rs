mod go_support;
use go_support::*;

fn behaviors(name: &str) -> serde_json::Value {
    let lab = Lab::new();
    let q = ske_query::Query::open(&lab.db).unwrap();
    q.reconstruct_behaviors("latest", None, name, 100, 0)
        .unwrap()
}

fn cases(name: &str) -> Vec<(String, String, String)> {
    let b = behaviors(name);
    b["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["when"].as_str().unwrap().to_owned(),
                c["then"].as_str().unwrap().to_owned(),
                c["precision"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn behavior_check_exact_reference() {
    let b = behaviors("example.Check");
    assert_eq!(b["precision"], "EXACT");
    let list = b["cases"].as_array().unwrap();
    assert_eq!(list.len(), 3);
    let expected = [
        ("age < 18", "return ErrUnderage"),
        ("age >= 18 AND enabled == false", "return ErrDisabled"),
        ("age >= 18 AND enabled == true", "return nil"),
    ];
    for (i, (when, then)) in expected.iter().enumerate() {
        assert_eq!(list[i]["ordinal"], i as u64);
        assert_eq!(list[i]["when"].as_str().unwrap(), *when);
        assert_eq!(list[i]["then"].as_str().unwrap(), *then);
        assert_eq!(list[i]["exit_kind"], "RETURN");
        assert_eq!(list[i]["precision"], "EXACT");
    }
    let mut covered = std::collections::BTreeSet::new();
    for c in list {
        for s in c["source_outcomes"].as_array().unwrap() {
            covered.insert(s.as_str().unwrap().to_owned());
        }
    }
    assert_eq!(covered.len(), 4);
}

#[test]
fn behavior_check_rules_and_evidence_11() {
    let lab = Lab::new();
    let q = ske_query::Query::open(&lab.db).unwrap();
    q.reconstruct_behaviors("latest", None, "example.Check", 100, 0)
        .unwrap();
    let c = lab.conn();
    let rules: i64 = c
        .query_row("SELECT COUNT(*) FROM functional_rules", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rules, 3);
    let evidence: i64 = c
        .query_row("SELECT COUNT(*) FROM knowledge_evidence", [], |r| r.get(0))
        .unwrap();
    assert_eq!(evidence, 11);
    let min_evidence: i64 = c
        .query_row(
            "SELECT MIN(n) FROM (SELECT COUNT(*) AS n FROM knowledge_evidence GROUP BY functional_rule_id)",
            [], |r| r.get(0),
        )
        .unwrap();
    assert!(min_evidence >= 1, "every rule needs at least one evidence");
    let unvalidated: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM functional_rules WHERE origin='DETERMINISTIC' AND validation_status='UNVALIDATED'",
            [], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(unvalidated, 3);
    assert_eq!(q.db_check().unwrap()["ok"], true);
}

#[test]
fn behavior_decide_no_unreachable_return() {
    let lab = Lab::new();
    let q = ske_query::Query::open(&lab.db).unwrap();
    let b = q
        .reconstruct_behaviors("latest", None, "example.Decide", 100, 0)
        .unwrap();
    let list = b["cases"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert!(list.iter().all(|c| c["then"] != "return -2"));
    assert_eq!(list[0]["when"], "x < 0");
    assert_eq!(list[0]["then"], "return -1");
    assert_eq!(list[1]["when"], "x >= 0 AND x >= -10");
    assert_eq!(list[1]["then"], "return 0");
    let sidx: i64 = lab
        .conn()
        .query_row(
            "SELECT s.id FROM snapshots s WHERE s.status='CLOSED' ORDER BY s.id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let fidx: i64 = lab.conn().query_row(
        "SELECT f.id FROM functions f JOIN symbols s ON s.id=f.symbol_id WHERE s.qualified_name='example.Decide' AND s.snapshot_id=?1",
        [sidx], |r| r.get(0),
    ).unwrap();
    let count = |c: &rusqlite::Connection| -> i64 {
        c.query_row(
            "SELECT COUNT(*) FROM source_outcomes o JOIN source_decisions d ON d.id=o.decision_id WHERE d.function_id=?1",
            [fidx], |r| r.get(0),
        ).unwrap()
    };
    let before = count(&lab.conn());
    let mut q = q;
    q.analyze(
        "latest",
        None,
        "example.Decide",
        &Default::default(),
        &Default::default(),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    let after = count(&lab.conn());
    assert_eq!(after, before, "prop_inventory_never_shrinks");
    let branches = q
        .branches_run("latest", None, "example.Decide", "latest", 100, 0)
        .unwrap();
    let ir = branches["ir_outcomes"].as_array().unwrap();
    assert_eq!(ir.len(), 4);
    let proven = ir
        .iter()
        .filter(|o| o["status"] == "PROVEN_UNREACHABLE")
        .count();
    assert_eq!(proven, 1);
}

#[test]
fn behavior_adjusted_no_simplification() {
    let list = cases("example.Adjusted");
    assert_eq!(list.len(), 2);
    assert!(list.contains(&(
        "x > 10".to_owned(),
        "return x - 3".to_owned(),
        "EXACT".to_owned()
    )));
    assert!(list.contains(&(
        "x <= 10".to_owned(),
        "return x - 0".to_owned(),
        "EXACT".to_owned()
    )));
}

#[test]
fn behavior_entry_reference() {
    let list = cases("example.Entry");
    assert_eq!(list.len(), 2);
    assert!(list.contains(&(
        "a > 100".to_owned(),
        "return 1".to_owned(),
        "EXACT".to_owned()
    )));
    assert!(list.contains(&(
        "a <= 100".to_owned(),
        "return 0".to_owned(),
        "EXACT".to_owned()
    )));
}

#[test]
fn behavior_divmod_reference() {
    let list = cases("example.DivMod");
    assert_eq!(list.len(), 2);
    assert!(list.contains(&(
        "b == 0".to_owned(),
        "return 0, ErrZero".to_owned(),
        "EXACT".to_owned()
    )));
    assert!(list.contains(&(
        "b != 0".to_owned(),
        "return a / b, nil".to_owned(),
        "EXACT".to_owned()
    )));
}

#[test]
fn behavior_unknown_effect_memopaque_overapprox() {
    let lab = Lab::new();
    let q = ske_query::Query::open(&lab.db).unwrap();
    let b = q
        .reconstruct_behaviors("latest", None, "example.MemOpaque", 100, 0)
        .unwrap();
    for c in b["cases"].as_array().unwrap() {
        let precision = c["precision"].as_str().unwrap();
        assert!(precision == "OVERAPPROX" || precision == "PARTIAL" || precision == "UNKNOWN");
        assert_ne!(precision, "EXACT");
    }
}

#[test]
fn behavior_no_business_text() {
    let lab = Lab::new();
    let q = ske_query::Query::open(&lab.db).unwrap();
    q.reconstruct_behaviors("latest", None, "example.Check", 100, 0)
        .unwrap();
    let rules = q.list_rules("latest", None, true, 100, 0).unwrap();
    for rule in rules["rules"].as_array().unwrap() {
        let description = rule["description"].as_str().unwrap();
        assert!(description.starts_with("WHEN "), "{description}");
        assert!(description.contains(" THEN "), "{description}");
    }
}

#[test]
fn behavior_yaml_parses_same_as_json() {
    let lab = Lab::new();
    let q = ske_query::Query::open(&lab.db).unwrap();
    let json = q
        .reconstruct_behaviors("latest", None, "example.Check", 100, 0)
        .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ske"))
        .env_remove("SKE_DB")
        .env("GOMAXPROCS", "2")
        .args([
            "behaviors",
            "reconstruct",
            "--entry",
            "example.Check",
            "--format",
            "yaml",
            "--db",
        ])
        .arg(&lab.db)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "yaml: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    let yaml_value: serde_json::Value = serde_yaml::from_str(&text).unwrap();
    assert_eq!(yaml_value["cases"].as_array().unwrap().len(), 3);
    assert_eq!(json["function"], yaml_value["function"]);
    assert_eq!(json["cases"][0]["when"], yaml_value["cases"][0]["when"]);
    assert_eq!(json["cases"][0]["then"], yaml_value["cases"][0]["then"]);
}

#[test]
fn rules_stale_check_detects_function_change() {
    // One temp repository: ingest once (A), reconstruct rules for Decide+Check,
    // modify decide.go, re-ingest (B), then stale-check A -> B.
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("k.sqlite");
    let repo = dir.path().join("lab");
    std::fs::create_dir_all(&repo).unwrap();
    fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let text = name.to_str().unwrap();
            if text.ends_with(".cir.pb") || text == "tags" || text == "partial" {
                continue;
            }
            let target = to.join(&name);
            if entry.file_type().unwrap().is_dir() {
                std::fs::create_dir_all(&target).unwrap();
                copy_dir(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    copy_dir(&root().join("examples/go-branch-lab"), &repo);

    // init + ingest A
    let run = |args: &[&str]| -> serde_json::Value {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_ske"))
            .env_remove("SKE_DB")
            .env("GOMAXPROCS", "2")
            .args(args)
            .arg("--db")
            .arg(&db)
            .arg("--format")
            .arg("json")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    };
    run(&["init"]);
    let profile_a = dir.path().join("a.json");
    std::fs::write(&profile_a, profile("linux", "amd64").to_string()).unwrap();
    let a = run(&[
        "ingest",
        "go",
        "--repo",
        repo.to_str().unwrap(),
        "--adapter",
        adapter().to_str().unwrap(),
        "--yes-run-adapter",
        "--profile",
        profile_a.to_str().unwrap(),
    ]);
    let a_id = a["snapshot_id"].as_i64().unwrap();
    // Reconstruct rules for Decide and Check in A.
    for name in ["example.Decide", "example.Check"] {
        run(&["behaviors", "reconstruct", "--entry", name]);
    }
    // Modify decide.go in the SAME repo directory.
    let decide = repo.join("decide.go");
    let text = std::fs::read_to_string(&decide)
        .unwrap()
        .replace("return -1", "return -11");
    std::fs::write(&decide, text).unwrap();
    // Re-ingest (B). Same repository (same root_uri) => snapshot B in same repo.
    let b = run(&[
        "ingest",
        "go",
        "--repo",
        repo.to_str().unwrap(),
        "--adapter",
        adapter().to_str().unwrap(),
        "--yes-run-adapter",
        "--profile",
        profile_a.to_str().unwrap(),
    ]);
    let b_id = b["snapshot_id"].as_i64().unwrap();
    let repo_name: String = {
        let c = rusqlite::Connection::open(&db).unwrap();
        c.query_row("SELECT name FROM repositories LIMIT 1", [], |r| r.get(0))
            .unwrap()
    };
    let stale = std::process::Command::new(env!("CARGO_BIN_EXE_ske"))
        .env_remove("SKE_DB")
        .args(["rules", "stale-check", "--db"])
        .arg(&db)
        .arg("--repository")
        .arg(&repo_name)
        .arg("--from")
        .arg(a_id.to_string())
        .arg("--to")
        .arg(b_id.to_string())
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();
    assert!(
        stale.status.success(),
        "stale: {}",
        String::from_utf8_lossy(&stale.stderr)
    );
    let stale_json: serde_json::Value = serde_json::from_slice(&stale.stdout).unwrap();
    assert_eq!(stale_json["rules_marked_stale"], 2);
    assert_eq!(stale_json["rules_unchanged"], 3);
    // A's rules: Decide rules are STALE, Check rules remain UNVALIDATED.
    let c = rusqlite::Connection::open(&db).unwrap();
    let decide_stale: i64 = c.query_row(
        "SELECT COUNT(*) FROM functional_rules fr JOIN behavior_cases b ON b.id=fr.behavior_case_id
         JOIN behavior_scenarios bs ON bs.id=b.behavior_id JOIN functions f ON f.id=bs.entry_function_id
         JOIN symbols s ON s.id=f.symbol_id
         WHERE fr.snapshot_id=?1 AND s.qualified_name='example.Decide' AND fr.validation_status='STALE'",
        [a_id], |r| r.get(0),
    ).unwrap();
    assert_eq!(decide_stale, 2);
    let check_ok: i64 = c.query_row(
        "SELECT COUNT(*) FROM functional_rules fr JOIN behavior_cases b ON b.id=fr.behavior_case_id
         JOIN behavior_scenarios bs ON bs.id=b.behavior_id JOIN functions f ON f.id=bs.entry_function_id
         JOIN symbols s ON s.id=f.symbol_id
         WHERE fr.snapshot_id=?1 AND s.qualified_name='example.Check' AND fr.validation_status='UNVALIDATED'",
        [a_id], |r| r.get(0),
    ).unwrap();
    assert_eq!(check_ok, 3);
}

#[test]
fn summaries_reuse_across_snapshots() {
    // Ingest the same repo twice -> snapshots A and B. Compute summaries in A,
    // then in B. B must reuse every function's summary (summaries_reused = count,
    // summaries_computed = 0, reused_from_summary_id non-NULL) with identical
    // results (modulo ids).
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("k.sqlite");
    let repo = dir.path().join("lab");
    std::fs::create_dir_all(&repo).unwrap();
    fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let text = name.to_str().unwrap();
            if text.ends_with(".cir.pb") || text == "tags" || text == "partial" {
                continue;
            }
            let target = to.join(&name);
            if entry.file_type().unwrap().is_dir() {
                std::fs::create_dir_all(&target).unwrap();
                copy_dir(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    copy_dir(&root().join("examples/go-branch-lab"), &repo);
    let run = |args: &[&str]| -> serde_json::Value {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_ske"))
            .env_remove("SKE_DB")
            .env("GOMAXPROCS", "2")
            .args(args)
            .arg("--db")
            .arg(&db)
            .arg("--format")
            .arg("json")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    };
    run(&["init"]);
    let profile_path = dir.path().join("p.json");
    let ingest = |snapshot_label: &str| -> serde_json::Value {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_ske"))
            .env_remove("SKE_DB")
            .env("GOMAXPROCS", "2")
            .args([
                "ingest",
                "go",
                "--repo",
                repo.to_str().unwrap(),
                "--adapter",
                adapter().to_str().unwrap(),
                "--yes-run-adapter",
                "--profile",
                profile_path.to_str().unwrap(),
                "--db",
            ])
            .arg(&db)
            .arg("--format")
            .arg("json")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "ingest {snapshot_label}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    };
    std::fs::write(&profile_path, profile("linux", "amd64").to_string()).unwrap();
    let a = ingest("A");
    let a_id = a["snapshot_id"].as_i64().unwrap();
    let b = ingest("B");
    let b_id = b["snapshot_id"].as_i64().unwrap();
    assert_ne!(a_id, b_id);
    // Compute summaries for Check + Decide + DivMod in A (capture results).
    let q = ske_query::Query::open(&db).unwrap();
    let mut a_results = Vec::new();
    for name in ["example.Check", "example.Decide", "example.DivMod"] {
        let s = q
            .summarize_scalar(&a_id.to_string(), None, name, &Default::default())
            .unwrap();
        assert_eq!(s["metrics"]["summaries_computed"], 1);
        assert_eq!(s["metrics"]["summaries_reused"], 0);
        a_results.push((name.to_owned(), s));
    }
    // Compute the same in B: everything must be reused.
    let mut results = Vec::new();
    for name in ["example.Check", "example.Decide", "example.DivMod"] {
        let s = q
            .summarize_scalar(&b_id.to_string(), None, name, &Default::default())
            .unwrap();
        assert_eq!(
            s["metrics"]["summaries_computed"], 0,
            "{name}: B must not recompute"
        );
        assert_eq!(s["metrics"]["summaries_reused"], 1, "{name}: B must reuse");
        results.push((name.to_owned(), s));
    }
    // reused_from_summary_id non-NULL on every B summary row.
    for name in ["example.Check", "example.Decide", "example.DivMod"] {
        let reused: i64 =
            {
                let c = rusqlite::Connection::open(&db).unwrap();
                c.query_row(
                "SELECT COUNT(*) FROM function_summaries fs JOIN functions f ON f.id=fs.function_id
                 JOIN symbols s ON s.id=f.symbol_id
                 WHERE s.qualified_name=?1 AND fs.reused_from_summary_id IS NOT NULL",
                [name], |r| r.get(0),
            ).unwrap()
            };
        assert_eq!(reused, 1, "{name}: reused_from_summary_id set");
    }
    // Results identical to A (same case count / precision per function):
    // compare the A results captured before B was summarized with B's.
    assert_eq!(a_results.len(), results.len());
    for ((a_name, in_a), (b_name, in_b)) in a_results.iter().zip(&results) {
        assert_eq!(a_name, b_name);
        assert_eq!(
            in_a["cases"], in_b["cases"],
            "{a_name}: identical case count"
        );
        assert_eq!(
            in_a["precision"], in_b["precision"],
            "{a_name}: identical precision"
        );
        assert_eq!(
            in_a["complete"], in_b["complete"],
            "{a_name}: identical completeness"
        );
    }
}
