mod common;
use common::Harness;

fn corrupted(sql: &str, expected: &str) {
    let h = Harness::new();
    h.ingest("decide");
    let conn = h.conn();
    conn.execute_batch(sql).unwrap();
    // Corruption must remain referentially valid, rather than merely testing FK checks.
    let violations: i64 = conn
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);
    drop(conn);
    let (exit, value) = h.run(&["db", "check"]);
    assert_eq!(exit, 6, "{value}");
    assert_eq!(value["ok"], false);
    let checks = value["checks"].as_array().unwrap();
    assert_eq!(checks.len(), 22);
    assert!(
        checks.iter().any(|check| {
            (check["id"] == expected || check["code"] == expected)
                && check["violations"].as_u64().is_some_and(|n| n >= 1)
        }),
        "missing {expected}: {value}"
    );
}

#[test]
fn missing_terminator_and_edges_chk04() {
    // Select a RETURN terminator: it has no edges/IR outcomes referencing it.
    corrupted(
        "DELETE FROM instruction_operands WHERE instruction_id=(
            SELECT id FROM instructions WHERE opcode='RETURN' ORDER BY id LIMIT 1);
         DELETE FROM instructions WHERE id=(
            SELECT id FROM instructions WHERE opcode='RETURN' ORDER BY id LIMIT 1);",
        "CHK-04",
    );
}

#[test]
fn edge_source_mismatch_chk07() {
    corrupted(
        "UPDATE control_edges SET source_block_id=(
            SELECT id FROM basic_blocks WHERE id != control_edges.source_block_id ORDER BY id LIMIT 1)
         WHERE id=(SELECT MIN(id) FROM control_edges);",
        "CHK-07",
    );
}

#[test]
fn counter_mismatch_chk13() {
    corrupted(
        "UPDATE compilation_units SET source_decision_count=99;",
        "CHK-13",
    );
}

#[test]
fn expression_cycle_chk18() {
    corrupted(
        "INSERT INTO symbolic_expressions(id,snapshot_id,expression_hash,kind)
            SELECT 1001,id,'corrupt-parent','CONST' FROM snapshots LIMIT 1;
         INSERT INTO expression_operands(expression_id,ordinal,operand_expression_id)
            VALUES(1001,0,1001);",
        "CHK-18",
    );
}
