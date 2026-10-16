use rusqlite::Connection;
use ske_storage::{map_sql_error, Storage, INITIAL_MIGRATION};
use std::collections::BTreeSet;

fn memory() -> Storage {
    Storage::open(":memory:").unwrap()
}

fn seed(db: &Storage) {
    db.conn
        .execute_batch(
            "INSERT INTO repositories VALUES(1,'repo','file:///repo');
         INSERT INTO snapshots(id,repository_id) VALUES(1,1),(2,1);
         INSERT INTO compilation_units(id,snapshot_id,unit_key,language,adapter_name,
          adapter_version,toolchain_version,build_fingerprint,semantic_profile,
          semantic_profile_hash,inventory_status)
         VALUES(1,1,'unit1','go','fixture','1','1','hash','fixture','hash','PARTIAL'),
               (2,2,'unit2','go','fixture','1','1','hash','fixture','hash','PARTIAL');
         INSERT INTO symbols(id,snapshot_id,unit_id,local_key,name,qualified_name,kind)
          VALUES(1,1,1,'f','f','f','FUNCTION'),(2,2,2,'f','f','f','FUNCTION');
         INSERT INTO functions(id,symbol_id,entry_block_id) VALUES(1,1,1),(2,2,2);
         INSERT INTO basic_blocks(id,function_id,local_key,ordinal) VALUES(1,1,'b',0),(2,2,'b',0);
         INSERT INTO instructions(id,block_id,local_key,ordinal,opcode,is_terminator)
          VALUES(1,1,'i',0,'BRANCH',1),(2,2,'i',0,'BRANCH',1);
         INSERT INTO control_edges(id,local_key,source_block_id,target_block_id,
          terminator_instruction_id,ordinal,label,edge_kind)
          VALUES(1,'e1',1,1,1,0,'TRUE','BRANCH_TRUE'),
                (2,'e2',1,1,1,1,'FALSE','BRANCH_FALSE'),
                (3,'e3',2,2,2,0,'TRUE','BRANCH_TRUE');
         INSERT INTO ir_decisions VALUES(1,1,'BRANCH'),(2,2,'BRANCH');
         INSERT INTO ir_outcomes VALUES(1,1,1,0,'TRUE'),(2,1,2,1,'FALSE'),(3,2,3,0,'TRUE');
         INSERT INTO analysis_runs(id,snapshot_id,analysis_kind,engine_version,
          config_fingerprint,assumptions_fingerprint,status,exploration_closed)
          VALUES(1,1,'BRANCH_ANALYSIS','fixture','hash','hash','COMPLETED',1),
                (2,1,'BRANCH_ANALYSIS','fixture','hash','hash','COMPLETED',1);
         INSERT INTO solver_queries(id,analysis_run_id,query_fingerprint,result,smtlib)
          VALUES(1,1,'q','UNSAT','(check-sat)');
         INSERT INTO outcome_arrivals(id,analysis_run_id,ir_outcome_id,arrival_seq,result,soundness)
          VALUES(1,1,1,0,'UNSAT','EXACT');
         INSERT INTO analysis_assumptions(id,analysis_run_id,kind,description)
          VALUES(1,1,'SEQUENTIAL_EXECUTION','fixture');",
        )
        .unwrap();
}

fn proof(db: &Storage, run: i64, outcome: i64, verdict: &str) -> i64 {
    db.conn
        .execute(
            "INSERT INTO proof_records(analysis_run_id,ir_outcome_id,verifier_name,
         verifier_version,verdict,verification_level,model_soundness,
         eliminated_arrivals,residual_frontier)
         VALUES(?1,?2,'fixture','1',?3,'MODEL_UNREACHABLE','EXACT',1,0)",
            rusqlite::params![run, outcome, verdict],
        )
        .unwrap();
    db.conn.last_insert_rowid()
}

fn result(
    db: &Storage,
    run: i64,
    outcome: i64,
    status: &str,
    proof: Option<i64>,
) -> rusqlite::Result<usize> {
    db.conn.execute(
        "INSERT INTO branch_analysis_results(analysis_run_id,ir_outcome_id,status,soundness,proof_id)
         VALUES(?1,?2,?3,'EXACT',?4)",
        rusqlite::params![run, outcome, status, proof],
    )
}

