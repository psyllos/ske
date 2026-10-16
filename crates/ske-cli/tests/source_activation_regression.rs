mod go_support;
use go_support::*;

#[test]
fn short_circuit_activation_uses_ordered_joint_cases() {
    let lab = Lab::new();
    assert_activation(&lab.conn());
    for name in ["OrDecision", "AndDecision", "MixedDecision"] {
        let name = format!("example.{name}");
        lab.ok(&["branches", "analyze", "--entry", &name]);
        let v = lab.ok(&["branches", "list", "--function", &name, "--run", "latest"]);
        // Both outcomes of every unconstrained boolean source atom and
        // compound expression are reachable.
        for d in v["source_decisions"].as_array().unwrap() {
            for o in d["outcomes"].as_array().unwrap() {
                assert_ne!(o["status"], "PROVEN_UNREACHABLE", "{name}: {o}");
                assert!(o["activation_mapping_complete"].is_boolean());
                assert!(o["activation_case_count"].is_u64());
            }
        }
    }
    lab.ok(&["branches", "analyze", "--entry", "example.AllocTwice"]);
    let v = lab.ok(&[
        "branches",
        "list",
        "--function",
        "example.AllocTwice",
        "--run",
        "latest",
    ]);
    // The final condition is *first != 1. Concrete execution takes FALSE,
    // returning true; earlier loop conditions are not the return predicate.
    let last = v["source_decisions"].as_array().unwrap().last().unwrap();
    let taken = last["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["label"] == "FALSE")
        .expect("final IF FALSE");
    assert_ne!(taken["status"], "PROVEN_UNREACHABLE");
}

#[test]
fn incomplete_activation_is_unknown_and_elided_unanalyzed() {
    let lab = Lab::new();
    let c = lab.conn();
    let ds = decisions(&c, function(&c, "example.OrDecision"));
    let os = outcomes(&c, &ds[0][0]);
    // Mutation of trusted test inventory before any proof exists.
    c.execute(
        "UPDATE source_outcomes SET activation_mapping_complete=0 WHERE id=?",
        [&os[0][0]],
    )
    .unwrap();
    c.execute("DELETE FROM source_outcome_activation_terms WHERE case_id IN (SELECT id FROM source_outcome_activation_cases WHERE source_outcome_id=?)", [&os[1][0]]).unwrap();
    c.execute(
        "DELETE FROM source_outcome_activation_cases WHERE source_outcome_id=?",
        [&os[1][0]],
    )
    .unwrap();
    c.execute(
        "UPDATE source_outcomes SET elided=1 WHERE id=?",
        [&os[1][0]],
    )
    .unwrap();
    lab.ok(&["branches", "analyze", "--entry", "example.OrDecision"]);
    let v = lab.ok(&[
        "branches",
        "list",
        "--function",
        "example.OrDecision",
        "--run",
        "latest",
    ]);
    assert_eq!(v["source_decisions"][0]["outcomes"][0]["status"], "UNKNOWN");
    assert_eq!(
        v["source_decisions"][0]["outcomes"][1]["status"],
        "UNANALYZED"
    );
}
