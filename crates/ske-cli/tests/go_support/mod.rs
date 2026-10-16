#![allow(dead_code)]
//! Go conformance support. Deliberately independent of the v0.1 Harness.
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::OnceLock,
};

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

pub fn adapter() -> &'static PathBuf {
    static ADAPTER: OnceLock<PathBuf> = OnceLock::new();
    ADAPTER.get_or_init(|| {
        let path = root().join("target/ske-adapter-go");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Separate cargo integration-test executables can run concurrently. A directory
        // lock serializes their builds, without adding a dependency to ske-cli.
        let lock = root().join("target/go-conformance-build.lock");
        let start = std::time::Instant::now();
        loop {
            match std::fs::create_dir(&lock) {
                Ok(()) => break,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    assert!(start.elapsed().as_secs() < 180, "adapter build lock timed out: {lock:?}; remove a stale lock after checking no build is running");
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => panic!("cannot lock adapter build: {e}"),
            }
        }
        struct Lock(PathBuf);
        impl Drop for Lock {
            fn drop(&mut self) {
                std::fs::remove_dir(&self.0).expect("release adapter build lock");
            }
        }
        let _lock = Lock(lock);
        let out = Command::new("go")
            .current_dir(root().join("adapters/go"))
            .args(["build", "-p=1", "-o"])
            .arg(&path)
            .arg("./cmd/ske-adapter-go")
            .env("GOTOOLCHAIN", "local")
            .env("GOMAXPROCS", "2")
            .output()
            .expect("Go conformance requires go and adapters/go; adapter must not be silently skipped");
        assert!(out.status.success(), "cannot build Go adapter (not the studied project): {}", String::from_utf8_lossy(&out.stderr));
        assert!(path.is_file(), "adapter build did not produce {path:?}");
        path
    })
}

pub fn profile(os: &str, arch: &str) -> Value {
    json!({"profile_version":1,"language":"go","go_version":null,"goos":os,
        "goarch":arch,"tags":[],"include_tests":false,"cgo":false,
        "patterns":["./..."],"go_work":null})
}

pub struct Lab {
    pub dir: tempfile::TempDir,
    pub db: PathBuf,
}
impl Lab {
    pub fn empty() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("knowledge.sqlite");
        let lab = Self { dir, db };
        lab.ok(&["init"]);
        lab
    }
    pub fn new() -> Self {
        let lab = Self::empty();
        lab.ingest("linux", "amd64");
        lab
    }
    pub fn ok(&self, args: &[&str]) -> Value {
        let out = Command::new(env!("CARGO_BIN_EXE_ske"))
            .env_remove("SKE_DB")
            .args(args)
            .arg("--db")
            .arg(&self.db)
            .args(["--format", "json"])
            .env("GOMAXPROCS", "2")
            .output()
            .expect("launch actual ske CLI");
        assert!(
            out.status.success(),
            "ske {args:?}: status={:?}\nstdout={}\nstderr={}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("CLI stdout must be JSON")
    }
    pub fn ingest(&self, os: &str, arch: &str) -> Value {
        let p = self.dir.path().join(format!("{os}-{arch}.json"));
        std::fs::write(&p, profile(os, arch).to_string()).unwrap();
        self.ok(&[
            "ingest",
            "go",
            "--repo",
            root().join("examples/go-branch-lab").to_str().unwrap(),
            "--adapter",
            adapter().to_str().unwrap(),
            "--yes-run-adapter",
            "--profile",
            p.to_str().unwrap(),
        ])
    }
    pub fn conn(&self) -> Connection {
        Connection::open(&self.db).unwrap()
    }
    pub fn branches(&self, name: &str) -> Value {
        self.ok(&["branches", "list", "--function", name])
    }
}

