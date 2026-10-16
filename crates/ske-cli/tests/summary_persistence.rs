mod go_support;
use go_support::*;

#[test]
fn summary_persistence_passes_real_cli_db_check() {
    let lab = Lab::new();
    let query = ske_query::Query::open(&lab.db).unwrap();
    for (name, cases, returns) in [
        ("example.Entry", 2, 2),
        ("example.DivMod", 2, 4),
        ("example.Divide2", 3, 6),
        ("example.Fact", 4, 4),
    ] {
        let summary = query
            .summarize_scalar("latest", None, name, &Default::default())
            .unwrap();
        assert_eq!(summary["cases"], cases);
        let id = summary["summary_id"].as_i64().unwrap();
        let count:i64=lab.conn().query_row(
            "SELECT COUNT(*) FROM summary_case_returns r JOIN summary_cases c ON c.id=r.summary_case_id
             WHERE c.summary_id=?1",[id],|r| r.get(0)).unwrap();
        assert_eq!(count, returns);
    }
    assert_eq!(lab.ok(&["db", "check"])["ok"], true);
}
