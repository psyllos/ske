//! Embedded SQLite schema, integrity checks, and snapshot access boundaries.
use rusqlite::{types::ValueRef, Connection, OptionalExtension, Params};
use serde_json::{json, Value};
use ske_common::{sha256_hex, Error, Result};
use std::{path::Path, time::Duration};

pub const INITIAL_MIGRATION: &str = include_str!("../../../migrations/0001_initial.sql");
pub const CAPABILITIES_MIGRATION: &str = include_str!("../../../migrations/0002_capabilities.sql");
const CHECKS: [&str; 22] = [
    include_str!("../sql/checks/chk_01.sql"),
    include_str!("../sql/checks/chk_02.sql"),
    include_str!("../sql/checks/chk_03.sql"),
    include_str!("../sql/checks/chk_04.sql"),
    include_str!("../sql/checks/chk_05.sql"),
    include_str!("../sql/checks/chk_06.sql"),
    include_str!("../sql/checks/chk_07.sql"),
    include_str!("../sql/checks/chk_08.sql"),
    include_str!("../sql/checks/chk_09.sql"),
    include_str!("../sql/checks/chk_10.sql"),
    include_str!("../sql/checks/chk_11.sql"),
    include_str!("../sql/checks/chk_12.sql"),
    include_str!("../sql/checks/chk_13.sql"),
    include_str!("../sql/checks/chk_14.sql"),
    include_str!("../sql/checks/chk_15.sql"),
    include_str!("../sql/checks/chk_16.sql"),
    include_str!("../sql/checks/chk_17.sql"),
    include_str!("../sql/checks/chk_18.sql"),
    include_str!("../sql/checks/chk_19.sql"),
    include_str!("../sql/checks/chk_20.sql"),
    include_str!("../sql/checks/chk_21.sql"),
    include_str!("../sql/checks/chk_22.sql"),
];

/// Translate SQLite failures into stable domain codes.
pub fn map_sql_error(error: rusqlite::Error) -> Error {
    let message = error.to_string();
    let code = [
        "E_PROOF_REQUIRED",
        "E_PROOF_IMMUTABLE",
        "E_PROOF_SUPPORT_FROZEN",
        "E_WITNESSED_NOT_PRODUCIBLE",
    ]
    .into_iter()
    .find(|code| message.contains(code))
    .unwrap_or_else(|| {
        if message.contains("idx_one_terminator_per_block")
            || message == "UNIQUE constraint failed: instructions.block_id"
        {
            "E_MULTI_TERMINATOR"
        } else if message.contains("basic_blocks.function_id, basic_blocks.ordinal")
            || message.contains("instructions.block_id, instructions.ordinal")
            || message.contains("control_edges.terminator_instruction_id, control_edges.ordinal")
        {
            "E_ORDINAL_DUPLICATE"
        } else if message.contains("FOREIGN KEY constraint failed") {
            "E_DANGLING_REF"
        } else if message.contains("compilation_units.snapshot_id, compilation_units.unit_key") {
            "E_UNIT_DUPLICATE"
        } else {
            "E_INTERNAL"
        }
    });
    Error::new(code, message)
}

#[derive(Debug)]
pub struct Storage {
    pub conn: Connection,
}

impl Storage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_busy_timeout(path, Duration::from_millis(5000))
    }

    pub fn open_with_busy_timeout(path: impl AsRef<Path>, timeout: Duration) -> Result<Self> {
        let mut conn = Connection::open(path).map_err(map_sql_error)?;
        conn.busy_timeout(timeout).map_err(map_sql_error)?;
        conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")
            .map_err(map_sql_error)?;
        migrate(&mut conn)?;
        Ok(Self { conn })
    }

    /// Always returns the full report, including on integrity violations.
    /// The CLI maps `ok == false` to E_DB_INTEGRITY / exit 6.
    pub fn db_check(&self) -> Result<Value> {
        let tx = self.conn.unchecked_transaction().map_err(map_sql_error)?;
        let mut checks = Vec::new();
        let mut ok = true;
        for (index, sql) in CHECKS.iter().enumerate() {
            let mut statement = tx.prepare(sql).map_err(map_sql_error)?;
            let columns = statement.column_count();
            let mut rows = statement.query([]).map_err(map_sql_error)?;
            let mut count = 0usize;
            let mut sample = Vec::new();
            while let Some(row) = rows.next().map_err(map_sql_error)? {
                count += 1;
                if sample.len() < 20 {
                    let values = (0..columns)
                        .map(|column| {
                            Ok(match row.get_ref(column).map_err(map_sql_error)? {
                                ValueRef::Null => Value::Null,
                                ValueRef::Integer(n) => json!(n),
                                ValueRef::Real(n) => json!(n),
                                ValueRef::Text(s) => json!(String::from_utf8_lossy(s)),
                                ValueRef::Blob(b) => json!(b),
                            })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    sample.push(if columns == 1 {
                        values.into_iter().next().unwrap()
                    } else {
                        json!(values)
                    });
                }
            }
            let severity = if index == 18 { "WARNING" } else { "ERROR" };
            let mut check = json!({
                "id": format!("CHK-{:02}", index + 1),
                "severity": severity,
                "violations": count,
            });
            if count > 0 {
                check["rows"] = json!(sample);
                if severity == "ERROR" {
                    ok = false;
                }
            }
            checks.push(check);
        }
        tx.commit().map_err(map_sql_error)?;
        Ok(json!({"ok": ok, "checks": checks}))
    }

    pub fn open_snapshot_writer(&self, snapshot_id: i64) -> Result<OpenSnapshotWriter<'_>> {
        require_status(&self.conn, snapshot_id, "OPEN")?;
        Ok(OpenSnapshotWriter {
            conn: &self.conn,
            snapshot_id,
        })
    }

    pub fn closed_snapshot(&self, snapshot_id: i64) -> Result<ClosedSnapshot<'_>> {
        require_status(&self.conn, snapshot_id, "CLOSED")?;
        Ok(ClosedSnapshot {
            conn: &self.conn,
            snapshot_id,
        })
    }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY, description TEXT NOT NULL,
            checksum TEXT NOT NULL, applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );",
    )
    .map_err(map_sql_error)?;
    // The immediate lock covers both checking and applying, so concurrent opens
    // cannot race into applying the same migration.
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(map_sql_error)?;
    let checksum = sha256_hex(INITIAL_MIGRATION);
    let applied: Option<String> = tx
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version=1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_sql_error)?;
    match applied {
        Some(previous) if previous != checksum => {
            return Err(Error::new(
                "E_MIGRATION_CHECKSUM",
                "Migration 0001 checksum differs",
            ));
        }
        Some(_) => {}
        None => {
            tx.execute_batch(INITIAL_MIGRATION).map_err(map_sql_error)?;
            tx.execute(
                "INSERT INTO schema_migrations(version,description,checksum) VALUES(1,'initial',?1)",
                [checksum],
            ).map_err(map_sql_error)?;
        }
    }
    // v0.5: migration 0002 (capabilities), same immediate transaction, its own checksum record.
    let capabilities_checksum = sha256_hex(CAPABILITIES_MIGRATION);
    let applied: Option<String> = tx
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version=2",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_sql_error)?;
    match applied {
        Some(previous) if previous != capabilities_checksum => {
            return Err(Error::new(
                "E_MIGRATION_CHECKSUM",
                "Migration 0002 checksum differs",
            ));
        }
        Some(_) => {}
        None => {
            tx.execute_batch(CAPABILITIES_MIGRATION)
                .map_err(map_sql_error)?;
            tx.execute(
                "INSERT INTO schema_migrations(version,description,checksum) VALUES(2,'capabilities',?1)",
                [capabilities_checksum],
            ).map_err(map_sql_error)?;
        }
    }
    tx.commit().map_err(map_sql_error)
}

