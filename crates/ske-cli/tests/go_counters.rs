mod go_support;
use go_support::*;

#[test]
fn independent_ast_counters_match_persisted_inventory() {
    let lab = Lab::new();
    let c = lab.conn();
    let units = rows(&c, "SELECT u.unit_key,u.expected_source_decisions,u.source_decision_count,COUNT(d.id) FROM compilation_units u LEFT JOIN source_decisions d ON d.unit_id=u.id GROUP BY u.id ORDER BY u.unit_key");
    assert_eq!(units.len(), 3);
    // Totals transcribed by summing the manual §15.1.5 decisions, not SKE output.
    for (unit, (suffix, expected)) in units.iter().zip([
        ("go:example/partial@linux/amd64", 1),
        ("go:example/tags@linux/amd64", 0),
        ("go:example@linux/amd64", 52),
    ]) {
        assert_eq!(unit[0], suffix);
        for value in &unit[1..] {
            assert_eq!(value, &expected.to_string(), "{}", unit[0]);
        }
    }
}
