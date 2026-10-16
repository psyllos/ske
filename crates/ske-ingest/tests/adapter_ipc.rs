//! Adversarial IPC children, not substitutes for extraction/conformance tests.
#![cfg(unix)]
use prost::Message;
use ske_cir::{
    adapter_proto::*,
    proto::{record::Content, Record, SourceFileDef},
};
use ske_ingest::{
    adapter::{extract_unit, list_units, RunLimits},
    ingest_go, GoOptions,
};
use ske_storage::Storage;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

struct ChildFixture {
    dir: tempfile::TempDir,
    executable: PathBuf,
}
impl ChildFixture {
    fn new(bytes: &[u8], behavior: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("response"), bytes).unwrap();
        let executable = dir.path().join("child");
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\ncd '{}' || exit 99\ncat > request\n{behavior}\n",
                dir.path().display()
            ),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        Self { dir, executable }
    }
    fn list(&self, limits: &RunLimits) -> ske_common::Result<Vec<UnitDescriptor>> {
        list_units(&self.executable, self.dir.path(), "{}", limits)
    }
}

// Bounded retry so a momentarily starved child can still land its file write
// before the read; the deadline stays far below any `sleep 30` descendant.
fn read_file_bounded(path: &Path, give_up: Duration) -> Option<String> {
    let deadline = Instant::now() + give_up;
    loop {
        match fs::read_to_string(path) {
            Ok(text) => return Some(text),
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
}
fn frames(bodies: Vec<adapter_response::Body>) -> Vec<u8> {
    let mut bytes = Vec::new();
    for body in bodies {
        let body = AdapterResponse { body: Some(body) }.encode_to_vec();
        bytes.extend((body.len() as u32).to_be_bytes());
        bytes.extend(body);
    }
    bytes
}
fn done() -> adapter_response::Body {
    adapter_response::Body::ListDone(ListUnitsDone {})
}
fn unit() -> adapter_response::Body {
    adapter_response::Body::Unit(UnitDescriptor {
        unit_key: "go:example@linux/amd64".into(),
        language: "go".into(),
        source_paths: vec!["a.go".into()],
    })
}
fn limits() -> RunLimits {
    RunLimits {
        timeout_ms: 5000,
        ..RunLimits::default()
    }
}
#[test]
fn adapter_list_units_request_eof_and_clean_environment() {
    let child = ChildFixture::new(&frames(vec![unit(), done()]),
        "test \"$GOENV\" = off && test \"$GOTOOLCHAIN\" = local && test \"$GOPROXY\" = off && test \"$GOFLAGS\" = -mod=readonly && test \"$GOWORK\" = off || exit 98\ncat response");
    assert_eq!(child.list(&limits()).unwrap().len(), 1);
    let request = fs::read(child.dir.path().join("request")).unwrap();
    let length = u32::from_be_bytes(request[..4].try_into().unwrap()) as usize;
    assert_eq!(request.len(), length + 4, "exactly one request then EOF");
    let request = AdapterRequest::decode(&request[4..]).unwrap();
    assert_eq!(request.protocol_major, 1);
    let adapter_request::Body::ListUnits(request) = request.body.unwrap() else {
        panic!()
    };
    assert!(Path::new(&request.project_root).is_absolute());
    assert_eq!(request.build_profile_json, "{}");
    assert_eq!(request.limits.unwrap().timeout_ms, 5000);
}
#[test]
fn adapter_list_units_rejects_missing_done_wrong_body_duplicate_done_and_error() {
    for (bodies, expected) in [
        (vec![unit()], "E_RECORD_ORDER"),
        (vec![done(), done()], "E_RECORD_ORDER"),
        (vec![done(), unit()], "E_RECORD_ORDER"),
        (
            vec![adapter_response::Body::Record(Record::default())],
            "E_RECORD_ORDER",
        ),
        (
            vec![adapter_response::Body::Error(AdapterError {
                code: "TOOLCHAIN_MISSING".into(),
                message: "no go".into(),
            })],
            "E_ADAPTER_LIMIT",
        ),
    ] {
        let child = ChildFixture::new(&frames(bodies), "cat response");
        assert_eq!(child.list(&limits()).unwrap_err().code, expected);
    }
}
#[test]
fn adapter_limits_oversize_count_truncation_invalid_and_nonzero_exit() {
    for (bytes, behavior, limit, expected) in [
        (
            1001_u32.to_be_bytes().to_vec(),
            "cat response",
            RunLimits {
                max_message_bytes: 1000,
                ..limits()
            },
            "E_ADAPTER_LIMIT",
        ),
        (
            frames(vec![unit(), done()]),
            "cat response",
            RunLimits {
                max_messages: 1,
                ..limits()
            },
            "E_ADAPTER_LIMIT",
        ),
        (vec![0, 0], "cat response", limits(), "E_PROTO_DECODE"),
        (
            vec![0, 0, 0, 2, 255],
            "cat response",
            limits(),
            "E_PROTO_DECODE",
        ),
        (vec![0, 0, 0, 0], "cat response", limits(), "E_PROTO_DECODE"),
        (
            frames(vec![done()]),
            "cat response; exit 9",
            limits(),
            "E_ADAPTER_LIMIT",
        ),
    ] {
        let child = ChildFixture::new(&bytes, behavior);
        assert_eq!(child.list(&limit).unwrap_err().code, expected);
    }
}
#[test]
fn adapter_limits_failed_child_exit_takes_precedence_over_incomplete_response() {
    for bytes in [vec![], vec![0, 0], vec![0, 0, 0, 2, 255]] {
        // Closing stdout before exiting makes EOF observable before try_wait
        // can report failure. Linux allocation failures have the same ordering.
        let child = ChildFixture::new(&bytes, "cat response\nexec 1>&-\nsleep 0.2\nexit 9");
        let error = child.list(&limits()).unwrap_err();
        assert_eq!(error.code, "E_ADAPTER_LIMIT");
        assert!(error.message.contains("adapter exited"), "{error}");
    }
}
#[test]
fn adapter_limits_invalid_frame_does_not_wait_for_child_exit() {
    let child = ChildFixture::new(&[0, 0, 0, 0], "cat response\nsleep 30");
    let start = Instant::now();
    assert_eq!(child.list(&limits()).unwrap_err().code, "E_PROTO_DECODE");
    // Prompt return: far below the child's 30 s sleep, with room for process
    // startup latency on a loaded machine; waiting for the child would take
    // about 30 s and still be caught.
    assert!(start.elapsed() < Duration::from_secs(10));
}
#[test]
fn adapter_limits_eof_still_bounds_wait_for_child_exit() {
    let child = ChildFixture::new(&[], "exec 1>&-\nsleep 30");
    let start = Instant::now();
    let error = child
        .list(&RunLimits {
            timeout_ms: 300,
            ..limits()
        })
        .unwrap_err();
    assert_eq!(error.code, "E_ADAPTER_LIMIT");
    assert!(error.message.contains("monitoring limit"), "{error}");
    // Bound, far below the child's 30 s sleep, with room for process startup
    // latency on a loaded machine.
    assert!(start.elapsed() < Duration::from_secs(10));
}
#[test]
fn adapter_limits_stderr_flood_drained_without_polluting_frames() {
    let child = ChildFixture::new(
        &frames(vec![done()]),
        "dd if=/dev/zero bs=65536 count=64 >&2 2>/dev/null\ncat response",
    );
    assert!(child.list(&limits()).unwrap().is_empty());
}
#[test]
fn adapter_limits_memory_stops_allocating_child_before_timeout() {
    // python3 is a test driver only; it never examines or executes studied sources.
    let child = ChildFixture::new(
        &[],
        "exec python3 -c 'import time; open(\"memory-started\", \"w\").write(\"started\"); x=bytearray(128*1024*1024); time.sleep(30)'",
    );
    let start = Instant::now();
    let error = child
        .list(&RunLimits {
            max_memory_mb: 64,
            timeout_ms: 8000,
            ..limits()
        })
        .unwrap_err();
    assert_eq!(error.code, "E_ADAPTER_LIMIT");
    assert!(
        child.dir.path().join("memory-started").exists(),
        "Python driver must execute; a missing interpreter is not a memory-limit PASS"
    );
    assert!(
        start.elapsed() < Duration::from_secs(6),
        "memory limit must trigger before timeout"
    );
}

#[test]
fn adapter_limits_nonzero_exit_without_done_is_process_failure() {
    // A hard address-space limit may fail an allocation and make the child
    // exit before the RSS watchdog samples it. Missing Done must not mask
    // that non-zero process exit as a mere record-order error.
    let failed = ChildFixture::new(&[], "exit 42");
    assert_eq!(failed.list(&limits()).unwrap_err().code, "E_ADAPTER_LIMIT");
    // A successful but incomplete protocol remains a protocol error.
    let incomplete = ChildFixture::new(&[], "exit 0");
    assert_eq!(
        incomplete.list(&limits()).unwrap_err().code,
        "E_RECORD_ORDER"
    );
}

fn decide_records() -> Vec<Record> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cir/decide.cir.pb");
    let mut input = fs::File::open(path).unwrap();
    let mut records = Vec::new();
    while let Some(record) =
        ske_ingest::read_record(&mut input, ske_ingest::MAX_MESSAGE_BYTES).unwrap()
    {
        records.push(record);
    }
    records
}
fn replay(records: Vec<Record>, late_failure: bool) -> ChildFixture {
    let key = records
        .iter()
        .find_map(|r| match &r.content {
            Some(Content::UnitDef(u)) => Some(u.unit_key.clone()),
            _ => None,
        })
        .unwrap();
    let list = frames(vec![
        adapter_response::Body::Unit(UnitDescriptor {
            unit_key: key,
            language: "go".into(),
            source_paths: vec![],
        }),
        done(),
    ]);
    let child = ChildFixture::new(
        &list,
        "if test -f listed; then cat extracted; else touch listed; cat response; fi",
    );
    let lab = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/go-branch-lab");
    for record in &records {
        if let Some(Content::SourceFileDef(file)) = &record.content {
            let path = child.dir.path().join(&file.path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::copy(lab.join(&file.path), path).unwrap();
        }
    }
    let mut bodies: Vec<_> = records
        .into_iter()
        .map(adapter_response::Body::Record)
        .collect();
    if late_failure {
        bodies.push(adapter_response::Body::Error(AdapterError {
            code: "LATE_FAILURE".into(),
            message: "after EndUnit".into(),
        }));
    }
    fs::write(child.dir.path().join("extracted"), frames(bodies)).unwrap();
    child
}
#[test]
fn adapter_limits_late_failure_after_endunit_rolls_back_valid_structure() {
    let child = replay(decide_records(), true);
    let storage = Storage::open(child.dir.path().join("db")).unwrap();
    let value = ingest_go(
        &storage,
        &GoOptions {
            repo: child.dir.path(),
            adapter: &child.executable,
            profile_json: "{}",
            limits: &limits(),
            repository: None,
            redact: false,
        },
    )
    .unwrap();
    assert_eq!(value["status"], "ABORTED");
    assert_eq!(
        storage
            .conn
            .query_row("SELECT COUNT(*) FROM functions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    let message: String = storage
        .conn
        .query_row("SELECT message FROM diagnostics", [], |r| r.get(0))
        .unwrap();
    assert!(message.contains("LATE_FAILURE"), "{message}");
    assert_eq!(storage.db_check().unwrap()["ok"], true);
}
#[test]
fn adapter_redaction_masks_successful_diagnostic_without_changing_fingerprint() {
    let mut records = decide_records();
    records.insert(
        2,
        Record {
            content: Some(Content::Diagnostic(ske_cir::proto::Diagnostic {
                severity: "INFO".into(),
                code: "SOURCE_NOT_VERIFIED".into(),
                message: "private /absolute/project/file.go".into(),
                impact: "OTHER".into(),
                ..Default::default()
            })),
        },
    );
    let child = replay(records, false);
    let mut fingerprints = Vec::new();
    for redact in [false, true] {
        let _ = fs::remove_file(child.dir.path().join("listed"));
        let storage = Storage::open(child.dir.path().join(format!("db-{redact}"))).unwrap();
        let value = ingest_go(
            &storage,
            &GoOptions {
                repo: child.dir.path(),
                adapter: &child.executable,
                profile_json: "{}",
                limits: &limits(),
                repository: None,
                redact,
            },
        )
        .unwrap();
        assert_eq!(value["status"], "CLOSED");
        let message: String = storage
            .conn
            .query_row(
                "SELECT message FROM diagnostics WHERE code='SOURCE_NOT_VERIFIED'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(message.contains("/absolute/project"), !redact);
        let fingerprint: (String, String) = storage
            .conn
            .query_row(
                "SELECT source_fingerprint,config_fingerprint FROM snapshots",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        fingerprints.push(fingerprint);
        assert_eq!(storage.db_check().unwrap()["ok"], true);
    }
    assert_eq!(fingerprints[0], fingerprints[1]);
}
#[test]
fn adapter_limits_invalid_options_and_profile_fail_before_snapshot_or_child() {
    let child = ChildFixture::new(&[], "touch ran");
    let storage = Storage::open(child.dir.path().join("db")).unwrap();
    for (profile, limit) in [
        (
            "{}",
            RunLimits {
                max_memory_mb: u64::MAX,
                ..limits()
            },
        ),
        (r#"{"unknown":1}"#, limits()),
    ] {
        let error = ingest_go(
            &storage,
            &GoOptions {
                repo: child.dir.path(),
                adapter: &child.executable,
                profile_json: profile,
                limits: &limit,
                repository: None,
                redact: false,
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "E_USAGE");
        assert_eq!(
            storage
                .conn
                .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(!child.dir.path().join("ran").exists());
    }
}
#[test]
fn source_hash_go_mismatch_rolls_back_structure_but_retains_header_identity() {
    let records = decide_records();
    let key = records
        .iter()
        .find_map(|r| match &r.content {
            Some(Content::UnitDef(u)) => Some(u.unit_key.clone()),
            _ => None,
        })
        .unwrap();
    let header = records
        .iter()
        .find_map(|r| match &r.content {
            Some(Content::Header(h)) => Some(h.clone()),
            _ => None,
        })
        .unwrap();
    let list = frames(vec![
        adapter_response::Body::Unit(UnitDescriptor {
            unit_key: key,
            language: "go".into(),
            source_paths: vec![],
        }),
        done(),
    ]);
    let child = ChildFixture::new(
        &list,
        "if test -f listed; then cat extracted; else touch listed; cat response; fi",
    );
    fs::write(
        child.dir.path().join("extracted"),
        frames(
            records
                .into_iter()
                .map(adapter_response::Body::Record)
                .collect(),
        ),
    )
    .unwrap();
    // Missing/changed trusted fixture sources: the core must verify, not trust the child's digest.
    let storage = Storage::open(child.dir.path().join("db")).unwrap();
    let value = ingest_go(
        &storage,
        &GoOptions {
            repo: child.dir.path(),
            adapter: &child.executable,
            profile_json: "{}",
            limits: &limits(),
            repository: None,
            redact: false,
        },
    )
    .unwrap();
    assert_eq!(value["status"], "ABORTED");
    let (name, version, status): (String, String, String) = storage
        .conn
        .query_row(
            "SELECT adapter_name,adapter_version,inventory_status FROM compilation_units",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (name, version, status),
        (header.adapter_name, header.adapter_version, "FAILED".into())
    );
    let message: String = storage
        .conn
        .query_row("SELECT message FROM diagnostics", [], |r| r.get(0))
        .unwrap();
    assert!(message.contains("E_SOURCE_HASH_MISMATCH"), "{message}");
    assert_eq!(
        storage
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(storage.db_check().unwrap()["ok"], true);
}
#[test]
fn source_hash_symlink_escape_is_rejected_even_when_digest_matches() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let records = decide_records();
    let source = records
        .iter()
        .find_map(|r| match &r.content {
            Some(Content::SourceFileDef(f)) => Some(f),
            _ => None,
        })
        .unwrap();
    let lab = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/go-branch-lab");
    let target = outside.path().join("source.go");
    fs::copy(lab.join(&source.path), &target).unwrap();
    let link = dir.path().join(&source.path);
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(target, link).unwrap();
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cir/decide.cir.pb");
    let storage = Storage::open(dir.path().join("db")).unwrap();
    assert_eq!(
        ske_ingest::ingest_many(&storage, &[path], Some(dir.path()), None)
            .unwrap_err()
            .code,
        "E_PATH_INVALID"
    );
}
#[test]
fn adapter_limits_timeout_kills_descendants_holding_pipes() {
    // The descendant records its own PID before sleeping, so its identity never
    // races the group kill, and the read below tolerates a short write delay.
    let child = ChildFixture::new(&[], "sh -c 'echo $$ > descendant; exec sleep 30' &\nwait");
    let start = Instant::now();
    assert_eq!(
        child
            .list(&RunLimits {
                timeout_ms: 2000,
                ..limits()
            })
            .unwrap_err()
            .code,
        "E_ADAPTER_LIMIT"
    );
    assert!(start.elapsed() < Duration::from_secs(5));
    let pid = read_file_bounded(&child.dir.path().join("descendant"), Duration::from_secs(5))
        .expect("descendant must record its PID before the group kill");
    // Zombies may remain until init reaps them, but cannot hold pipes or execute.
    let output = std::process::Command::new("/bin/ps")
        .args(["-o", "stat=", "-p", pid.trim()])
        .output()
        .unwrap();
    let state = String::from_utf8_lossy(&output.stdout);
    assert!(
        state.trim().is_empty() || state.trim().starts_with('Z'),
        "{state}"
    );
}
#[test]
fn adapter_limits_timeout_also_bounds_wait_and_blocked_stdin_write() {
    let child = ChildFixture::new(
        &frames(vec![done()]),
        "cat response\nexec 1>&- 2>&-\nsleep 30",
    );
    let limit = RunLimits {
        timeout_ms: 1000,
        ..limits()
    };
    let start = Instant::now();
    assert_eq!(child.list(&limit).unwrap_err().code, "E_ADAPTER_LIMIT");
    assert!(start.elapsed() < Duration::from_secs(5));

    // Do not read stdin at all; the request is deliberately larger than a pipe.
    fs::write(&child.executable, "#!/bin/sh\nsleep 30\n").unwrap();
    let start = Instant::now();
    let error = list_units(
        &child.executable,
        child.dir.path(),
        &" ".repeat(1024 * 1024),
        &limit,
    )
    .unwrap_err();
    assert_eq!(error.code, "E_ADAPTER_LIMIT");
    assert!(start.elapsed() < Duration::from_secs(5));
}
#[test]
fn adapter_limits_failed_unit_persisted_and_no_structure_leaked() {
    let child = ChildFixture::new(&1001_u32.to_be_bytes(), "cat response");
    let storage = Storage::open(child.dir.path().join("db")).unwrap();
    let value = ingest_go(
        &storage,
        &GoOptions {
            repo: child.dir.path(),
            adapter: &child.executable,
            profile_json: "{}",
            limits: &RunLimits {
                max_message_bytes: 1000,
                ..limits()
            },
            repository: None,
            redact: false,
        },
    )
    .unwrap();
    assert_eq!(value["status"], "ABORTED");
    assert_eq!(value["units"][0]["inventory_status"], "FAILED");
    let diagnostic: String = storage
        .conn
        .query_row("SELECT message FROM diagnostics", [], |r| r.get(0))
        .unwrap();
    assert!(diagnostic.contains("E_ADAPTER_LIMIT"));
    assert_eq!(
        storage
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(storage.db_check().unwrap()["ok"], true);
}
#[test]
fn adapter_extract_rejects_unconsumed_and_late_error() {
    for bodies in [
        vec![adapter_response::Body::Record(Record::default())],
        vec![adapter_response::Body::Error(AdapterError {
            code: "LATE".into(),
            message: "failed".into(),
        })],
    ] {
        let child = ChildFixture::new(&frames(bodies), "cat response");
        assert!(extract_unit(
            &child.executable,
            child.dir.path(),
            "{}",
            "key",
            &limits(),
            |_| Ok(())
        )
        .is_err());
    }
}
#[test]
fn source_hash_changed_between_fixture_extraction_and_import() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.go"), "modified after extraction").unwrap();
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cir/decide.cir.pb");
    let mut input = fs::File::open(fixture).unwrap();
    let mut bytes = Vec::new();
    while let Some(mut record) =
        ske_ingest::read_record(&mut input, ske_ingest::MAX_MESSAGE_BYTES).unwrap()
    {
        if matches!(record.content, Some(Content::SourceFileDef(_))) {
            record.content = Some(Content::SourceFileDef(SourceFileDef {
                path: "a.go".into(),
                language: "go".into(),
                content_sha256: ske_common::sha256_hex(b"original extracted content"),
            }));
        }
        let body = record.encode_to_vec();
        bytes.extend((body.len() as u32).to_be_bytes());
        bytes.extend(body);
    }
    let path = dir.path().join("changed.pb");
    fs::write(&path, bytes).unwrap();
    let storage = Storage::open(dir.path().join("db")).unwrap();
    assert_eq!(
        ske_ingest::ingest_many(&storage, &[path], Some(dir.path()), None)
            .unwrap_err()
            .code,
        "E_SOURCE_HASH_MISMATCH"
    );
    assert_eq!(
        storage
            .conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}
