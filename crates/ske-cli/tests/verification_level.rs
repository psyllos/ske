mod common;
mod go_support;
use common::Harness;
use go_support::{count, rows};
use serde_json::Value;

fn check_proofs(v: &Value, seen: &mut usize) {
    match v {
        Value::Object(o) => {
            if o.get("status").and_then(Value::as_str) == Some("PROVEN_UNREACHABLE") {
                let proof = o
                    .get("proof")
                    .or_else(|| {
                        o.get("evidence")?
                            .as_array()?
                            .iter()
                            .find_map(|e| e.get("proof"))
                    })
                    .expect("proof metadata must be public, not merely a proof id");
                assert_eq!(proof["verification_level"], "MODEL_UNREACHABLE");
                assert!(!proof["assumptions"].as_array().unwrap().is_empty());
                assert!(!proof["solver_version"].as_str().unwrap().is_empty());
                *seen += 1;
            }
            for v in o.values() {
                check_proofs(v, seen);
            }
        }
        Value::Array(a) => {
            for v in a {
                check_proofs(v, seen);
            }
        }
        _ => {}
    }
}

#[test]
fn prop_json_possible_vs_witnessed() {
    let h = Harness::new();
    h.ingest("decide");
    let run = h.ok(&["branches", "analyze", "--entry", "example.Decide"]);
    assert_eq!(run["counts"]["POSSIBLE"], 3);
    assert_eq!(run["counts"]["WITNESSED"], 0);
    let result = h.ok(&[
        "branches",
        "list",
        "--function",
        "example.Decide",
        "--run",
        "latest",
    ]);
    let mut seen = 0;
    check_proofs(&result, &mut seen);
    assert!(seen >= 1);
    assert!(!result.to_string().contains("CERTIFIED_UNREACHABLE"));
    assert_eq!(
        rows(
            &h.conn(),
            "SELECT verification_level FROM proof_records WHERE verdict='ACCEPTED'"
        ),
        vec![vec!["MODEL_UNREACHABLE".to_owned()]]
    );
    assert_eq!(
        count(
            &h.conn(),
            "SELECT COUNT(*) FROM branch_analysis_results WHERE status='WITNESSED'"
        ),
        0
    );
}

#[test]
fn prop_inventory_never_shrinks() {
    let h = Harness::new();
    h.ingest("decide");
    let tables = [
        "compilation_units",
        "source_files",
        "source_spans",
        "symbols",
        "functions",
        "values_ir",
        "basic_blocks",
        "instructions",
        "instruction_operands",
        "control_edges",
        "source_decisions",
        "source_outcomes",
        "ir_decisions",
        "ir_outcomes",
        "source_ir_links",
        "source_outcome_activation_cases",
        "source_outcome_activation_terms",
    ];
    let counts = || {
        tables
            .iter()
            .map(|table| {
                h.conn()
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
                        r.get::<_, i64>(0)
                    })
                    .unwrap()
            })
            .collect::<Vec<_>>()
    };
    let before = counts();
    let report = h.ok(&["branches", "analyze", "--entry", "example.Decide"]);
    assert_eq!(report["run_status"], "COMPLETED");
    assert_eq!(counts(), before);
    h.ok(&["db", "check"]);
}
