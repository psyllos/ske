mod common;
use common::Harness;
use serde_json::{json, Value};

const UNIT: &str = "go:example@linux/amd64";

fn branches(h: &Harness, function: &str) -> Value {
    h.ok(&["branches", "list", "--function", function])
}

#[test]
fn golden_check_inverted_polarity() {
    let h = Harness::new();
    h.ingest("check");
    let value = branches(&h, "example.Check");
    assert_eq!(value["function"], "example.Check");
    assert_eq!(value["unit_key"], UNIT);
    assert_eq!(value["analysis"], Value::Null);
    assert_eq!(
        value["inventory"],
        json!({
            "source_status":"COMPLETE", "ir_status":"COMPLETE",
            "source_decisions":2, "source_outcomes":4, "ir_decisions":2, "ir_outcomes":4
        })
    );
    let decisions = value["source_decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 2);
    for (index, (block, instruction, span, inverted)) in [
        ("b0", "i1", "check.go:9:5-9:13", false),
        ("b2", "i0", "check.go:12:5-12:13", true),
    ]
    .into_iter()
    .enumerate()
    {
        let decision = &decisions[index];
        assert_eq!(decision["id"], format!("example.Check/sd{index}"));
        assert_eq!(decision["kind"], "IF");
        assert_eq!(decision["span"], span);
        let outcomes = decision["outcomes"].as_array().unwrap();
        assert_eq!(outcomes.len(), 2);
        for (ordinal, label) in ["TRUE", "FALSE"].into_iter().enumerate() {
            let edge = if inverted { 1 - ordinal } else { ordinal };
            assert_eq!(
                outcomes[ordinal],
                json!({
                    "id":format!("example.Check/sd{index}/o{ordinal}"),
                    "label":label, "elided":false, "status":"UNANALYZED",
                    "links":[{
                        "ir_outcome":format!("example.Check/{block}/{instruction}/e{edge}"),
                        "mapping_kind":"DIRECT", "quality":"EXACT", "inverted":inverted
                    }]
                })
            );
        }
    }
    assert_eq!(
        value["ir_outcomes"],
        json!([
            {"id":"example.Check/b0/i1/e0","label":"TRUE","edge_kind":"BRANCH_TRUE","status":"UNANALYZED","explanation":null,"evidence":[]},
            {"id":"example.Check/b0/i1/e1","label":"FALSE","edge_kind":"BRANCH_FALSE","status":"UNANALYZED","explanation":null,"evidence":[]},
            {"id":"example.Check/b2/i0/e0","label":"TRUE","edge_kind":"BRANCH_TRUE","status":"UNANALYZED","explanation":null,"evidence":[]},
            {"id":"example.Check/b2/i0/e1","label":"FALSE","edge_kind":"BRANCH_FALSE","status":"UNANALYZED","explanation":null,"evidence":[]}
        ])
    );
    assert_eq!(
        value["page"],
        json!({"limit":100,"offset":0,"total":4,"truncated":false})
    );
}

#[test]
fn golden_switch_labels_and_edges() {
    let h = Harness::new();
    h.ingest("switch3");
    let value = branches(&h, "example.DaySwitch");
    let decisions = value["source_decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0]["id"], "example.DaySwitch/sd0");
    assert_eq!(decisions[0]["kind"], "SWITCH");
    assert_eq!(decisions[0]["span"], "switch_fixture.go:4:9-4:10");
    let outcomes = decisions[0]["outcomes"].as_array().unwrap();
    let ir = value["ir_outcomes"].as_array().unwrap();
    assert_eq!(outcomes.len(), 3);
    assert_eq!(ir.len(), 3);
    for (ordinal, (label, kind)) in [
        ("CASE:0", "SWITCH_CASE"),
        ("CASE:1", "SWITCH_CASE"),
        ("DEFAULT", "SWITCH_DEFAULT"),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("example.DaySwitch/b0/i0/e{ordinal}");
        assert_eq!(
            outcomes[ordinal],
            json!({
                "id":format!("example.DaySwitch/sd0/o{ordinal}"),
                "label":label, "elided":false,"status":"UNANALYZED",
                "links":[{"ir_outcome":id,"mapping_kind":"SWITCH_CASE","quality":"EXACT","inverted":false}]
            })
        );
        assert_eq!(
            ir[ordinal],
            json!({
                "id":id,"label":label,"edge_kind":kind,"status":"UNANALYZED","explanation":null,"evidence":[]
            })
        );
    }
    let cfg = h.ok(&["functions", "cfg", "--function", "example.DaySwitch"]);
    let successors = cfg["blocks"][0]["successors"].as_array().unwrap();
    assert_eq!(successors.len(), 3);
    for (ordinal, target) in ["b1", "b2", "b3"].into_iter().enumerate() {
        assert_eq!(
            successors[ordinal]["target"],
            format!("example.DaySwitch/{target}")
        );
        assert_eq!(successors[ordinal]["guard"], Value::Null);
    }
}

#[test]
fn golden_invoke_external_visibility() {
    let h = Harness::new();
    h.ingest("invoke");
    let visible = h.ok(&["functions", "list"]);
    assert_eq!(
        visible["functions"],
        json!([{
            "qualified_name":"example.Try","unit_key":UNIT,"is_external":false,
            "inventory_status":"COMPLETE","blocks":3,"source_decisions":0,"ir_outcomes":2
        }])
    );
    assert_eq!(
        visible["page"],
        json!({"limit":100,"offset":0,"total":1,"truncated":false})
    );
    let all = h.ok(&["functions", "list", "--include-external"]);
    assert_eq!(all["functions"][0], visible["functions"][0]);
    assert_eq!(
        all["functions"][1],
        json!({
            "qualified_name":"example.risky","unit_key":UNIT,"is_external":true,
            "inventory_status":"COMPLETE","blocks":0,"source_decisions":0,"ir_outcomes":0
        })
    );
    assert_eq!(all["page"]["total"], 2);
    let cfg = h.ok(&["functions", "cfg", "--function", "example.Try"]);
    assert_eq!(cfg["entry"], "example.Try/b0");
    assert_eq!(cfg["blocks"][0]["instructions"][0]["opcode"], "INVOKE");
    assert_eq!(
        cfg["blocks"][0]["successors"],
        json!([
            {"edge":"example.Try/b0/i0/e0","target":"example.Try/b1","kind":"NORMAL","label":"NORMAL","guard":null},
            {"edge":"example.Try/b0/i0/e1","target":"example.Try/b2","kind":"EXCEPTION","label":"EXCEPTION","guard":null}
        ])
    );
}

#[test]
fn golden_adjusted_phi_cfg() {
    let h = Harness::new();
    h.ingest("adjusted");
    let cfg = h.ok(&["functions", "cfg", "--function", "example.Adjusted"]);
    assert_eq!(cfg["function"], "example.Adjusted");
    assert_eq!(cfg["unit_key"], UNIT);
    assert_eq!(cfg["entry"], "example.Adjusted/b0");
    let blocks = cfg["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 3);
    for (ordinal, block) in blocks.iter().enumerate() {
        assert_eq!(block["id"], format!("example.Adjusted/b{ordinal}"));
        assert_eq!(block["ordinal"], ordinal);
    }
    assert_eq!(
        blocks[2]["instructions"],
        json!([
            {"id":"example.Adjusted/b2/i0","ordinal":0,"opcode":"PHI","operands":[],"result":"example.Adjusted/v2","span":null},
            {"id":"example.Adjusted/b2/i1","ordinal":1,"opcode":"SUB","operands":["example.Adjusted/v0","example.Adjusted/v2"],"result":"example.Adjusted/v3","span":null},
            {"id":"example.Adjusted/b2/i2","ordinal":2,"opcode":"RETURN","operands":["example.Adjusted/v3"],"result":null,"span":"adjusted.go:8:2-8:14"}
        ])
    );
    assert_eq!(
        blocks[1]["successors"],
        json!([{
            "edge":"example.Adjusted/b1/i0/e0","target":"example.Adjusted/b2",
            "kind":"UNCONDITIONAL","label":"NEXT","guard":null
        }])
    );
    assert_eq!(blocks[2]["successors"], json!([]));
}

#[test]
fn golden_snapshots_are_not_deduplicated() {
    let h = Harness::new();
    assert_eq!(h.ingest("decide")["snapshot_id"], 1);
    assert_eq!(h.ingest("decide")["snapshot_id"], 2);
    let value = h.ok(&["snapshots", "list"]);
    let snapshots = value["snapshots"].as_array().unwrap();
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0]["id"], 2);
    assert_eq!(snapshots[1]["id"], 1);
    for key in ["source_fingerprint", "config_fingerprint"] {
        let fingerprint = snapshots[0][key].as_str().unwrap();
        assert_eq!(fingerprint.len(), 64);
        assert!(fingerprint.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(snapshots[0][key], snapshots[1][key]);
    }
    for snapshot in snapshots {
        assert_eq!(snapshot["repository"], "cir-fixtures");
        assert_eq!(snapshot["status"], "CLOSED");
        assert_eq!(snapshot["inventory_status"], "COMPLETE");
    }
    assert_eq!(
        value["page"],
        json!({"limit":100,"offset":0,"total":2,"truncated":false})
    );
    let latest = h.ok(&["inspect"]);
    assert_eq!(latest["snapshot"]["id"], 2);
}
