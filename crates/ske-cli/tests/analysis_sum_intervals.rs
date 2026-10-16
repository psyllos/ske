mod go_support;
use go_support::Lab;

#[test]
fn analysis_sum_intervals() {
    let lab = Lab::new();
    let budget = lab.dir.path().join("intervals.toml");
    std::fs::write(
        &budget,
        "[budget]\nloop_mode = 'intervals'\nloop_unroll = 3\n",
    )
    .unwrap();
    let run = lab.ok(&[
        "branches",
        "analyze",
        "--entry",
        "example.Sum",
        "--budget",
        budget.to_str().unwrap(),
    ]);
    assert_eq!(run["run_status"], "COMPLETED");
    assert_eq!(run["exploration_closed"], true);
    assert_eq!(run["cut_count"], 0);
    assert!(run["assumptions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a == "LOOP_INTERVAL_ABSTRACTION"));
    let results = lab.ok(&[
        "branches",
        "list",
        "--function",
        "example.Sum",
        "--run",
        "latest",
    ]);
    let outcomes = results["ir_outcomes"].as_array().unwrap();
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes.iter().all(|o| o["status"] == "POSSIBLE"));
    let count: i64 = lab
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM diagnostics WHERE code='LOOP_ABSTRACTED'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(count > 0);
    assert_eq!(lab.ok(&["db", "check"])["ok"], true);
}