fn rejected(conn: &Connection, sql: &str, code: &str) {
    let error = map_sql_error(conn.execute_batch(sql).unwrap_err());
    assert_eq!(error.code, code, "{sql}: {error}");
}

#[test]
fn schema_tables_match() {
    let db = memory();
    let spec = include_str!("../../../SKE_Specification_Implementation_v1.4.md");
    let section = spec.split("## 8.2.").nth(1).unwrap();
    let literal = section
        .split("```sql\n")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    assert_eq!(INITIAL_MIGRATION.as_bytes(), literal.as_bytes());
    let table_list = section
        .split("`repositories, snapshots,")
        .nth(1)
        .unwrap()
        .split('`')
        .next()
        .unwrap();
    let mut expected: BTreeSet<String> =
        table_list.split(',').map(|s| s.trim().to_owned()).collect();
    expected.extend([
        "repositories".to_owned(),
        "snapshots".to_owned(),
        "schema_migrations".to_owned(),
    ]);
    let mut stmt = db
        .conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .unwrap();
    let actual = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<BTreeSet<_>>>()
        .unwrap();
    assert_eq!(actual.len(), 47);
    assert_eq!(actual, expected);
    let expected_triggers: BTreeSet<_> = literal
        .split("CREATE TRIGGER ")
        .skip(1)
        .map(|s| s.split_whitespace().next().unwrap().to_owned())
        .collect();
    let mut stmt = db
        .conn
        .prepare("SELECT name FROM sqlite_master WHERE type='trigger'")
        .unwrap();
    let actual = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<BTreeSet<_>>>()
        .unwrap();
    assert_eq!(actual.len(), 21);
    assert_eq!(actual, expected_triggers);
    assert_eq!(
        db.db_check().unwrap()["checks"].as_array().unwrap().len(),
        22
    );
    assert_eq!(db.db_check().unwrap()["ok"], true);
}

