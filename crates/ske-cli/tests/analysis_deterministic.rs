mod common;
mod go_support;
use common::Harness;
use go_support::rows;

#[test]
fn analysis_deterministic() {
    let h = Harness::new();
    h.ingest("decide");
    let mut projections = Vec::new();
    for _ in 0..2 {
        let run = h.ok(&[
            "branches",
            "analyze",
            "--snapshot",
            "latest",
            "--entry",
            "example.Decide",
        ]);
        let id = run["run_id"].as_i64().unwrap();
        let c = h.conn();
        projections.push((
            rows(&c, &format!("SELECT o.ordinal,r.status,COALESCE(r.explanation,'') FROM branch_analysis_results r JOIN ir_outcomes o ON o.id=r.ir_outcome_id WHERE r.analysis_run_id={id} ORDER BY o.id")),
            rows(&c, &format!("SELECT query_fingerprint FROM solver_queries WHERE analysis_run_id={id} ORDER BY id")),
        ));
    }
    assert!(!projections[0].0.is_empty());
    assert!(!projections[0].1.is_empty());
    assert_eq!(projections[0], projections[1]);
}
