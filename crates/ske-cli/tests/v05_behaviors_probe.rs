mod go_support;
use go_support::*;

#[test]
fn probe_behaviors_check_decide() {
    let lab = Lab::new();
    let q = ske_query::Query::open(&lab.db).unwrap();
    for name in [
        "example.Check",
        "example.Decide",
        "example.Adjusted",
        "example.DivMod",
    ] {
        q.summarize_scalar("latest", None, name, &Default::default())
            .unwrap();
    }
    let c = lab.conn();
    for name in ["example.Check", "example.Decide"] {
        let behaviors = q
            .reconstruct_behaviors("latest", None, name, 100, 0)
            .unwrap();
        println!("===== {name} =====");
        println!("{}", serde_json::to_string(&behaviors).unwrap());
    }
    // Evidence counts and rule rows per AC-0.5-03 (Check only).
    let rules: i64 = c
        .query_row("SELECT COUNT(*) FROM functional_rules", [], |r| r.get(0))
        .unwrap();
    let evidence: i64 = c
        .query_row("SELECT COUNT(*) FROM knowledge_evidence", [], |r| r.get(0))
        .unwrap();
    let check_evidence: i64 = c.query_row(
        "SELECT COUNT(*) FROM knowledge_evidence ke JOIN functional_rules fr ON fr.id=ke.functional_rule_id WHERE fr.rule_key LIKE 'example.Check@%'",
        [], |r| r.get(0)).unwrap();
    println!("COUNTS rules={rules} evidence={evidence} check_evidence={check_evidence}");
    println!("DB_CHECK={}", q.db_check().unwrap());
}
