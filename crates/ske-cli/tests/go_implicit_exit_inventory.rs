mod go_support;
use go_support::*;

#[test]
fn implicit_panics_remain_visible_without_behavioral_completeness_claims() {
    let lab = Lab::new();
    let c = lab.conn();
    // Independent source-level witnesses. Do not infer expected sites from CIR
    // instructions: that would miss precisely the lowering omissions at issue.
    for (name, kind) in [
        ("example.MemNoCall", "NIL_DEREFERENCE"),
        ("example.MemOpaque", "NIL_DEREFERENCE"),
        ("example.MemAlias", "NIL_DEREFERENCE"),
        ("example.MemAlloc", "NIL_DEREFERENCE"),
        ("example.DivMod", "DIVISION_BY_ZERO"),
    ] {
        let fid = function(&c, name);
        assert!(count(&c, &format!("SELECT COUNT(*) FROM implicit_exit_sites WHERE function_id={fid} AND exit_kind='{kind}' AND modeling_status='UNMODELED'")) > 0, "{name}: missing source witness {kind}");
    }
    let sites = rows(&c, "SELECT x.function_id,x.local_key,x.exit_kind,x.modeling_status,x.source_span_id,s.start_line,s.start_column FROM implicit_exit_sites x LEFT JOIN source_spans s ON s.id=x.source_span_id ORDER BY x.function_id,s.start_line,s.start_column,x.exit_kind");
    assert!(!sites.is_empty(), "empty implicit-exit inventory");
    let mut previous = String::new();
    let mut ordinal = 0;
    for site in &sites {
        if site[0] != previous {
            previous.clone_from(&site[0]);
            ordinal = 0;
        }
        assert!(
            site[1].ends_with(&format!("/x{ordinal}")),
            "site identity/order: {site:?}"
        );
        ordinal += 1;
        assert_eq!(
            site[3], "UNMODELED",
            "v0.2 must not claim modeled panic edges"
        );
        assert!(
            [
                "NIL_DEREFERENCE",
                "INDEX_OUT_OF_RANGE",
                "TYPE_ASSERTION",
                "DIVISION_BY_ZERO",
                "NIL_MAP_WRITE",
                "CLOSED_CHANNEL",
                "NEGATIVE_SHIFT"
            ]
            .contains(&site[2].as_str())
                || site[2].starts_with("X_"),
            "invalid exit kind: {site:?}"
        );
        assert!(!site[4].is_empty(), "implicit exit without source span");
        let text = span_text(&c, site[4].parse().unwrap());
        assert!(!text.trim().is_empty(), "empty byte span: {site:?}");
        if site[2] == "DIVISION_BY_ZERO" {
            assert!(
                text.contains('/') || text.contains('%'),
                "division site does not point to division: {text:?}"
            );
        }
    }
    for fid in sites
        .iter()
        .map(|s| s[0].clone())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let n = count(
            &c,
            &format!("SELECT COUNT(*) FROM implicit_exit_sites WHERE function_id={fid}"),
        );
        let diagnostic = rows(&c, &format!("SELECT severity,message FROM diagnostics WHERE function_id={fid} AND code='IMPLICIT_PANIC_UNMODELED'"));
        assert_eq!(
            diagnostic.len(),
            1,
            "exactly one implicit panic diagnostic per function"
        );
        assert_eq!(diagnostic[0][0], "INFO");
        assert!(
            diagnostic[0][1]
                .split(|ch: char| !ch.is_ascii_digit())
                .any(|v| v == n.to_string()),
            "diagnostic must report {n} sites: {diagnostic:?}"
        );
    }
    // v0.2 inventories are structural. They must not silently manufacture
    // behavioral results/proofs for functions with unmodeled exits.
    for table in [
        "analysis_runs",
        "proof_records",
        "function_summaries",
        "behavior_scenarios",
    ] {
        assert_eq!(
            count(&c, &format!("SELECT COUNT(*) FROM {table}")),
            0,
            "unexpected behavioral completeness artifact: {table}"
        );
    }
    lab.ok(&["db", "check"]);
}
