mod go_support;
use go_support::*;

/// AC-0.7-01 / §11.7: the ten query DTO services are reachable through the
/// SQLite-independent ApiService and return coherent shapes on a real DB.
#[test]
fn knowledge_api_all_ten_dto_services_respond() {
    let lab = Lab::new();
    let api = ske_api::ApiService::open(&lab.db).unwrap();
    // 1. inspect_snapshot
    let inspect = api.inspect_snapshot("latest", None).unwrap();
    assert!(inspect["snapshot"]["id"].is_i64());
    assert!(!inspect["units"].as_array().unwrap().is_empty());
    assert!(inspect["diagnostics_summary"].is_object());
    // 2. list_functions
    let functions = api.list_functions("latest", None, false, 100, 0).unwrap();
    assert!(functions["page"].is_object());
    let names: Vec<&str> = functions["functions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["qualified_name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"example.Check"));
    assert!(names.contains(&"example.Decide"));
    // 3. list_branches
    let branches = api
        .list_branches("latest", None, "example.Decide", 100, 0)
        .unwrap();
    assert!(branches["source_decisions"].is_array());
    assert!(branches["ir_outcomes"].is_array());
    assert!(branches["inventory"].is_object());
    // 4. get_function_cfg
    let cfg = api
        .get_function_cfg("latest", None, "example.Decide")
        .unwrap();
    assert_eq!(cfg["function"], "example.Decide");
    assert!(!cfg["blocks"].as_array().unwrap().is_empty());
    // 5. explore_paths
    let paths = api
        .explore_paths(
            "latest",
            None,
            "example.Entry",
            "example.helper",
            None,
            None,
            None,
            100,
            0,
        )
        .unwrap();
    assert!(paths["page"].is_object());
    // 6. get_function_summary (requires behavior reconstruction)
    let db = lab.db.clone();
    drop(api);
    let q = ske_query::Query::open(&db).unwrap();
    q.reconstruct_behaviors("latest", None, "example.Check", 100, 0)
        .unwrap();
    let api = ske_api::ApiService::open(&db).unwrap();
    let summary = api
        .get_function_summary("latest", None, "example.Check")
        .unwrap();
    assert!(summary["summaries"].is_array());
    // 7. reconstruct_behaviors
    let behaviors = api
        .reconstruct_behaviors("latest", None, "example.Check", 100, 0)
        .unwrap();
    assert_eq!(behaviors["cases"].as_array().unwrap().len(), 3);
    // 8. list_rules
    let rules = api.list_rules("latest", None, true, 100, 0).unwrap();
    let rule_rows = rules["rules"].as_array().unwrap();
    assert!(rule_rows.len() >= 3);
    // 9. get_rule_evidence by exact key
    let rule_key = rule_rows[0]["rule_key"].as_str().unwrap();
    let evidence = api.get_rule_evidence("latest", None, rule_key).unwrap();
    assert_eq!(evidence["rule_key"], rule_key);
    assert!(!evidence["evidence"].as_array().unwrap().is_empty());
    // 10. compare_snapshots requires two different snapshots of the same repo.
    // Comparing "latest" to itself resolves to the same snapshot and is
    // rejected rather than claiming an empty diff. Repo name is discovered
    // from the actual database rather than assumed.
    let snapshots = q.snapshots(None).unwrap();
    let repo = snapshots["snapshots"][0]["repository"]
        .as_str()
        .unwrap()
        .to_string();
    let err = api
        .compare_snapshots(&repo, "latest", "latest")
        .unwrap_err();
    assert_eq!(err.code, "E_USAGE");
}
