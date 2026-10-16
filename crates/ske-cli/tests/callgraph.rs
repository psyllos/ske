//! AC-0.4-01 (graphe d'appels + SCC) and AC-0.4-02 (chemins statiques).
mod go_support;
use go_support::*;

fn count(c: &rusqlite::Connection, sql: &str) -> i64 {
    c.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn callgraph_resolution_statuses() {
    let lab = Lab::new();
    let c = lab.conn();
    // example.Entry -> example.helper : static, RESOLVED_UNIQUE, closed world.
    let (status, closed): (String, bool) = c
        .query_row(
            "SELECT cs.resolution_status, cs.world_closed FROM call_sites cs
             JOIN instructions i ON i.id=cs.instruction_id
             JOIN basic_blocks b ON b.id=i.block_id
             JOIN functions f ON f.id=b.function_id
             JOIN symbols s ON s.id=f.symbol_id
             WHERE s.qualified_name='example.Entry' AND i.opcode='CALL'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "RESOLVED_UNIQUE");
    assert!(
        closed,
        "Entry->helper must be world_closed for RESOLVED_UNIQUE"
    );
    let helper: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM call_targets ct
             JOIN call_sites cs ON cs.id=ct.call_site_id
             JOIN instructions i ON i.id=cs.instruction_id
             JOIN basic_blocks b ON b.id=i.block_id
             JOIN functions f ON f.id=b.function_id
             JOIN symbols s ON s.id=f.symbol_id
             JOIN symbols t ON t.id=(SELECT symbol_id FROM functions WHERE id=ct.target_function_id)
             WHERE s.qualified_name='example.Entry' AND t.qualified_name='example.helper'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(helper, 1, "Entry must target helper exactly once");

    // example.CallDoer : interface Doer with no implementation => UNRESOLVED,
    // no call_targets, and CALL_TARGET_UNRESOLVED diagnostic.
    let (doer_status, doer_world): (String, bool) = c
        .query_row(
            "SELECT cs.resolution_status, cs.world_closed FROM call_sites cs
             JOIN instructions i ON i.id=cs.instruction_id
             JOIN basic_blocks b ON b.id=i.block_id
             JOIN functions f ON f.id=b.function_id
             JOIN symbols s ON s.id=f.symbol_id
             WHERE s.qualified_name='example.CallDoer'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(doer_status, "UNRESOLVED");
    assert!(!doer_world, "an unresolved interface call is not closed");
    assert_eq!(
        count(
            &c,
            "SELECT COUNT(*) FROM call_targets ct
             JOIN call_sites cs ON cs.id=ct.call_site_id
             JOIN instructions i ON i.id=cs.instruction_id
             JOIN basic_blocks b ON b.id=i.block_id
             JOIN functions f ON f.id=b.function_id
             JOIN symbols s ON s.id=f.symbol_id
             WHERE s.qualified_name='example.CallDoer'",
        ),
        0,
        "CallDoer must have no resolved target"
    );
    assert!(
        count(
            &c,
            "SELECT COUNT(*) FROM diagnostics WHERE code='CALL_TARGET_UNRESOLVED'",
        ) >= 1,
        "CALL_TARGET_UNRESOLVED diagnostic must be recorded"
    );

    // example.Total (v0.2) : interface with implementations =>
    // RESOLVED_POSSIBLE and the CHA target set (value + pointer receivers).
    let (total_status, total_targets): (String, i64) = c
        .query_row(
            "SELECT cs.resolution_status,
                    (SELECT COUNT(*) FROM call_targets ct WHERE ct.call_site_id=cs.id)
             FROM call_sites cs
             JOIN instructions i ON i.id=cs.instruction_id
             JOIN basic_blocks b ON b.id=i.block_id
             JOIN functions f ON f.id=b.function_id
             JOIN symbols s ON s.id=f.symbol_id
             WHERE s.qualified_name='example.Total'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(total_status, "RESOLVED_POSSIBLE");
    assert!(
        total_targets >= 2,
        "Total must resolve its interface implementations"
    );
    let mut targets: Vec<String> = {
        let mut stmt = c
            .prepare(
                "SELECT t.qualified_name FROM call_targets ct
                 JOIN call_sites cs ON cs.id=ct.call_site_id
                 JOIN instructions i ON i.id=cs.instruction_id
                 JOIN basic_blocks b ON b.id=i.block_id
                 JOIN functions f ON f.id=b.function_id
                 JOIN symbols s ON s.id=f.symbol_id
                 JOIN functions tf ON tf.id=ct.target_function_id
                 JOIN symbols t ON t.id=tf.symbol_id
                 WHERE s.qualified_name='example.Total' ORDER BY t.qualified_name",
            )
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    targets.sort();
    assert_eq!(
        targets,
        vec![
            "example.(*Rect).Area".to_string(),
            "example.(*Square).Area".to_string(),
            "example.Rect.Area".to_string(),
            "example.Square.Area".to_string(),
        ]
    );
}

#[test]
fn callgraph_scc_fact_is_self_loop() {
    let lab = Lab::new();
    let c = lab.conn();
    // Build the subgraph of calls reachable from example.Fact.
    let edges: Vec<(i64, i64)> = {
        let mut stmt = c
            .prepare(
                "SELECT bfn.id AS caller_id, cfn.id AS callee_id
                 FROM call_targets ct
                 JOIN call_sites cs ON cs.id=ct.call_site_id
                 JOIN instructions i ON i.id=cs.instruction_id
                 JOIN basic_blocks b ON b.id=i.block_id
                 JOIN functions bfn ON bfn.id=b.function_id
                 JOIN functions cfn ON cfn.id=ct.target_function_id",
            )
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    let fact: i64 = c
        .query_row(
            "SELECT f.id FROM functions f JOIN symbols s ON s.id=f.symbol_id WHERE s.qualified_name='example.Fact'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    // Dense index all participants; we only need to test that Fact's SCC is
    // exactly {Fact}. With only the self edge, the SCC algorithm returns the
    // singleton component.
    let nodes: std::collections::BTreeSet<i64> = edges.iter().flat_map(|(a, b)| [*a, *b]).collect();
    let mut index = std::collections::BTreeMap::new();
    for (i, n) in nodes.iter().enumerate() {
        index.insert(*n, i);
    }
    let g = ske_graph::Graph::new(
        nodes.len(),
        edges
            .iter()
            .filter_map(|(a, b)| Some((*index.get(a)?, *index.get(b)?))),
    )
    .unwrap();
    let scc = g.strongly_connected_components();
    let fact_comp = scc
        .iter()
        .find(|comp| comp.contains(&index[&fact]))
        .expect("Fact must appear in an SCC");
    assert_eq!(
        fact_comp.len(),
        1,
        "Fact's SCC must be a singleton auto-loop"
    );
    assert!(fact_comp[0] == index[&fact]);
}

#[test]
fn paths_explore_static_reachability() {
    let lab = Lab::new();
    // Entry -> helper : POTENTIALLY_REACHABLE, one static path, not incomplete.
    let entry_to_helper = lab.ok(&[
        "paths",
        "explore",
        "--source",
        "example.Entry",
        "--target",
        "example.helper",
    ]);
    assert_eq!(entry_to_helper["status"], "POTENTIALLY_REACHABLE");
    assert_eq!(entry_to_helper["incomplete"], false);
    let paths = entry_to_helper["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 1);
    let calls = paths[0]["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["caller"], "example.Entry");
    assert_eq!(calls[0]["callsite"], "example.Entry/b0/i0");
    assert_eq!(calls[0]["callee"], "example.helper");
    assert_eq!(calls[0]["resolution"], "STATIC");
    assert_eq!(entry_to_helper["visited_count"], 2);
    assert_eq!(entry_to_helper["frontier_count"], 0);
    assert_eq!(entry_to_helper["page"]["total"], 1);
    assert_eq!(entry_to_helper["truncated"], false);
    assert!(entry_to_helper["truncation_reason"].is_null());
    assert_eq!(entry_to_helper["source"], "example.Entry");
    assert_eq!(entry_to_helper["target"], "example.helper");

    // helper -> Entry : no path, not incomplete.
    let helper_to_entry = lab.ok(&[
        "paths",
        "explore",
        "--source",
        "example.helper",
        "--target",
        "example.Entry",
    ]);
    assert_eq!(helper_to_entry["status"], "NO_PATH_FOUND");
    assert_eq!(helper_to_entry["incomplete"], false);
    assert_eq!(helper_to_entry["paths"].as_array().unwrap().len(), 0);

    // CallDoer (UNRESOLVED interface) -> helper : incomplete=true.
    let calldoer = lab.ok(&[
        "paths",
        "explore",
        "--source",
        "example.CallDoer",
        "--target",
        "example.helper",
    ]);
    assert_eq!(calldoer["incomplete"], true);
}

#[test]
fn paths_limits_and_pagination_are_distinct() {
    let lab = Lab::new();
    let base = [
        "paths",
        "explore",
        "--source",
        "example.Entry",
        "--target",
        "example.helper",
    ];
    for option in ["--max-nodes", "--max-depth", "--max-paths"] {
        let mut args = base.to_vec();
        args.extend([option, "0"]);
        let result = lab.ok(&args);
        assert_eq!(result["truncated"], true, "{option}");
        assert!(result["frontier_count"].as_u64().unwrap() > 0, "{option}");
        assert_eq!(result["page"]["truncated"], false);
    }
    let mut args = base.to_vec();
    args.extend(["--limit", "0"]);
    let result = lab.ok(&args);
    assert_eq!(result["page"]["total"], 1);
    assert_eq!(result["page"]["truncated"], true);
    assert_eq!(result["truncated"], false);
    let mut args = base.to_vec();
    args.extend(["--max-paths", "1"]);
    assert_eq!(
        lab.ok(&args)["truncated"],
        false,
        "one exhaustive path must not be marked truncated"
    );
    let identity = lab.ok(&[
        "paths",
        "explore",
        "--source",
        "example.helper",
        "--target",
        "example.helper",
    ]);
    assert_eq!(identity["paths"][0]["calls"].as_array().unwrap().len(), 0);
    assert_eq!(identity["page"]["truncated"], false);
}