#[test]
fn migrations_idempotent_wal_and_checksum() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ske.db");
    for _ in 0..2 {
        let db = Storage::open(&path).unwrap();
        let (count, version): (i64, i64) = db
            .conn
            .query_row(
                "SELECT COUNT(*),MAX(version) FROM schema_migrations",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((count, version), (1, 1));
        assert_eq!(
            db.conn
                .query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "wal"
        );
        assert_eq!(
            db.conn
                .query_row("PRAGMA busy_timeout", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            5000
        );
    }
    let db = Storage::open(&path).unwrap();
    db.conn
        .execute("UPDATE schema_migrations SET checksum='tampered'", [])
        .unwrap();
    drop(db);
    let err = Storage::open(path).err().unwrap();
    assert_eq!(err.code, "E_MIGRATION_CHECKSUM");
    assert_eq!(err.exit_code(), 6);
}

#[test]
fn connection_has_fk_on() {
    let dir = tempfile::tempdir().unwrap();
    for _ in 0..3 {
        let db = Storage::open(dir.path().join("ske.db")).unwrap();
        assert_eq!(
            db.conn
                .query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        rejected(
            &db.conn,
            "INSERT INTO snapshots(repository_id) VALUES(999)",
            "E_DANGLING_REF",
        );
    }
}

#[test]
fn closed_snapshot_rejects_structural_writes() {
    let db = memory();
    seed(&db);
    let writer = db.open_snapshot_writer(1).unwrap();
    writer
        .execute(
            "UPDATE snapshots SET status='CLOSED',source_fingerprint='s',
        config_fingerprint='c',closed_at=CURRENT_TIMESTAMP WHERE id=?1",
            [1],
        )
        .unwrap();
    assert_eq!(
        db.open_snapshot_writer(1).err().unwrap().code,
        "E_SNAPSHOT_CLOSED"
    );
    assert_eq!(
        writer
            .execute(
                "INSERT INTO source_files(snapshot_id,path,language,content_hash)
        VALUES(1,'x','go','hash')",
                []
            )
            .unwrap_err()
            .code,
        "E_SNAPSHOT_CLOSED"
    );
    let closed = db.closed_snapshot(1).unwrap();
    assert_eq!(closed.inventory_status().unwrap(), "PARTIAL");
    closed
        .add_analysis_run("BRANCH_ANALYSIS", "1", "c", "a")
        .unwrap();
    assert_eq!(
        db.open_snapshot_writer(999).err().unwrap().code,
        "E_NOT_FOUND"
    );
    db.conn
        .execute("UPDATE snapshots SET status='ABORTED' WHERE id=2", [])
        .unwrap();
    assert_eq!(
        db.open_snapshot_writer(2).err().unwrap().code,
        "E_SNAPSHOT_CLOSED"
    );
}

#[test]
fn trigger_proof_required_and_witnessed() {
    let db = memory();
    seed(&db);
    for status in ["PROVEN_UNREACHABLE", "WITNESSED"] {
        let error = map_sql_error(result(&db, 1, 1, status, None).unwrap_err());
        assert_eq!(
            error.code,
            if status == "WITNESSED" {
                "E_WITNESSED_NOT_PRODUCIBLE"
            } else {
                "E_PROOF_REQUIRED"
            }
        );
    }
    let rejected_proof = proof(&db, 1, 1, "REJECTED");
    let other_outcome = proof(&db, 1, 2, "ACCEPTED");
    let other_run = proof(&db, 2, 1, "ACCEPTED");
    for p in [rejected_proof, other_outcome, other_run] {
        assert_eq!(
            map_sql_error(result(&db, 1, 1, "PROVEN_UNREACHABLE", Some(p)).unwrap_err()).code,
            "E_PROOF_REQUIRED"
        );
    }
    let p = proof(&db, 1, 1, "ACCEPTED");
    result(&db, 1, 1, "PROVEN_UNREACHABLE", Some(p)).unwrap();
    result(&db, 2, 2, "POSSIBLE", None).unwrap();
    rejected(
        &db.conn,
        "UPDATE branch_analysis_results SET status='WITNESSED' WHERE analysis_run_id=2",
        "E_WITNESSED_NOT_PRODUCIBLE",
    );
    rejected(
        &db.conn,
        "UPDATE branch_analysis_results SET status='PROVEN_UNREACHABLE' WHERE analysis_run_id=2",
        "E_PROOF_REQUIRED",
    );
}

#[test]
fn trigger_proof_accept_only_closed() {
    for mutation in ["status='RUNNING'", "exploration_closed=0", "cut_count=1"] {
        let db = memory();
        seed(&db);
        db.conn
            .execute_batch(&format!("UPDATE analysis_runs SET {mutation} WHERE id=1"))
            .unwrap();
        rejected(&db.conn,
            "INSERT INTO proof_records(analysis_run_id,ir_outcome_id,verifier_name,verifier_version,
             verdict,verification_level,model_soundness,eliminated_arrivals,residual_frontier)
             VALUES(1,1,'test','1','ACCEPTED','MODEL_UNREACHABLE','EXACT',1,0)",
            "E_PROOF_REQUIRED");
    }
}

#[test]
fn proof_immutability_proof_and_support() {
    let db = memory();
    seed(&db);
    let p = proof(&db, 1, 1, "ACCEPTED");
    result(&db, 1, 1, "PROVEN_UNREACHABLE", Some(p)).unwrap();
    rejected(
        &db.conn,
        "UPDATE proof_records SET verdict='REJECTED'",
        "E_PROOF_IMMUTABLE",
    );
    rejected(&db.conn, "DELETE FROM proof_records", "E_PROOF_IMMUTABLE");
    for sql in [
        "INSERT INTO analysis_assumptions(analysis_run_id,kind,description) VALUES(1,'PARAM_RANGE','x')",
        "UPDATE analysis_assumptions SET description='changed' WHERE id=1",
        "DELETE FROM analysis_assumptions WHERE id=1",
        "INSERT INTO solver_queries(analysis_run_id,query_fingerprint,result) VALUES(1,'q2','SAT')",
        "UPDATE solver_queries SET result='SAT' WHERE id=1",
        "DELETE FROM solver_queries WHERE id=1",
        "INSERT INTO outcome_arrivals(analysis_run_id,ir_outcome_id,arrival_seq,result,soundness) VALUES(1,1,1,'SAT','EXACT')",
        "UPDATE outcome_arrivals SET result='SAT' WHERE id=1",
        "DELETE FROM outcome_arrivals WHERE id=1",
        "DELETE FROM analysis_runs WHERE id=1",
        "UPDATE branch_analysis_results SET explanation='changed'",
        "DELETE FROM branch_analysis_results",
    ] {
        rejected(&db.conn,sql,"E_PROOF_SUPPORT_FROZEN");
    }
    for assignment in [
        "snapshot_id=2",
        "entry_function_id=2",
        "engine_version='other'",
        "solver_name='other'",
        "solver_version='other'",
        "context_name='other'",
        "context_json='[]'",
        "budget_json='[]'",
        "config_fingerprint='other'",
        "assumptions_fingerprint='other'",
        "exploration_closed=0",
        "cut_count=1",
        "status='CANCELLED'",
        "status='RUNNING'",
        "status='PARTIAL'",
        "status='FAILED'",
        "status='PENDING'",
    ] {
        rejected(
            &db.conn,
            &format!("UPDATE analysis_runs SET {assignment} WHERE id=1"),
            "E_PROOF_SUPPORT_FROZEN",
        );
    }
    db.conn.execute_batch("UPDATE analysis_runs SET metrics_json='{\"replays\":1}',finished_at=CURRENT_TIMESTAMP,status='COMPLETED' WHERE id=1;
        INSERT INTO analysis_runs(snapshot_id,analysis_kind,engine_version,config_fingerprint,assumptions_fingerprint,status)
        VALUES(1,'BRANCH_ANALYSIS','1','c','a','PENDING');").unwrap();
    assert_eq!(
        db.conn
            .query_row(
                "SELECT COUNT(*) FROM solver_queries WHERE analysis_run_id=1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    let saved: (String,String,i64,i64) = db.conn.query_row(
        "SELECT verdict,verification_level,analysis_run_id,ir_outcome_id FROM proof_records WHERE id=?1",
        [p],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    assert_eq!(saved, ("ACCEPTED".into(), "MODEL_UNREACHABLE".into(), 1, 1));
}

#[test]
fn source_activation_integrity() {
    let db = memory();
    seed(&db);
    db.conn.execute_batch(
        "INSERT INTO source_files VALUES(1,1,'x.go','go','hash');
         INSERT INTO source_spans VALUES(1,1,1,1,1,2);
         INSERT INTO source_decisions VALUES(1,1,1,1,NULL,'sd','IF',1);
         INSERT INTO source_outcomes(id,decision_id,local_key,ordinal,label) VALUES(1,1,'so',0,'TRUE');
         INSERT INTO source_outcome_activation_cases VALUES(1,1,0,'EXACT');"
    ).unwrap();
    let chk = || db.db_check().unwrap()["checks"][21].clone();
    assert_eq!(chk()["rows"][0], "EMPTY_CASE:1");
    rejected(
        &db.conn,
        "INSERT INTO source_outcome_activation_terms VALUES(1,0,999)",
        "E_DANGLING_REF",
    );
    rejected(
        &db.conn,
        "INSERT INTO source_outcome_activation_terms VALUES(999,0,1)",
        "E_DANGLING_REF",
    );
    db.conn
        .execute_batch("INSERT INTO source_outcome_activation_terms VALUES(1,0,3)")
        .unwrap();
    assert_eq!(chk()["rows"][0], "CROSS_UNIT:1:0");
    db.conn
        .execute_batch(
            "UPDATE source_outcome_activation_terms SET ir_outcome_id=1;
        UPDATE source_outcomes SET activation_mapping_complete=1",
        )
        .unwrap();
    assert_eq!(chk()["violations"], 0);
    db.conn
        .execute_batch("UPDATE source_outcome_activation_cases SET quality='APPROX'")
        .unwrap();
    assert_eq!(chk()["rows"][0], "COMPLETE_NONEXACT:1");
    db.conn
        .execute_batch("UPDATE source_outcomes SET elided=1")
        .unwrap();
    assert_eq!(chk()["rows"][0], "ELIDED_WITH_CASE:1");
    db.conn
        .execute_batch(
            "DELETE FROM source_outcome_activation_terms;
        DELETE FROM source_outcome_activation_cases;
        UPDATE source_outcomes SET elided=0",
        )
        .unwrap();
    assert_eq!(chk()["rows"][0], "COMPLETE_NO_CASE:1");
    db.conn
        .execute_batch("UPDATE source_outcomes SET activation_mapping_complete=0")
        .unwrap();
    assert_eq!(chk()["violations"], 0);
    assert_eq!(
        db.conn
            .query_row("SELECT COUNT(*) FROM source_outcomes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    let spec = include_str!("../../../SKE_Specification_Implementation_v1.4.md");
    let sql = spec
        .split("**SQL normatif `chk_22.sql`**")
        .nth(1)
        .unwrap()
        .split("```sql\n")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    assert_eq!(include_str!("../sql/checks/chk_22.sql"), sql);
}

#[test]
fn db_check_corruption_and_warning_report() {
    for (index, mutation) in [
        (3, "DELETE FROM outcome_arrivals; DELETE FROM ir_outcomes; DELETE FROM ir_decisions;
             DELETE FROM control_edges; DELETE FROM instructions WHERE id=1"),
        (4, "UPDATE functions SET entry_block_id=NULL WHERE id=1"),
        (5, "INSERT INTO instructions(block_id,local_key,ordinal,opcode) VALUES(1,'late',1,'NOP')"),
        (6, "UPDATE control_edges SET source_block_id=2 WHERE id=1"),
        (7, "UPDATE control_edges SET target_block_id=2 WHERE id=1"),
        (8, "UPDATE functions SET entry_block_id=2 WHERE id=1"),
        (9, "UPDATE ir_outcomes SET decision_id=2,ordinal=9 WHERE id=1"),
        (11, "DELETE FROM outcome_arrivals; DELETE FROM ir_outcomes WHERE decision_id=1;
              DELETE FROM ir_decisions WHERE id=1"),
        (12, "UPDATE compilation_units SET source_decision_count=99 WHERE id=1"),
        (13, "UPDATE snapshots SET status='CLOSED',inventory_status='COMPLETE',
              source_fingerprint='s',config_fingerprint='c',closed_at=CURRENT_TIMESTAMP WHERE id=1"),
        (15, "INSERT INTO branch_analysis_results(analysis_run_id,ir_outcome_id,status,soundness)
              VALUES(1,3,'POSSIBLE','EXACT')"),
        (17, "INSERT INTO symbolic_expressions(id,snapshot_id,expression_hash,kind) VALUES(1,1,'h','CONST');
              INSERT INTO expression_operands VALUES(1,0,1)"),
        (20, "UPDATE outcome_arrivals SET ir_outcome_id=3 WHERE id=1"),
    ] {
        let db = memory();
        seed(&db);
        db.conn.execute_batch(mutation).unwrap();
        let report = db.db_check().unwrap();
        assert_eq!(report["ok"],false,"{mutation}");
        assert!(report["checks"][index]["violations"].as_u64().unwrap()>0,"{mutation}: {report}");
    }
    let db = memory();
    db.conn.execute_batch(
        "INSERT INTO repositories VALUES(1,'r','r');
         INSERT INTO snapshots(id,repository_id) VALUES(1,1);
         WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<25)
         INSERT INTO analysis_runs(id,snapshot_id,analysis_kind,engine_version,config_fingerprint,
          assumptions_fingerprint,status) SELECT x,1,'BRANCH_ANALYSIS','1','c','a','PENDING' FROM n;"
    ).unwrap();
    let report = db.db_check().unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(report["checks"][18]["severity"], "WARNING");
    assert_eq!(report["checks"][18]["violations"], 25);
    assert_eq!(report["checks"][18]["rows"].as_array().unwrap().len(), 20);
    assert!(report["checks"][0].get("rows").is_none());
}

#[test]
fn db_check_types_phi_spans_and_unlinked_source() {
    for (index, mutation) in [
        (1, "UPDATE cir_types SET snapshot_id=2 WHERE id=1"),
        (2, "UPDATE values_ir SET function_id=2 WHERE id=1"),
        (
            10,
            "INSERT INTO instructions(id,block_id,local_key,ordinal,opcode,result_value_id)
              VALUES(3,1,'phi',1,'PHI',1)",
        ),
        (
            10,
            "INSERT INTO instructions(id,block_id,local_key,ordinal,opcode,result_value_id)
              VALUES(3,1,'phi',1,'PHI',1);
              INSERT INTO phi_inputs VALUES(3,0,2,1)",
        ),
        (16, "UPDATE source_files SET snapshot_id=2 WHERE id=1"),
        (
            19,
            "UPDATE compilation_units SET inventory_status='COMPLETE' WHERE id=1",
        ),
    ] {
        let db = memory();
        seed(&db);
        db.conn
            .execute_batch(
                "INSERT INTO cir_types VALUES(1,1,1,'int','BITVECTOR','int',X'00');
             INSERT INTO values_ir(id,snapshot_id,unit_id,function_id,local_key,type_id,value_role)
              VALUES(1,1,1,1,'value',1,'SSA');
             UPDATE symbols SET type_id=1 WHERE id=1;
             INSERT INTO source_files VALUES(1,1,'x.go','go','h');
             INSERT INTO source_spans VALUES(1,1,1,1,1,2);
             UPDATE symbols SET primary_span_id=1 WHERE id=1;
             INSERT INTO source_decisions VALUES(1,1,1,1,NULL,'sd','IF',1);
             INSERT INTO source_outcomes(id,decision_id,local_key,ordinal,label)
              VALUES(1,1,'so',0,'TRUE');",
            )
            .unwrap();
        db.conn.execute_batch(mutation).unwrap();
        let report = db.db_check().unwrap();
        assert_eq!(report["ok"], false);
        assert!(
            report["checks"][index]["violations"].as_u64().unwrap() > 0,
            "{mutation}: {report}"
        );
    }
}

#[test]
fn writer_savepoint_rollback_and_stale_connection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ske.db");
    let db = Storage::open(&path).unwrap();
    seed(&db);
    db.conn.execute_batch("BEGIN IMMEDIATE").unwrap();
    let writer = db.open_snapshot_writer(1).unwrap();
    writer
        .execute(
            "INSERT INTO source_files(snapshot_id,path,language,content_hash)
        VALUES(?1,'x.go','go','h')",
            [writer.snapshot_id()],
        )
        .unwrap();
    assert_eq!(
        writer
            .execute(
                "INSERT INTO source_files(snapshot_id,path,language,content_hash)
        VALUES(999,'orphan.go','go','h')",
                []
            )
            .unwrap_err()
            .code,
        "E_DANGLING_REF"
    );
    assert!(!db.conn.is_autocommit());
    db.conn.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        db.conn
            .query_row("SELECT COUNT(*) FROM source_files", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let second = Storage::open(&path).unwrap();
    second
        .open_snapshot_writer(1)
        .unwrap()
        .execute(
            "UPDATE snapshots SET status='CLOSED',source_fingerprint='s',config_fingerprint='c',
         closed_at=CURRENT_TIMESTAMP WHERE id=?1",
            [1],
        )
        .unwrap();
    assert_eq!(
        writer
            .execute("UPDATE snapshots SET revision='illegal' WHERE id=1", [])
            .unwrap_err()
            .code,
        "E_SNAPSHOT_CLOSED"
    );
    assert!(db.conn.is_autocommit());
}

#[test]
fn trigger_rejects_unsupported_or_residual_proof_and_allows_valid_update() {
    for (soundness, residual) in [("UNSUPPORTED", 0), ("EXACT", 1)] {
        let db = memory();
        seed(&db);
        db.conn
            .execute(
                "INSERT INTO proof_records(id,analysis_run_id,ir_outcome_id,verifier_name,
             verifier_version,verdict,verification_level,model_soundness,
             eliminated_arrivals,residual_frontier)
             VALUES(1,1,1,'test','1','ACCEPTED','MODEL_UNREACHABLE',?1,1,?2)",
                rusqlite::params![soundness, residual],
            )
            .unwrap();
        assert_eq!(
            map_sql_error(result(&db, 1, 1, "PROVEN_UNREACHABLE", Some(1)).unwrap_err()).code,
            "E_PROOF_REQUIRED"
        );
    }
    let db = memory();
    seed(&db);
    result(&db, 1, 1, "POSSIBLE", None).unwrap();
    let p = proof(&db, 1, 1, "ACCEPTED");
    db.conn
        .execute(
            "UPDATE branch_analysis_results SET status='PROVEN_UNREACHABLE',proof_id=?1",
            [p],
        )
        .unwrap();
    assert_eq!(db.db_check().unwrap()["checks"][14]["violations"], 0);
}
