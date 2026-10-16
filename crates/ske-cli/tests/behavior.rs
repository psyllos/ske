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
    let result_spans = [
        "check.go:10:3-10:21",
        "check.go:13:3-13:21",
        "check.go:15:2-15:12",
    ];
    for (i, (when, then)) in expected.iter().enumerate() {
        assert_eq!(list[i]["ordinal"], i as u64);
        assert_eq!(list[i]["when"].as_str().unwrap(), *when);
        assert_eq!(list[i]["then"].as_str().unwrap(), *then);
        assert_eq!(list[i]["exit_kind"], "RETURN");
        assert_eq!(list[i]["precision"], "EXACT");
        assert!(list[i]["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["relation"] == "RESULT_SITE" && e["span"] == result_spans[i]));
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
fn behavior_pagination_persists_all_cases_without_renumbering() {
    let lab = Lab::new();
    let q = ske_query::Query::open(&lab.db).unwrap();
    let page = q
        .reconstruct_behaviors("latest", None, "example.Check", 1, 1)
        .unwrap();
    assert_eq!(page["cases"].as_array().unwrap().len(), 1);
    assert_eq!(page["cases"][0]["ordinal"], 1);
    assert_eq!(page["cases"][0]["then"], "return ErrDisabled");
    assert_eq!(page["page"]["total"], 3);
    let rules = q.list_rules("latest", None, true, 100, 0).unwrap();
    assert_eq!(rules["rules"].as_array().unwrap().len(), 3);
}

#[test]
fn behavior_failure_rolls_back_scenario_run_and_rules() {
    let lab = Lab::new();
    let q = ske_query::Query::open(&lab.db).unwrap();
    q.summarize_scalar("latest", None, "example.Check", &Default::default())
        .unwrap();
    let c = lab.conn();
    c.execute(
        "UPDATE symbolic_expressions SET kind='INVALID_TEST_NODE'",
        [],
    )
    .unwrap();
    assert!(q
        .reconstruct_behaviors("latest", None, "example.Check", 100, 0)
        .is_err());
    for table in [
        "behavior_scenarios",
        "behavior_cases",
        "functional_rules",
        "knowledge_evidence",
    ] {
        let count: i64 = c
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "{table} must roll back");
    }
    let runs: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM analysis_runs WHERE analysis_kind='BEHAVIOR'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(runs, 0);
}

#[test]
fn behavior_dedup_retains_every_summary_and_result_site() {
    let lab = Lab::new();
    let q = ske_query::Query::open(&lab.db).unwrap();
    q.summarize_scalar("latest", None, "example.Check", &Default::default())
        .unwrap();
    let c = lab.conn();
    // Controlled persistence fixture: make both error exits return the same
    // expression, so the normative grouping rule must merge them.
    c.execute(
        "UPDATE summary_case_returns SET expression_id=(
           SELECT r.expression_id FROM summary_case_returns r JOIN summary_cases sc ON sc.id=r.summary_case_id
           JOIN source_spans sp ON sp.id=sc.exit_span_id WHERE sp.start_line=10 LIMIT 1)
         WHERE summary_case_id IN (
           SELECT sc.id FROM summary_cases sc JOIN source_spans sp ON sp.id=sc.exit_span_id WHERE sp.start_line=13)",
        [],
    ).unwrap();
    let result = q
        .reconstruct_behaviors("latest", None, "example.Check", 100, 0)
        .unwrap();
    assert_eq!(result["cases"].as_array().unwrap().len(), 2);
    let merged = &result["cases"][0]["evidence"];
    assert_eq!(
        merged
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["relation"] == "DERIVED_FROM")
            .count(),
        2
    );
    assert_eq!(
        merged
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["relation"] == "RESULT_SITE")
            .count(),
        2
    );
    let anchors: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM knowledge_evidence WHERE relation='DERIVED_FROM'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(anchors, 3);
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
        let run = s["run_id"].as_i64().unwrap();
        let assumptions: i64 = rusqlite::Connection::open(&db)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM analysis_assumptions
             WHERE analysis_run_id=?1 AND kind IN ('SEQUENTIAL_EXECUTION','CLOSED_ENTRY_CONTEXT')",
                [run],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(assumptions, 2, "reuse must preserve mandatory assumptions");
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
    let c = rusqlite::Connection::open(&db).unwrap();
    let cross_snapshot_outcomes: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM summary_case_outcomes sco
         JOIN summary_cases sc ON sc.id=sco.summary_case_id
         JOIN function_summaries fs ON fs.id=sc.summary_id
         JOIN analysis_runs ar ON ar.id=fs.produced_by_run_id
         JOIN ir_outcomes io ON io.id=sco.ir_outcome_id
         JOIN control_edges e ON e.id=io.control_edge_id
         JOIN basic_blocks b ON b.id=e.source_block_id
         JOIN functions f ON f.id=b.function_id
         JOIN symbols s ON s.id=f.symbol_id
         WHERE ar.snapshot_id<>s.snapshot_id",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        cross_snapshot_outcomes, 0,
        "reused outcomes must belong to the destination snapshot"
    );
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
        let original = q
            .reconstruct_behaviors(&a_id.to_string(), None, a_name, 100, 0)
            .unwrap();
        let reused = q
            .reconstruct_behaviors(&b_id.to_string(), None, b_name, 100, 0)
            .unwrap();
        for (a_case, b_case) in original["cases"]
            .as_array()
            .unwrap()
            .iter()
            .zip(reused["cases"].as_array().unwrap())
        {
            for field in ["when", "then", "precision", "source_outcomes"] {
                assert_eq!(a_case[field], b_case[field], "{a_name}: {field}");
            }
        }
    }
    // A new caller must be able to apply a reused callee, not just report its
    // case count. Only helper is cached in A; Entry must be computed in B.
    q.summarize_scalar(
        &a_id.to_string(),
        None,
        "example.helper",
        &Default::default(),
    )
    .unwrap();
    let entry = q
        .summarize_scalar(
            &b_id.to_string(),
            None,
            "example.Entry",
            &Default::default(),
        )
        .unwrap();
    assert_eq!(entry["cases"], 2);
    assert_eq!(entry["metrics"]["summaries_reused"], 1);
    assert_eq!(entry["metrics"]["summaries_computed"], 1);

    // Build options invalidate all reuse even when the function body and unit
    // key remain identical (an otherwise unused build tag).
    let mut changed_profile = profile("linux", "amd64");
    changed_profile["tags"] = serde_json::json!(["ske_unused_validation_tag"]);
    std::fs::write(&profile_path, changed_profile.to_string()).unwrap();
    let configured = ingest("new build configuration");
    let c_id = configured["snapshot_id"].as_i64().unwrap();
    let configured_summary = q
        .summarize_scalar(
            &c_id.to_string(),
            None,
            "example.Check",
            &Default::default(),
        )
        .unwrap();
    assert_eq!(configured_summary["metrics"]["summaries_computed"], 1);
    assert_eq!(configured_summary["metrics"]["summaries_reused"], 0);
    q.summarize_scalar(
        &c_id.to_string(),
        None,
        "example.Decide",
        &Default::default(),
    )
    .unwrap();

    // A structural change under the same build configuration also invalidates.
    let decide = repo.join("decide.go");
    let text = std::fs::read_to_string(&decide)
        .unwrap()
        .replace("return -1", "return -11");
    std::fs::write(&decide, text).unwrap();
    let changed = ingest("changed body");
    let d_id = changed["snapshot_id"].as_i64().unwrap();
    let changed_summary = q
        .summarize_scalar(
            &d_id.to_string(),
            None,
            "example.Decide",
            &Default::default(),
        )
        .unwrap();
    assert_eq!(changed_summary["metrics"]["summaries_computed"], 1);
    assert_eq!(changed_summary["metrics"]["summaries_reused"], 0);
    let deps: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM analysis_dependencies WHERE snapshot_id=?1",
            [b_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        deps >= 4,
        "FUNCTION and SUMMARY dependencies must be durable"
    );
}
