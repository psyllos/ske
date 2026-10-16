mod common;
use common::Harness;

#[test]
fn decide_query_outputs_and_pagination() {
    let h = Harness::new();
    let ingest = h.ingest("decide");
    assert_eq!(ingest["snapshot_id"], 1);
    assert_eq!(ingest["status"], "CLOSED");
    assert_eq!(ingest["inventory_status"], "COMPLETE");
    assert_eq!(
        ingest["diagnostics"],
        serde_json::json!({"INFO":0,"WARNING":0,"ERROR":0})
    );
    let inspect = h.ok(&["inspect"]);
    assert_eq!(
        inspect["units"][0]["counts"],
        serde_json::json!({
            "functions":1, "blocks":5, "instructions":7,
            "source_decisions":2, "source_outcomes":4,
            "ir_decisions":2, "ir_outcomes":4, "diagnostics":0
        })
    );
    let functions = h.ok(&["functions", "list"]);
    assert_eq!(functions["page"]["total"], 1);
    let cfg = h.ok(&["functions", "cfg", "--function", "example.Decide"]);
    assert_eq!(cfg["entry"], "example.Decide/b0");
    assert_eq!(cfg["blocks"].as_array().unwrap().len(), 5);
    let branches = h.ok(&["branches", "list", "--function", "example.Decide"]);
    assert!(branches["analysis"].is_null());
    assert_eq!(branches["source_decisions"].as_array().unwrap().len(), 2);
    let outcomes = branches["ir_outcomes"].as_array().unwrap();
    assert_eq!(outcomes.len(), 4);
    assert!(outcomes
        .iter()
        .all(|outcome| outcome["status"] == "UNANALYZED"));
}

#[test]
fn latest_repository_and_function_resolution() {
    let h = Harness::new();
    for repository in ["first", "second"] {
        h.ok(&[
            "ingest",
            "cir",
            "--input",
            &Harness::fixture("decide"),
            "--source-root",
            &Harness::root(),
            "--repository",
            repository,
        ]);
    }
    let (code, value) = h.run(&["inspect", "--snapshot", "latest"]);
    assert_eq!(code, 2);
    assert_eq!(value["error"]["code"], "E_AMBIGUOUS");
    let candidates = value["error"]["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    for name in ["first", "second"] {
        assert!(candidates
            .iter()
            .any(|v| v.as_str().unwrap().contains(name)));
        h.ok(&["inspect", "--repository", name]);
    }
    let (code, value) = h.run(&[
        "functions",
        "cfg",
        "--repository",
        "first",
        "--function",
        "nope",
    ]);
    assert_eq!(code, 7);
    assert_eq!(value["error"]["code"], "E_NOT_FOUND");
}

#[test]
fn latest_selects_closed_and_function_names_are_exact() {
    let h = Harness::new();
    h.ingest("decide");
    h.conn().execute("INSERT INTO snapshots(repository_id,status) SELECT repository_id,'OPEN' FROM snapshots WHERE id=1",[]).unwrap();
    assert_eq!(h.ok(&["inspect"])["snapshot"]["id"], 1);
    h.conn()
        .execute("UPDATE snapshots SET status='ABORTED' WHERE id=2", [])
        .unwrap();
    assert_eq!(h.ok(&["inspect"])["snapshot"]["id"], 1);
    for function in ["Decide", "1", "go:example@linux/amd64|example.Decide"] {
        let (exit, value) = h.run(&["functions", "cfg", "--function", function]);
        assert_eq!(exit, 7, "{function}: {value}");
        assert_eq!(value["error"]["code"], "E_NOT_FOUND");
    }
}

#[test]
fn branch_and_snapshot_pages_honor_global_options() {
    let h = Harness::new();
    h.ingest("decide");
    h.ingest("decide");
    let snapshots = h.ok(&["snapshots", "list", "--limit", "1", "--offset", "1"]);
    assert_eq!(snapshots["snapshots"].as_array().unwrap().len(), 1);
    assert_eq!(snapshots["snapshots"][0]["id"], 1);
    assert_eq!(
        snapshots["page"],
        serde_json::json!({"limit":1,"offset":1,"total":2,"truncated":false})
    );
    let branches = h.ok(&[
        "branches",
        "list",
        "--function",
        "example.Decide",
        "--limit",
        "1",
        "--offset",
        "1",
    ]);
    assert_eq!(branches["ir_outcomes"].as_array().unwrap().len(), 1);
    assert_eq!(branches["ir_outcomes"][0]["id"], "example.Decide/b0/i1/e1");
    assert_eq!(
        branches["page"],
        serde_json::json!({"limit":1,"offset":1,"total":4,"truncated":true})
    );
    let beyond = h.ok(&["functions", "list", "--offset", "100"]);
    assert_eq!(beyond["functions"], serde_json::json!([]));
    assert_eq!(beyond["page"]["total"], 1);
}

#[test]
fn external_stubs_are_listable_but_not_function_selectors() {
    let h = Harness::new();
    h.ingest("invoke");
    let (exit, value) = h.run(&["functions", "cfg", "--function", "example.risky"]);
    assert_eq!(exit, 7);
    assert_eq!(value["error"]["code"], "E_NOT_FOUND");
}
