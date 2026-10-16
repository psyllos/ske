mod go_support;
use go_support::*;

#[test]
fn specified_diagnostics_and_inventory_propagation() {
    let lab = Lab::new();
    let c = lab.conn();
    assert_eq!(
        rows(
            &c,
            "SELECT unit_key,inventory_status FROM compilation_units ORDER BY unit_key"
        ),
        vec![
            vec!["go:example/partial@linux/amd64", "PARTIAL"],
            vec!["go:example/tags@linux/amd64", "COMPLETE"],
            vec!["go:example@linux/amd64", "COMPLETE"],
        ]
    );
    assert_eq!(
        rows(&c, "SELECT status,inventory_status FROM snapshots"),
        vec![vec!["CLOSED", "PARTIAL"]]
    );
    assert_eq!(
        count(
            &c,
            "SELECT COUNT(*) FROM diagnostics WHERE severity='ERROR'"
        ),
        0
    );
    for (name, code, severity, impact) in [
        (
            "example.Pipe",
            "CONCURRENCY_PARTIAL",
            "WARNING",
            "CONCURRENCY",
        ),
        (
            "example.Pipe$1",
            "CONCURRENCY_PARTIAL",
            "WARNING",
            "CONCURRENCY",
        ),
        (
            "example/partial.Safe",
            "DEFER_UNMODELED",
            "WARNING",
            "EFFECTS",
        ),
        (
            "example/partial.Safe$1",
            "RECOVER_UNMODELED",
            "WARNING",
            "INVENTORY",
        ),
    ] {
        let fid = function(&c, name);
        assert_eq!(rows(&c, &format!("SELECT severity,impact FROM diagnostics WHERE function_id={fid} AND code='{code}'")), vec![vec![severity,impact]], "{name}/{code}");
    }
    assert_eq!(
        rows(
            &c,
            &format!(
                "SELECT inventory_status FROM functions WHERE id={}",
                function(&c, "example/partial.Safe$1")
            )
        ),
        vec![vec!["PARTIAL"]]
    );
    for g in golden() {
        let name = g["function"].as_str().unwrap();
        let fid = function(&c, name);
        if let Some(required) = g["diagnostics_required"].as_array() {
            for code in required {
                assert!(count(&c, &format!("SELECT COUNT(*) FROM diagnostics WHERE function_id={fid} AND code='{}'", code.as_str().unwrap())) > 0, "{name}: {code}");
            }
        }
        if let Some(forbidden) = g["diagnostics_forbidden_severity"].as_array() {
            for severity in forbidden {
                assert_eq!(count(&c, &format!("SELECT COUNT(*) FROM diagnostics WHERE function_id={fid} AND severity='{}'", severity.as_str().unwrap())), 0, "{name}/{severity}");
            }
        }
    }
    let diagnostics = rows(&c, "SELECT COALESCE(s.qualified_name,''),d.code,d.severity FROM diagnostics d JOIN compilation_units u ON u.id=d.unit_id LEFT JOIN functions f ON f.id=d.function_id LEFT JOIN symbols s ON s.id=f.symbol_id WHERE u.unit_key='go:example@linux/amd64'");
    assert!(!diagnostics.is_empty());
    for d in diagnostics {
        if (d[0] == "example.Pipe" || d[0] == "example.Pipe$1") && d[1] == "CONCURRENCY_PARTIAL" {
            continue;
        }
        assert_eq!(d[2], "INFO", "unlisted warning/error: {d:?}");
        assert!(
            [
                "IMPLICIT_PANIC_UNMODELED",
                "EXTERNAL_CALL_OPAQUE",
                "CALL_TARGET_APPROX",
                "OPAQUE_OP_EMITTED",
                "GENERIC_ORIGIN_OPAQUE",
                "SYNTHETIC_FUNCTION",
                "OUTCOME_ELIDED_BY_LOWERING"
            ]
            .contains(&d[1].as_str()),
            "unlisted diagnostic: {d:?}"
        );
    }
}
