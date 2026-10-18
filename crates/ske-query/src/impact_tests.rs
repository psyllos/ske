//! Real SQLite graph regressions, using the unchanged production migrations.
use super::impact;
use rusqlite::{params, Connection};
use serde_json::Value;

fn graph() -> Connection {
    let storage = ske_storage::Storage::open(":memory:").unwrap();
    let db = storage.conn;
    db.execute_batch(
        "INSERT INTO repositories(id,name,root_uri) VALUES(1,'test','file:///test');
         INSERT INTO snapshots(id,repository_id) VALUES(1,1);
         INSERT INTO compilation_units(id,snapshot_id,unit_key,language,adapter_name,
             adapter_version,toolchain_version,build_fingerprint,semantic_profile,
             semantic_profile_hash,inventory_status)
         VALUES(1,1,'u','go','test','1','1','x','{}','x','COMPLETE'),
               (2,1,'v','go','test','1','1','x','{}','x','COMPLETE');",
    )
    .unwrap();
    for (id, unit, name) in [(1, 1, "helper"), (2, 1, "Entry"), (3, 1, "Other")] {
        function(&db, id, unit, name);
    }
    db
}

fn function(db: &Connection, id: i64, unit: i64, name: &str) {
    db.execute(
        "INSERT INTO symbols(id,snapshot_id,unit_id,local_key,name,qualified_name,kind)
         VALUES(?1,1,?2,?3,?4,?4,'FUNCTION')",
        params![id, unit, format!("{unit}|{name}"), name],
    )
    .unwrap();
    db.execute("INSERT INTO functions(id,symbol_id) VALUES(?1,?1)", [id])
        .unwrap();
    db.execute(
        "INSERT INTO basic_blocks(id,function_id,local_key,ordinal) VALUES(?1,?1,'entry',0)",
        [id],
    )
    .unwrap();
}

fn site(db: &Connection, id: i64, caller: i64, targets: &[i64], resolution: &str, closed: bool) {
    db.execute(
        "INSERT INTO instructions(id,block_id,local_key,ordinal,opcode)
         SELECT ?1,?2,u.unit_key||'|call'||?1,?1,'CALL'
         FROM functions f JOIN symbols s ON s.id=f.symbol_id
         JOIN compilation_units u ON u.id=s.unit_id WHERE f.id=?2",
        params![id, caller],
    )
    .unwrap();
    db.execute(
        "INSERT INTO call_sites(id,instruction_id,dispatch_kind,resolution_status,world_closed)
         VALUES(?1,?1,'STATIC',?2,?3)",
        params![id, resolution, closed],
    )
    .unwrap();
    for target in targets {
        db.execute(
            "INSERT INTO call_targets VALUES(?1,?2,'STATIC')",
            params![id, target],
        )
        .unwrap();
    }
}

fn callers(out: &Value) -> Vec<i64> {
    out["callers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["function_id"].as_i64().unwrap())
        .collect()
}

#[test]
fn impact_helper_entry_with_unresolved_site_on_affected_path() {
    let db = graph();
    site(&db, 1, 2, &[1], "RESOLVED_UNIQUE", true);
    site(&db, 2, 2, &[], "UNRESOLVED", false);
    let out = impact(&db, 1, "helper").unwrap();
    assert_eq!(callers(&out), vec![2]);
    assert_eq!(out["callers"][0]["qualified_name"], "Entry");
    assert_eq!(out["incomplete"], true);
    assert_eq!(out["incomplete_sites"][0]["callsite"], "call2");
    assert_eq!(out["incomplete_sites"][0]["on_affected_path"], true);
}

#[test]
fn impact_follows_all_open_targets_and_deduplicates_cycles() {
    let db = graph();
    site(&db, 1, 2, &[1, 3], "UNRESOLVED", false);
    site(&db, 2, 2, &[1], "RESOLVED_UNIQUE", true);
    site(&db, 3, 3, &[2], "RESOLVED_UNIQUE", true);
    site(&db, 4, 1, &[3], "RESOLVED_UNIQUE", true);
    let out = impact(&db, 1, "helper").unwrap();
    assert_eq!(callers(&out), vec![2, 3]);
    assert_eq!(out["incomplete"], true);
    assert_eq!(out["incomplete_sites"].as_array().unwrap().len(), 1);
}

#[test]
fn impact_ambiguity_and_unit_identity() {
    let db = graph();
    function(&db, 4, 2, "helper");
    function(&db, 5, 2, "Entry");
    site(&db, 1, 2, &[1], "RESOLVED_UNIQUE", true);
    site(&db, 2, 5, &[4], "RESOLVED_UNIQUE", true);
    let error = impact(&db, 1, "helper").unwrap_err();
    assert_eq!(error.code, "E_AMBIGUOUS");
    assert_eq!(error.candidates, vec!["u|helper", "v|helper"]);
    assert_eq!(callers(&impact(&db, 1, "u|helper").unwrap()), vec![2]);
    let out = impact(&db, 1, "v|helper").unwrap();
    assert_eq!(callers(&out), vec![5]);
    assert_eq!(out["callers"][0]["unit_key"], "v");
    assert_eq!(out["incomplete"], false);
}

#[test]
fn impact_empty_known_callers_does_not_claim_closed_graph() {
    let db = graph();
    assert_eq!(impact(&db, 1, "helper").unwrap()["incomplete"], false);
    site(&db, 1, 3, &[], "UNRESOLVED", false);
    let out = impact(&db, 1, "helper").unwrap();
    assert!(callers(&out).is_empty());
    assert_eq!(out["incomplete"], true);
    assert_eq!(out["incomplete_sites"][0]["on_affected_path"], false);
}

#[test]
fn impact_marks_open_resolved_and_external_sites() {
    for (resolution, closed, external) in [
        ("RESOLVED_UNIQUE", false, false),
        ("EXTERNAL", true, false),
        ("RESOLVED_UNIQUE", true, true),
    ] {
        let db = graph();
        site(&db, 1, 2, &[1], "RESOLVED_UNIQUE", true);
        if external {
            db.execute("UPDATE functions SET is_external=1 WHERE id=3", [])
                .unwrap();
        }
        site(&db, 2, 2, &[3], resolution, closed);
        let out = impact(&db, 1, "helper").unwrap();
        assert_eq!(callers(&out), vec![2]);
        assert_eq!(out["incomplete"], true, "{out}");
        assert_eq!(out["incomplete_sites"][0]["on_affected_path"], true);
    }
}
