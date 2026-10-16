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
        // §19 v0.4 adds exactly four IF decisions in interproc.go. Preserve
        // the original v0.2 count independently instead of replacing 52 by
        // whatever total the extractor happened to emit.
        let extension = if suffix == "go:example@linux/amd64" {
            4
        } else {
            0
        };
        let added: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM source_decisions d
             JOIN compilation_units u ON u.id=d.unit_id
             JOIN source_spans span ON span.id=d.source_span_id
             JOIN source_files file ON file.id=span.source_file_id
             WHERE u.unit_key=?1 AND file.path='interproc.go'",
                [suffix],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(added, extension, "normative v0.4 fixture decisions");
        assert_eq!(
            unit[3].parse::<i64>().unwrap() - added,
            expected,
            "original v0.2 inventory"
        );
        for value in &unit[1..] {
            assert_eq!(value, &(expected + extension).to_string(), "{}", unit[0]);
        }
    }
}
