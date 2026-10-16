use prost::Message;
use rusqlite::params;
use ske_cir::proto::{record::Content, *};
use ske_common::sha256_hex;
use ske_ingest::{ingest_many, read_record, MAX_MESSAGE_BYTES};
use ske_storage::Storage;
use std::{collections::BTreeMap, fs, path::PathBuf};

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fixture(name: &str) -> Vec<Record> {
    let mut file =
        fs::File::open(project().join(format!("tests/fixtures/cir/{name}.cir.pb"))).unwrap();
    let mut out = Vec::new();
    while let Some(record) = read_record(&mut file, MAX_MESSAGE_BYTES).unwrap() {
        out.push(record);
    }
    out
}
fn write(path: &std::path::Path, records: &[Record]) {
    use std::io::Write;
    let mut out = fs::File::create(path).unwrap();
    for r in records {
        let b = r.encode_to_vec();
        out.write_all(&(b.len() as u32).to_be_bytes()).unwrap();
        out.write_all(&b).unwrap();
    }
}
fn counts(s: &Storage) -> BTreeMap<String, i64> {
    let mut stmt=s.conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| {
            let table = r.unwrap();
            let n = s
                .conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            (table, n)
        })
        .collect()
}

#[test]
fn forward_references_arbitrary_record_order() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("forward.pb");
    let mut records = fixture("adjusted");
    let n = records.len();
    records[2..n - 1].reverse();
    write(&path, &records);
    let s = Storage::open(dir.path().join("db")).unwrap();
    ingest_many(
        &s,
        &[path],
        Some(&project().join("examples/go-branch-lab")),
        None,
    )
    .unwrap();
    assert_eq!(s.db_check().unwrap()["ok"], true);
    assert_eq!(
        s.conn
            .query_row("SELECT COUNT(*) FROM phi_inputs", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn fingerprints_match_files_and_independent_formula() {
    let dir = tempfile::tempdir().unwrap();
    let s = Storage::open(dir.path().join("db")).unwrap();
    let root = project().join("examples/go-branch-lab");
    let fixture_path = project().join("tests/fixtures/cir/decide.cir.pb");
    let records = fixture("decide");
    let mut source_lines = Vec::new();
    let mut config_line = String::new();
    let mut header = None;
    let mut unit_key = String::new();
    for r in records {
        match r.content.unwrap() {
            Content::Header(h) => header = Some(h),
            Content::UnitDef(u) => unit_key = u.unit_key,
            Content::SourceFileDef(f) => source_lines.push(format!(
                "{}\t{}\n",
                f.path,
                sha256_hex(fs::read(root.join(&f.path)).unwrap())
            )),
            _ => (),
        }
    }
    source_lines.sort();
    source_lines.dedup();
    let header = header.unwrap();
    config_line.push_str(&format!(
        "{unit_key}\t{}\t{}\t{}@{}\n",
        header.build_fingerprint,
        sha256_hex(header.semantic_profile.as_bytes()),
        header.adapter_name,
        header.adapter_version
    ));
    for snapshot in 1..=2 {
        let result =
            ingest_many(&s, std::slice::from_ref(&fixture_path), Some(&root), None).unwrap();
        assert_eq!(result["snapshot_id"], snapshot);
        let (source, config): (String, String) = s
            .conn
            .query_row(
                "SELECT source_fingerprint,config_fingerprint FROM snapshots WHERE id=?1",
                [snapshot],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(source, sha256_hex(source_lines.concat()));
        assert_eq!(config, sha256_hex(&config_line));
    }
}

#[test]
fn global_rollback_preserves_existing_snapshot_and_all_tables() {
    let dir = tempfile::tempdir().unwrap();
    let s = Storage::open(dir.path().join("db")).unwrap();
    let path = project().join("tests/fixtures/cir/decide.cir.pb");
    let root = project().join("examples/go-branch-lab");
    ingest_many(&s, std::slice::from_ref(&path), Some(&root), None).unwrap();
    let before = counts(&s);
    assert_eq!(
        ingest_many(
            &s,
            &[path.clone(), path],
            Some(&root),
            Some("new-repository")
        )
        .unwrap_err()
        .code,
        "E_UNIT_DUPLICATE"
    );
    assert_eq!(counts(&s), before);
    // Same connection remains usable after rollback, including its TEMP schema.
    ingest_many(
        &s,
        &[project().join("tests/fixtures/cir/adjusted.cir.pb")],
        Some(&root),
        None,
    )
    .unwrap();
    assert_eq!(s.db_check().unwrap()["ok"], true);
}

#[test]
fn activation_complete_rejects_heuristic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.pb");
    let mut records = fixture("decide");
    for r in &mut records {
        if let Some(Content::SourceOutcomeActivationCase(c)) = &mut r.content {
            c.quality = "HEURISTIC".into();
            break;
        }
    }
    write(&path, &records);
    let s = Storage::open(dir.path().join("db")).unwrap();
    let before = counts(&s);
    assert_eq!(
        ingest_many(&s, &[path], None, None).unwrap_err().code,
        "E_SOURCE_ACTIVATION_INVALID"
    );
    assert_eq!(counts(&s), before);
}

#[test]
fn inventory_is_recomputed_and_never_promoted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("partial.pb");
    let mut records = fixture("decide");
    let last = records.len() - 1;
    records.insert(
        last,
        Record {
            content: Some(Content::Diagnostic(Diagnostic {
                severity: "WARNING".into(),
                code: "FIXTURE_WARNING".into(),
                message: "inventory gap".into(),
                impact: "INVENTORY".into(),
                affected_function_id: "example.Decide".into(),
                ..Default::default()
            })),
        },
    );
    write(&path, &records);
    let s = Storage::open(dir.path().join("db")).unwrap();
    let result = ingest_many(&s, &[path], None, None).unwrap();
    assert_eq!(result["inventory_status"], "PARTIAL");
    let status: String = s
        .conn
        .query_row(
            "SELECT inventory_status FROM functions WHERE is_external=0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "PARTIAL");
    let downgraded: i64 = s
        .conn
        .query_row(
            "SELECT COUNT(*) FROM diagnostics WHERE code='INVENTORY_STATUS_DOWNGRADED'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(downgraded, 1);
    assert_eq!(s.db_check().unwrap()["ok"], true);
}

#[test]
fn repository_identity_does_not_depend_on_source_root() {
    let dir = tempfile::tempdir().unwrap();
    let s = Storage::open(dir.path().join("db")).unwrap();
    let path = project().join("tests/fixtures/cir/invoke.cir.pb");
    ingest_many(&s, std::slice::from_ref(&path), None, Some("example")).unwrap();
    ingest_many(
        &s,
        &[path],
        Some(&project().join("examples/go-branch-lab")),
        Some("example"),
    )
    .unwrap();
    let (n, uri): (i64, String) = s
        .conn
        .query_row("SELECT COUNT(*),MIN(root_uri) FROM repositories", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((n, uri), (1, "ske:example".into()));
    s.conn
        .execute(
            "INSERT INTO repositories(name,root_uri) VALUES(?1,?2)",
            params!["collision", "file://elsewhere"],
        )
        .unwrap();
    assert_eq!(
        ingest_many(
            &s,
            &[project().join("tests/fixtures/cir/invoke.cir.pb")],
            None,
            Some("collision")
        )
        .unwrap_err()
        .code,
        "E_USAGE"
    );
}
