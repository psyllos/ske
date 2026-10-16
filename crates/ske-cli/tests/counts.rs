mod common;
use common::Harness;

fn check(name: &str, column: usize) {
    let h = Harness::new();
    h.ingest(name);
    let tables = [
        ("source_files", [1, 1, 1, 1, 0, 1, 1]),
        ("source_spans", [6, 8, 3, 2, 0, 6, 7]),
        ("cir_types", [3, 5, 3, 2, 3, 4, 6]),
        ("symbols", [2, 4, 2, 2, 3, 2, 3]),
        ("functions", [1, 1, 1, 1, 2, 1, 1]),
        ("values_ir", [7, 9, 7, 7, 3, 9, 11]),
        ("basic_blocks", [5, 5, 3, 4, 3, 5, 5]),
        ("instructions", [7, 8, 6, 4, 3, 9, 11]),
        ("instruction_operands", [9, 9, 6, 4, 3, 11, 13]),
        ("phi_inputs", [0, 0, 2, 0, 0, 0, 0]),
        ("control_edges", [4, 4, 3, 3, 2, 4, 4]),
        ("control_edge_case_values", [0, 0, 0, 3, 0, 0, 0]),
        ("source_decisions", [2, 2, 1, 1, 0, 2, 2]),
        ("source_outcomes", [4, 4, 2, 3, 0, 4, 4]),
        ("ir_decisions", [2, 2, 1, 1, 1, 2, 2]),
        ("ir_outcomes", [4, 4, 2, 3, 2, 4, 4]),
        ("source_ir_links", [4, 4, 2, 3, 0, 4, 4]),
        ("source_outcome_activation_cases", [4, 4, 2, 3, 0, 4, 4]),
        ("source_outcome_activation_terms", [4, 4, 2, 3, 0, 4, 4]),
        ("call_sites", [0, 0, 0, 0, 1, 0, 1]),
        ("call_targets", [0, 0, 0, 0, 1, 0, 0]),
        ("diagnostics", [0, 0, 0, 0, 0, 0, 0]),
    ];
    let mut actual = h.counts();
    for (table, counts) in tables {
        assert_eq!(
            actual.remove(table),
            Some(counts[column]),
            "{name}: {table}"
        );
    }
    for table in ["repositories", "snapshots", "compilation_units"] {
        assert_eq!(actual.remove(table), Some(1), "{table}");
    }
    // v0.1 schema + v0.5 capabilities migration (schema_migrations 2 rows).
    assert_eq!(
        actual.remove("schema_migrations"),
        Some(2),
        "schema_migrations"
    );
    for (table, count) in actual {
        assert_eq!(count, 0, "unexpected rows in {table}");
    }
    let check = h.ok(&["db", "check"]);
    assert_eq!(check["ok"], true);
    assert_eq!(check["checks"].as_array().unwrap().len(), 22);
    for entry in check["checks"].as_array().unwrap() {
        assert_eq!(entry["violations"], 0);
    }
}
#[test]
fn counts_decide() {
    check("decide", 0);
}
#[test]
fn counts_check() {
    check("check", 1);
}
#[test]
fn counts_adjusted() {
    check("adjusted", 2);
}
#[test]
fn counts_switch3() {
    check("switch3", 3);
}
#[test]
fn counts_invoke() {
    check("invoke", 4);
}
#[test]
fn counts_mem_nocall() {
    check("mem_nocall", 5);
}
#[test]
fn counts_mem_opaque() {
    check("mem_opaque", 6);
}
