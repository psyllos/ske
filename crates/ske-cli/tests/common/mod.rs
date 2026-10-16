#![allow(dead_code)]
use rusqlite::Connection;
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, process::Command};

pub struct Harness {
    pub dir: tempfile::TempDir,
    pub db: PathBuf,
}
impl Harness {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("k.sqlite");
        let h = Self { dir, db };
        h.ok(&["init"]);
        h
    }
    pub fn run(&self, args: &[&str]) -> (i32, Value) {
        let out = Command::new(env!("CARGO_BIN_EXE_ske"))
            .env_remove("SKE_DB")
            .args(args)
            .arg("--db")
            .arg(&self.db)
            .args(["--format", "json"])
            .output()
            .unwrap();
        let code = out.status.code().expect("process terminated by signal");
        assert_ne!(code, 1, "reserved exit code");
        let value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "invalid stdout: {e}: {:?}; stderr={:?}",
                out.stdout, out.stderr
            )
        });
        (code, value)
    }
    pub fn ok(&self, args: &[&str]) -> Value {
        let (code, value) = self.run(args);
        assert_eq!(code, 0, "{value}");
        value
    }
    pub fn fixture(name: &str) -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/cir")
            .join(format!("{name}.cir.pb"))
            .to_str()
            .unwrap()
            .to_owned()
    }
    pub fn root() -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/go-branch-lab")
            .to_str()
            .unwrap()
            .to_owned()
    }
    pub fn ingest(&self, name: &str) -> Value {
        self.ok(&[
            "ingest",
            "cir",
            "--input",
            &Self::fixture(name),
            "--source-root",
            &Self::root(),
        ])
    }
    pub fn conn(&self) -> Connection {
        let conn = Connection::open(&self.db).unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        conn
    }
    pub fn counts(&self) -> BTreeMap<String, i64> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap();
        let counts = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| {
                let name = r.unwrap();
                let count = conn
                    .query_row(&format!("SELECT COUNT(*) FROM \"{name}\""), [], |r| {
                        r.get(0)
                    })
                    .unwrap();
                (name, count)
            })
            .collect();
        counts
    }
}
