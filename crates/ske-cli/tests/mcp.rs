mod go_support;
use go_support::*;
use serde_json::json;
use ske_query::Query;

fn lab_analyzed_decide() -> (Lab, Query) {
    let lab = Lab::new();
    // Analyze Decide so sd1/o0 (x < -10) becomes PROVEN_UNREACHABLE.
    let value = lab.ok(&["branches", "analyze", "--entry", "example.Decide"]);
    assert_eq!(value["run_status"], "COMPLETED");
    let q = Query::open(&lab.db).unwrap();
    (lab, q)
}

#[test]
fn mcp_tools_list_exposes_exactly_five_tools() {
    let tools = ske_mcp::list_tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        vec!["inspect", "branches", "explain", "rules", "impact"]
    );
    for tool in &tools {
        assert!(tool.input_schema.is_object(), "{}/input", tool.name);
        assert!(tool.output_schema.is_object(), "{}/output", tool.name);
        assert!(!tool.description.is_empty());
    }
}

#[test]
fn mcp_rules_for_check_returns_three_rules_with_evidence() {
    let lab = Lab::new();
    let q = Query::open(&lab.db).unwrap();
    q.reconstruct_behaviors("latest", None, "example.Check", 100, 0)
        .unwrap();
    let out = ske_mcp::call_tool(&q, "rules", &json!({"with_evidence": true})).unwrap();
    let rules = out["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 3);
    for rule in rules {
        assert!(rule["evidence"]
            .as_array()
            .map(|e| !e.is_empty())
            .unwrap_or(false));
        assert_eq!(rule["origin"], "DETERMINISTIC");
        assert_eq!(rule["validation_status"], "UNVALIDATED");
    }
    assert!(out["page"].is_object());
}

#[test]
fn mcp_explain_decide_shows_unreachable_and_smtlib() {
    let (_lab, q) = lab_analyzed_decide();
    let out =
        ske_mcp::call_tool(&q, "explain", &json!({"outcome": "example.Decide/sd1/o0"})).unwrap();
    assert_eq!(out["status"], "PROVEN_UNREACHABLE");
    assert!(out["assumptions"].is_array());
    assert!(out["ir_outcomes"].is_array());
    // Every IR outcome carries SMT-LIB arrivals and a verdict when relevant.
    let ir = out["ir_outcomes"].as_array().unwrap();
    assert!(!ir.is_empty());
    let has_smt: usize = ir
        .iter()
        .filter(|o| {
            o["arrivals"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .any(|arr| !arr["smtlib"].as_str().unwrap_or("").is_empty())
                })
                .unwrap_or(false)
        })
        .count();
    assert!(has_smt > 0, "no arrival with SMT-LIB text: {out}");
    // For the unreachable outcome, arrivals must include UNSAT with SMT-LIB
    // text and the verifier verdict must be present (AC-0.7-04).
    let has_unsat_with_smt = ir.iter().any(|o| {
        o["arrivals"]
            .as_array()
            .map(|arr| {
                arr.iter().any(|a| {
                    a["result"] == "UNSAT"
                        && !a["smtlib"].as_str().unwrap_or("").is_empty()
                        && a["verifier_result"] == "UNSAT"
                })
            })
            .unwrap_or(false)
    });
    assert!(
        has_unsat_with_smt,
        "no UNSAT arrival with SMT-LIB + verdict: {out}"
    );
    // For the unreachable outcome, a proof verdict should exist.
    assert!(
        ir.iter()
            .any(|o| o["proof"].is_object() && o["proof"]["verdict"].as_str() == Some("ACCEPTED")),
        "no accepted proof verdict: {out}"
    );
}

#[test]
fn mcp_impact_helper_reports_entry_caller() {
    let lab = Lab::new();
    let q = Query::open(&lab.db).unwrap();
    let out = ske_mcp::call_tool(&q, "impact", &json!({"function": "example.helper"})).unwrap();
    let callers = out["callers"].as_array().unwrap();
    let names: Vec<&str> = callers
        .iter()
        .map(|c| c["qualified_name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"example.Entry"),
        "impact of helper must include Entry: {names:?}"
    );
    // `incomplete` is present and boolean (may be true/false depending on the
    // graph closure; never absent).
    assert!(out["incomplete"].is_boolean());
    // AC-0.7-02: impact is a bounded exploration with explicit truncation.
    assert!(out["page"].is_object());
    assert!(out["truncated"].is_boolean());
    assert!(out["visited_count"].is_u64());
    assert!(out["frontier_count"].is_u64());
    assert!(out["truncation_reason"].is_null() || out["truncation_reason"].is_string());
}

#[test]
fn mcp_rules_filtered_by_function_returns_only_matching_rules() {
    let lab = Lab::new();
    let q = Query::open(&lab.db).unwrap();
    q.reconstruct_behaviors("latest", None, "example.Check", 100, 0)
        .unwrap();
    q.reconstruct_behaviors("latest", None, "example.Decide", 100, 0)
        .unwrap();
    let out = ske_mcp::call_tool(
        &q,
        "rules",
        &json!({"function": "example.Check", "with_evidence": true}),
    )
    .unwrap();
    let rules = out["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 3);
    for rule in rules {
        assert!(rule["rule_key"]
            .as_str()
            .unwrap()
            .starts_with("example.Check@"));
    }
}

#[test]
fn mcp_explain_reports_paging_metadata() {
    let (_lab, q) = lab_analyzed_decide();
    let out =
        ske_mcp::call_tool(&q, "explain", &json!({"outcome": "example.Decide/sd1/o0"})).unwrap();
    assert!(out["truncated"].is_boolean());
    assert!(out["visited_count"].is_u64());
    assert!(out["frontier_count"].is_u64());
    assert!(out["page"].is_object());
}
