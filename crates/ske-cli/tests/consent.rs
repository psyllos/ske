#![cfg(unix)]
mod common;
use common::Harness;
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
};

fn run(h: &Harness, adapter: &std::path::Path, yes: bool) -> (i32, Value) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ske"));
    command
        .args(["ingest", "go", "--repo"])
        .arg(h.dir.path())
        .arg("--adapter")
        .arg(adapter)
        .arg("--db")
        .arg(&h.db)
        .args(["--format", "json"])
        .stdin(Stdio::null())
        .env_remove("SKE_DB");
    if yes {
        command.arg("--yes-run-adapter");
    }
    let out = command.output().unwrap();
    (
        out.status.code().unwrap(),
        serde_json::from_slice(&out.stdout).unwrap(),
    )
}
fn child(h: &Harness) -> std::path::PathBuf {
    let child = h.dir.path().join("adapter");
    fs::write(
        &child,
        format!(
            "#!/bin/sh\ncat >/dev/null\ntouch '{}'\nexit 9\n",
            h.dir.path().join("ran").display()
        ),
    )
    .unwrap();
    fs::set_permissions(&child, fs::Permissions::from_mode(0o700)).unwrap();
    child
}
#[test]
fn consent_nonterminal_refuses_before_execution_and_yes_persists_identity() {
    let h = Harness::new();
    let adapter = child(&h);
    let (code, value) = run(&h, &adapter, false);
    assert_eq!(code, 2, "{value}");
    assert_eq!(value["error"]["code"], "E_USAGE");
    assert!(!h.dir.path().join("ran").exists());
    let (code, value) = run(&h, &adapter, true);
    assert_eq!(code, 3, "{value}"); // This deliberately failing child is not an extractor.
    assert!(h.dir.path().join("ran").exists());
    let path = h.dir.path().join(".ske/consent.json");
    let entries: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(entries.as_array().unwrap().len(), 1);
    assert_eq!(
        entries[0]["adapter"],
        adapter.canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(
        entries[0]["repository"],
        h.dir.path().canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(entries[0]["version"].as_str().unwrap().len(), 64);
    assert!(entries[0]["date"].as_u64().unwrap() > 0);
    assert_eq!(run(&h, &adapter, false).0, 3, "saved consent is reusable");
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(&adapter)
        .unwrap()
        .write_all(b"# changed\n")
        .unwrap();
    assert_eq!(
        run(&h, &adapter, false).0,
        2,
        "changed executable needs new consent"
    );
}
#[test]
fn consent_rejects_oversize_and_symlink_store() {
    let h = Harness::new();
    let adapter = child(&h);
    let dir = h.dir.path().join(".ske");
    fs::create_dir(&dir).unwrap();
    let path = dir.join("consent.json");
    fs::write(&path, vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert_eq!(run(&h, &adapter, true).0, 2);
    fs::remove_file(&path).unwrap();
    let outside = h.dir.path().join("outside");
    fs::write(&outside, "[]").unwrap();
    std::os::unix::fs::symlink(&outside, &path).unwrap();
    assert_eq!(run(&h, &adapter, true).0, 2);
    assert_eq!(fs::read_to_string(outside).unwrap(), "[]");
    assert!(!h.dir.path().join("ran").exists());
}