pub fn rows(c: &Connection, sql: &str) -> Vec<Vec<String>> {
    let mut s = c.prepare(sql).unwrap_or_else(|e| panic!("{sql}: {e}"));
    let n = s.column_count();
    s.query_map([], |r| {
        (0..n)
            .map(|i| {
                use rusqlite::types::ValueRef;
                Ok(match r.get_ref(i)? {
                    ValueRef::Null => String::new(),
                    ValueRef::Integer(v) => v.to_string(),
                    ValueRef::Text(v) => String::from_utf8(v.to_vec()).unwrap(),
                    v => panic!("unexpected SQL value {v:?}"),
                })
            })
            .collect::<rusqlite::Result<Vec<_>>>()
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}
pub fn count(c: &Connection, sql: &str) -> i64 {
    c.query_row(sql, [], |r| r.get(0)).unwrap()
}
pub fn function(c: &Connection, name: &str) -> i64 {
    c.query_row("SELECT f.id FROM functions f JOIN symbols s ON s.id=f.symbol_id WHERE s.qualified_name=? AND f.is_external=0 ORDER BY f.id DESC LIMIT 1", [name], |r| r.get(0))
        .unwrap_or_else(|e| panic!("missing real function {name}: {e}"))
}
pub fn golden() -> Vec<Value> {
    let mut paths: Vec<_> = std::fs::read_dir(root().join("tests/golden/go"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 49, "manual golden inventory changed");
    paths
        .into_iter()
        .map(|p| serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap())
        .collect()
}

pub fn span_text(c: &Connection, span: i64) -> String {
    let (path, sl, sc, el, ec): (String, u32, u32, u32, u32) = c.query_row(
        "SELECT f.path,s.start_line,s.start_column,s.end_line,s.end_column FROM source_spans s JOIN source_files f ON f.id=s.source_file_id WHERE s.id=?",
        [span], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
    let bytes = std::fs::read(root().join("examples/go-branch-lab").join(path)).unwrap();
    let mut offsets = vec![0];
    offsets.extend(
        bytes
            .iter()
            .enumerate()
            .filter(|(_, b)| **b == b'\n')
            .map(|(i, _)| i + 1),
    );
    let offset = |line: usize, col: usize| {
        assert!(line > 0 && line <= offsets.len() && col > 0);
        let pos = offsets[line - 1] + col - 1;
        assert!(pos <= bytes.len());
        if line < offsets.len() {
            assert!(pos < offsets[line]);
        }
        pos
    };
    String::from_utf8(
        bytes[offset(sl as usize, sc as usize)..offset(el as usize, ec as usize)].to_vec(),
    )
    .unwrap()
}

pub fn decisions(c: &Connection, fid: i64) -> Vec<Vec<String>> {
    rows(c, &format!("SELECT d.id,d.local_key,d.kind,d.source_span_id,COALESCE(p.local_key,'') FROM source_decisions d JOIN source_spans s ON s.id=d.source_span_id LEFT JOIN source_decisions p ON p.id=d.parent_decision_id WHERE d.function_id={fid} ORDER BY s.start_line,s.start_column,s.end_line DESC,s.end_column DESC,d.kind"))
}
pub fn outcomes(c: &Connection, decision: &str) -> Vec<Vec<String>> {
    rows(c, &format!("SELECT id,local_key,ordinal,label,elided,activation_mapping_complete FROM source_outcomes WHERE decision_id={decision} ORDER BY ordinal"))
}
pub fn assert_golden(c: &Connection) {
    for g in golden() {
        let name = g["function"].as_str().unwrap();
        let ds = decisions(c, function(c, name));
        let expected = g["source_decisions"].as_array().unwrap();
        assert_eq!(ds.len(), expected.len(), "{name}: source count");
        for (index, (d, e)) in ds.iter().zip(expected).enumerate() {
            let sd = format!("sd{index}");
            assert_eq!(e["id"], sd);
            assert!(
                d[1].ends_with(&format!("/{sd}")),
                "{name}: source id {}",
                d[1]
            );
            assert_eq!(d[2], e["kind"].as_str().unwrap(), "{name}/{sd}");
            assert_eq!(
                span_text(c, d[3].parse().unwrap()),
                e["cond_text"].as_str().unwrap(),
                "{name}/{sd}: byte span"
            );
            match e["parent"].as_str() {
                Some(parent) => {
                    assert!(d[4].ends_with(&format!("/{parent}")), "{name}/{sd}: parent")
                }
                None => assert!(d[4].is_empty(), "{name}/{sd}: unexpected parent"),
            }
            let os = outcomes(c, &d[0]);
            let labels = e["outcomes"].as_array().unwrap();
            assert_eq!(os.len(), labels.len(), "{name}/{sd}");
            for (i, (o, label)) in os.iter().zip(labels).enumerate() {
                assert_eq!(o[2], i.to_string(), "{name}/{sd}: ordinal");
                assert!(o[1].ends_with(&format!("/{sd}/o{i}")));
                assert_eq!(o[3], label.as_str().unwrap(), "{name}/{sd}/o{i}");
            }
        }
    }
}

pub fn assert_links(c: &Connection) {
    assert_eq!(count(c, "SELECT COUNT(*) FROM source_outcomes o WHERE o.elided=0 AND NOT EXISTS (SELECT 1 FROM source_ir_links l WHERE l.source_outcome_id=o.id)"), 0, "P-LINK-ALL");
    assert_eq!(
        count(
            c,
            "SELECT COUNT(*) FROM source_ir_links WHERE quality='HEURISTIC'"
        ),
        0,
        "P-NO-HEUR"
    );
}

/// Activation alternatives are compared through the independently named source
/// atoms, never through emitter-generated ids or unordered unions of links.
pub fn assert_activation(c: &Connection) {
    type Cases<'a> = (&'a str, Vec<Vec<&'a str>>, Vec<Vec<&'a str>>);
    let cases: [Cases<'_>; 3] = [
        (
            "OrDecision",
            vec![vec!["1T"], vec!["1F", "2T"]],
            vec![vec!["1F", "2F"]],
        ),
        (
            "AndDecision",
            vec![vec!["1T", "2T"]],
            vec![vec!["1F"], vec!["1T", "2F"]],
        ),
        (
            "MixedDecision",
            vec![vec!["1T"], vec!["1F", "2T", "3T"]],
            vec![vec!["1F", "2F"], vec!["1F", "2T", "3F"]],
        ),
    ];
    for (name, yes, no) in cases {
        let ds = decisions(c, function(c, &format!("example.{name}")));
        let mut atom_ids = BTreeMap::new();
        for (i, d) in ds.iter().enumerate().skip(1) {
            for (j, o) in outcomes(c, &d[0]).iter().enumerate() {
                let links = rows(
                    c,
                    &format!(
                        "SELECT ir_outcome_id FROM source_ir_links WHERE source_outcome_id={}",
                        o[0]
                    ),
                );
                assert_eq!(links.len(), 1, "{name}: atom must have one edge");
                atom_ids.insert(
                    format!("{i}{}", if j == 0 { "T" } else { "F" }),
                    links[0][0].clone(),
                );
            }
        }
        for (o, expected) in outcomes(c, &ds[0][0]).iter().zip([yes, no]) {
            assert_eq!(o[5], "1", "{name}: mapping complete");
            let alternatives = rows(c, &format!("SELECT id,case_ordinal,quality FROM source_outcome_activation_cases WHERE source_outcome_id={} ORDER BY case_ordinal", o[0]));
            assert_eq!(
                alternatives.len(),
                expected.len(),
                "{name}: exact alternative count"
            );
            for (i, (case, terms)) in alternatives.iter().zip(expected).enumerate() {
                assert_eq!(case[1], i.to_string());
                assert_eq!(case[2], "EXACT");
                let actual = rows(c, &format!("SELECT term_ordinal,ir_outcome_id FROM source_outcome_activation_terms WHERE case_id={} ORDER BY term_ordinal", case[0]));
                let wanted: Vec<_> = terms
                    .iter()
                    .enumerate()
                    .map(|(i, t)| vec![i.to_string(), atom_ids[*t].clone()])
                    .collect();
                assert_eq!(actual, wanted, "{name}: ordered DNF terms");
            }
        }
    }
}

pub fn assert_negative_activation(lab: &Lab) {
    // v0.2 has no proof engine. The public navigation contract must preserve
    // incomplete/heuristic provenance without claiming a proof.
    let c = lab.conn();
    let fid = function(&c, "example.OrDecision");
    let ds = decisions(&c, fid);
    let os = outcomes(&c, &ds[0][0]);
    c.execute(
        "UPDATE source_outcomes SET activation_mapping_complete=0 WHERE id=?",
        [&os[0][0]],
    )
    .unwrap();
    c.execute(
        "UPDATE source_ir_links SET quality='HEURISTIC' WHERE source_outcome_id=?",
        [&os[0][0]],
    )
    .unwrap();
    c.execute("DELETE FROM source_outcome_activation_terms WHERE case_id IN (SELECT id FROM source_outcome_activation_cases WHERE source_outcome_id=?)", [&os[0][0]]).unwrap();
    c.execute(
        "DELETE FROM source_outcome_activation_cases WHERE source_outcome_id=?",
        [&os[0][0]],
    )
    .unwrap();
    let before = count(&c, "SELECT COUNT(*) FROM source_ir_links");
    let result = lab.branches("example.OrDecision");
    let outcome = &result["source_decisions"][0]["outcomes"][0];
    assert_eq!(
        outcome["status"], "UNANALYZED",
        "incomplete provenance is not proof"
    );
    assert_eq!(outcome["links"][0]["quality"], "HEURISTIC");
    assert_eq!(
        count(&c, "SELECT COUNT(*) FROM source_ir_links"),
        before,
        "navigation must not invent missing links"
    );
    assert_eq!(count(&c, "SELECT COUNT(*) FROM proof_records"), 0);
}

/// Small protobuf encoder for adapter requests only; tests do not need a new
/// dependency on the ingest implementation they exercise.
fn varint(mut n: usize, out: &mut Vec<u8>) {
    while n >= 128 {
        out.push((n as u8 & 127) | 128);
        n >>= 7;
    }
    out.push(n as u8);
}
fn field(tag: u8, bytes: &[u8], out: &mut Vec<u8>) {
    out.push(tag);
    varint(bytes.len(), out);
    out.extend(bytes);
}
pub fn extract(unit: &str, os: &str, arch: &str) -> Vec<u8> {
    let mut body = Vec::new();
    field(
        10,
        root()
            .join("examples/go-branch-lab")
            .to_str()
            .unwrap()
            .as_bytes(),
        &mut body,
    );
    field(18, profile(os, arch).to_string().as_bytes(), &mut body);
    field(26, unit.as_bytes(), &mut body);
    let mut request = vec![8, 1];
    field(26, &body, &mut request);
    let mut framed = (request.len() as u32).to_be_bytes().to_vec();
    framed.extend(request);
    let mut child = Command::new(adapter())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GOMAXPROCS", "2")
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&framed).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "adapter extract {unit}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.stdout.is_empty(),
        "empty extraction is not a deterministic stream"
    );
    out.stdout
}

/// Decode only protobuf's wire envelope for independent assertions on persisted
/// TypeDef/OperationPayload blobs. Missing scalar fields have proto3 defaults.
pub fn wire_fields(mut bytes: &[u8]) -> BTreeMap<u64, Vec<u8>> {
    fn take(bytes: &mut &[u8]) -> u64 {
        let mut n = 0;
        for shift in (0..70).step_by(7) {
            let (&b, rest) = bytes.split_first().expect("truncated protobuf varint");
            *bytes = rest;
            n |= u64::from(b & 127) << shift;
            if b < 128 {
                return n;
            }
        }
        panic!("invalid protobuf varint")
    }
    let mut fields = BTreeMap::new();
    while !bytes.is_empty() {
        let tag = take(&mut bytes);
        let value = match tag & 7 {
            0 => take(&mut bytes).to_be_bytes().to_vec(),
            2 => {
                let n = take(&mut bytes) as usize;
                let (v, rest) = bytes.split_at(n);
                bytes = rest;
                v.to_vec()
            }
            wire => panic!("unsupported wire type {wire}"),
        };
        fields.insert(tag >> 3, value);
    }
    fields
}
pub fn scalar(fields: &BTreeMap<u64, Vec<u8>>, tag: u64) -> u64 {
    fields
        .get(&tag)
        .map_or(0, |b| u64::from_be_bytes(b.as_slice().try_into().unwrap()))
}

pub fn assert_dominance(c: &Connection) {
    let fid = function(c, "example.AndCall");
    let ds = decisions(c, fid);
    let o = &outcomes(c, &ds[1][0])[0][0];
    let target: i64 = c.query_row("SELECT e.target_block_id FROM source_ir_links l JOIN ir_outcomes o ON o.id=l.ir_outcome_id JOIN control_edges e ON e.id=o.control_edge_id WHERE l.source_outcome_id=?", [o], |r| r.get(0)).unwrap();
    let call: i64 = c.query_row("SELECT i.block_id FROM call_targets t JOIN functions f ON f.id=t.target_function_id JOIN symbols s ON s.id=f.symbol_id JOIN call_sites cs ON cs.id=t.call_site_id JOIN instructions i ON i.id=cs.instruction_id JOIN basic_blocks b ON b.id=i.block_id WHERE b.function_id=? AND s.qualified_name='example.touch' AND i.opcode='CALL'", [fid], |r| r.get(0)).unwrap();
    let entry: i64 = c
        .query_row(
            "SELECT entry_block_id FROM functions WHERE id=?",
            [fid],
            |r| r.get(0),
        )
        .unwrap();
    // Removing the candidate dominator must make the call unreachable.
    let edges = rows(c, &format!("SELECT source_block_id,target_block_id FROM control_edges JOIN basic_blocks b ON b.id=source_block_id WHERE b.function_id={fid}"));
    let mut seen = BTreeSet::new();
    let mut work = vec![entry];
    while let Some(b) = work.pop() {
        if b == target || !seen.insert(b) {
            continue;
        }
        for e in &edges {
            if e[0].parse::<i64>().unwrap() == b {
                work.push(e[1].parse().unwrap());
            }
        }
    }
    assert!(
        !seen.contains(&call),
        "P-SHORT-AndCall: call reachable without a.TRUE target"
    );
    assert_ne!(target, entry, "nontrivial short-circuit dominator");
}