fn require_status(conn: &Connection, id: i64, expected: &str) -> Result<()> {
    let status: Option<String> = conn
        .query_row("SELECT status FROM snapshots WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .optional()
        .map_err(map_sql_error)?;
    match status {
        None => Err(Error::new(
            "E_NOT_FOUND",
            format!("Snapshot {id} not found"),
        )),
        Some(status) if status == expected => Ok(()),
        Some(_) => Err(Error::new(
            "E_SNAPSHOT_CLOSED",
            format!("Snapshot {id} is not {expected}"),
        )),
    }
}

/// A checked structural-write capability. SQL remains the importer's responsibility:
/// callers must bind all inserted rows to `snapshot_id`.
#[derive(Debug)]
pub struct OpenSnapshotWriter<'a> {
    conn: &'a Connection,
    snapshot_id: i64,
}

impl OpenSnapshotWriter<'_> {
    pub fn snapshot_id(&self) -> i64 {
        self.snapshot_id
    }

    /// Recheck under an immediate transaction/savepoint before each operation.
    /// This also rejects a stale writer after closure through another handle.
    pub fn execute(&self, sql: &str, params: impl Params) -> Result<usize> {
        let nested = !self.conn.is_autocommit();
        self.conn
            .execute_batch(if nested {
                "SAVEPOINT ske_writer"
            } else {
                "BEGIN IMMEDIATE"
            })
            .map_err(map_sql_error)?;
        let result = require_status(self.conn, self.snapshot_id, "OPEN")
            .and_then(|()| self.conn.execute(sql, params).map_err(map_sql_error));
        match result {
            Ok(count) => {
                self.conn
                    .execute_batch(if nested {
                        "RELEASE ske_writer"
                    } else {
                        "COMMIT"
                    })
                    .map_err(map_sql_error)?;
                Ok(count)
            }
            Err(error) => {
                self.conn
                    .execute_batch(if nested {
                        "ROLLBACK TO ske_writer; RELEASE ske_writer"
                    } else {
                        "ROLLBACK"
                    })
                    .map_err(map_sql_error)?;
                Err(error)
            }
        }
    }
}

/// Read-only snapshot view. New analysis runs are created separately, not through
/// a structural writer.
#[derive(Debug)]
pub struct ClosedSnapshot<'a> {
    conn: &'a Connection,
    snapshot_id: i64,
}

impl ClosedSnapshot<'_> {
    pub fn snapshot_id(&self) -> i64 {
        self.snapshot_id
    }

    pub fn inventory_status(&self) -> Result<String> {
        self.conn
            .query_row(
                "SELECT inventory_status FROM snapshots WHERE id=?1",
                [self.snapshot_id],
                |r| r.get(0),
            )
            .map_err(map_sql_error)
    }

    pub fn add_analysis_run(
        &self,
        analysis_kind: &str,
        engine_version: &str,
        config_fingerprint: &str,
        assumptions_fingerprint: &str,
    ) -> Result<i64> {
        require_status(self.conn, self.snapshot_id, "CLOSED")?;
        self.conn
            .execute(
                "INSERT INTO analysis_runs(snapshot_id,analysis_kind,engine_version,
             config_fingerprint,assumptions_fingerprint,status) VALUES(?1,?2,?3,?4,?5,'PENDING')",
                rusqlite::params![
                    self.snapshot_id,
                    analysis_kind,
                    engine_version,
                    config_fingerprint,
                    assumptions_fingerprint
                ],
            )
            .map_err(map_sql_error)?;
        Ok(self.conn.last_insert_rowid())
    }
}
