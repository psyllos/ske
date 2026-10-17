//! Independent v0.6 acceptance oracles. Missing tools/adapters are failures.
//! Only trusted, repository-owned fixtures are compiled; no fixture is executed.
mod go_support;

use go_support::{count, decisions, outcomes, root, rows, scalar, wire_fields, Lab};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::OnceLock,
};

fn checked(command: &mut Command) -> Output {
    let out = command
        .output()
        .unwrap_or_else(|e| panic!("{command:?}: {e}"));
    assert!(
        out.status.success(),
        "{command:?}: {:?}\n{}\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn adapter(lang: &str) -> PathBuf {
    static RUST: OnceLock<PathBuf> = OnceLock::new();
    static JAVA: OnceLock<PathBuf> = OnceLock::new();
    match lang {
        "go" => go_support::adapter().clone(),
        "rust" => RUST
            .get_or_init(|| {
                checked(
                    Command::new("cargo")
                        .arg("build")
                        .current_dir(root().join("adapters/rust")),
                );
                let p = root().join("adapters/rust/target/debug/ske-adapter-rust");
                assert!(p.is_file(), "missing Rust adapter executable: {p:?}");
                p
            })
            .clone(),
        "java" => JAVA
            .get_or_init(|| {
                checked(
                    Command::new("mvn")
                        .args(["-B", "package"])
                        .current_dir(root().join("adapters/java")),
                );
                let p = root().join("adapters/java/ske-adapter-java");
                assert!(p.is_file(), "missing Java adapter launcher: {p:?}");
                p
            })
            .clone(),
        _ => panic!("unknown language {lang}"),
    }
}

fn profile(lang: &str) -> Value {
    match lang {
        "rust" => json!({"profile_version":1,"language":"rust","target":null,
            "features":[],"cfg":[],"entry":"src/lib.rs"}),
        "java" => json!({"profile_version":1,"language":"java","class_path":["target/classes"],
            "source_roots":["src/main/java"],"target_release":17}),
        "go" => go_support::profile("linux", "amd64"),
        _ => panic!("unknown language"),
    }
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from)
        .unwrap_or_else(|e| panic!("required trusted fixture directory {from:?}: {e}"))
    {
        let e = e.unwrap();
        let name = e.file_name();
        if ["target", ".ske", ".git"].iter().any(|n| name == *n) {
            continue;
        }
        let dest = to.join(name);
        if e.file_type().unwrap().is_dir() {
            copy_tree(&e.path(), &dest);
        } else {
            assert!(
                e.file_type().unwrap().is_file(),
                "fixture must not contain symlinks"
            );
            std::fs::copy(e.path(), dest).unwrap();
        }
    }
}

struct MultiLab {
    lab: Lab,
    repo: PathBuf,
    lang: &'static str,
    adapter: PathBuf,
    profile: PathBuf,
}

impl MultiLab {
    fn prepare(lang: &'static str) -> Self {
        let lab = Lab::empty();
        let repo = lab.dir.path().join(format!("{lang}-branch-lab"));
        copy_tree(&root().join(format!("examples/{lang}-branch-lab")), &repo);
        if lang == "java" {
            // Explicit allow-list: never execute a build script from the studied repository.
            let classes = repo.join("target/classes");
            std::fs::create_dir_all(&classes).unwrap();
            let mut javac = Command::new("javac");
            javac.args(["--release", "17", "-g", "-d"]).arg(classes);
            for name in ["Decide", "ReflectionUnknown", "MultiCatchFinally"] {
                javac.arg(repo.join(format!("src/main/java/example/{name}.java")));
            }
            checked(&mut javac);
        }
        let profile_path = lab.dir.path().join("profile.json");
        std::fs::write(&profile_path, profile(lang).to_string()).unwrap();
        Self {
            lab,
            repo,
            lang,
            adapter: adapter(lang),
            profile: profile_path,
        }
    }

    fn new(lang: &'static str) -> Self {
        let lab = Self::prepare(lang);
        let out = lab.ingest(true, None);
        if !out.status.success() {
            let mut persisted = String::new();
            if let Ok(conn) = Connection::open(&lab.lab.db) {
                if let Ok(mut rows) =
                    conn.prepare("SELECT severity, code, message FROM diagnostics ORDER BY id")
                {
                    if let Ok(rows) = rows.query_map([], |r| {
                        Ok(format!(
                            "{} {} {}",
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?
                        ))
                    }) {
                        persisted = rows.filter_map(|r| r.ok()).collect::<Vec<_>>().join("\n");
                    }
                }
            }
            panic!(
                "[{lang}] ingest failed\nstdout:\n{}\nstderr:\n{}\npersisted:\n{persisted}\n",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let _: Value = serde_json::from_slice(&out.stdout).expect("pure CLI JSON");
        lab.lab.ok(&["db", "check"]);
        lab
    }

    fn ingest(&self, consent: bool, env: Option<(&str, &str)>) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ske"));
        command
            .env_remove("SKE_DB")
            .stdin(Stdio::null())
            .args(["ingest", self.lang, "--repo"])
            .arg(&self.repo)
            .arg("--adapter")
            .arg(&self.adapter)
            .arg("--profile")
            .arg(&self.profile)
            .arg("--db")
            .arg(&self.lab.db)
            .args(["--format", "json"]);
        if consent {
            command.arg("--yes-run-adapter");
        }
        if let Some((key, value)) = env {
            command.env(key, value);
        }
        command.output().unwrap()
    }

    fn conn(&self) -> Connection {
        self.lab.conn()
    }

    // Resolve the real symbol through its source-language suffix, without assuming
    // a compiler-specific crate prefix or JVM method descriptor.
    fn name(&self, suffix: &str) -> String {
        let names = rows(&self.conn(), "SELECT s.qualified_name FROM functions f JOIN symbols s ON s.id=f.symbol_id WHERE f.is_external=0");
        let matches: Vec<_> = names
            .iter()
            .filter(|r| {
                r[0] == suffix
                    || r[0].ends_with(&format!("::{suffix}"))
                    || r[0].ends_with(&format!(".{suffix}"))
            })
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "unique function {suffix}; available={names:?}"
        );
        matches[0][0].clone()
    }
}

fn span_text(lab: &MultiLab, span: &str) -> String {
    let c = lab.conn();
    let (path, sl, sc, el, ec): (String, u32, u32, u32, u32) = c.query_row(
        "SELECT f.path,s.start_line,s.start_column,s.end_line,s.end_column FROM source_spans s JOIN source_files f ON f.id=s.source_file_id WHERE s.id=?",
        [span], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
    let bytes = std::fs::read(lab.repo.join(path)).unwrap();
    let mut starts = vec![0];
    starts.extend(
        bytes
            .iter()
            .enumerate()
            .filter(|(_, b)| **b == b'\n')
            .map(|(i, _)| i + 1),
    );
    let offset = |line: usize, col: usize| {
        assert!(line > 0 && line <= starts.len() && col > 0);
        let pos = starts[line - 1] + col - 1;
        assert!(pos <= bytes.len());
        if line < starts.len() {
            assert!(pos < starts[line]);
        }
        pos
    };
    String::from_utf8(
        bytes[offset(sl as usize, sc as usize)..offset(el as usize, ec as usize)].to_vec(),
    )
    .unwrap()
}

type Decision<'a> = (&'a str, &'a str, &'a [&'a str]);
fn golden(lab: &MultiLab, suffix: &str, expected: &[Decision<'_>]) {
    let c = lab.conn();
    let name = lab.name(suffix);
    let actual = decisions(&c, go_support::function(&c, &name));
    assert_eq!(
        actual.len(),
        expected.len(),
        "{name}: exact decision inventory"
    );
    for (d, (kind, text, labels)) in actual.iter().zip(expected) {
        assert_eq!(&d[2], kind, "{name}: kind");
        assert_eq!(
            span_text(lab, &d[3]),
            *text,
            "{name}: exact UTF-8 byte span"
        );
        let os = outcomes(&c, &d[0]);
        assert_eq!(
            os.iter().map(|o| o[3].as_str()).collect::<Vec<_>>(),
            *labels
        );
        for (ordinal, o) in os.iter().enumerate() {
            assert_eq!(o[2], ordinal.to_string());
        }
    }
    let navigation = lab.lab.branches(&name);
    assert_eq!(
        navigation["source_decisions"].as_array().unwrap().len(),
        expected.len()
    );
}

#[test]
fn rust_golden_decisions() {
    let lab = MultiLab::new("rust");
    golden(
        &lab,
        "decide",
        &[
            ("IF", "x < 0", &["TRUE", "FALSE"]),
            ("IF", "x < -10", &["TRUE", "FALSE"]),
        ],
    );
    golden(
        &lab,
        "kind",
        &[("MATCH", "n", &["CASE:0", "CASE:1", "DEFAULT"])],
    );
    golden(
        &lab,
        "parse_len",
        &[("ERROR_PROPAGATION", "s.parse()?", &["SUCCESS", "ERROR"])],
    );
}

#[test]
fn java_golden_decisions() {
    let lab = MultiLab::new("java");
    golden(
        &lab,
        "Decide.decide",
        &[
            ("IF", "x < 0", &["TRUE", "FALSE"]),
            ("IF", "x < -10", &["TRUE", "FALSE"]),
        ],
    );
    golden(
        &lab,
        "Decide.day",
        &[("SWITCH", "n", &["CASE:0", "CASE:1", "DEFAULT"])],
    );
    golden(
        &lab,
        "Decide.guarded",
        &[("TRY_CATCH", "try", &["SUCCESS", "EXCEPTION"])],
    );
}

fn parameter_type(lab: &MultiLab, suffix: &str, bits: u64, signed: u64) {
    let c = lab.conn();
    let fid = go_support::function(&c, &lab.name(suffix));
    let payload: Vec<u8> = c.query_row("SELECT t.payload FROM values_ir v JOIN cir_types t ON t.id=v.type_id WHERE v.function_id=? AND v.param_index=0", [fid], |r| r.get(0)).unwrap();
    let fields = wire_fields(&payload);
    assert_eq!(fields.get(&2).unwrap(), b"BITVECTOR");
    assert_eq!(scalar(&fields, 5), bits);
    assert_eq!(scalar(&fields, 6), signed);
}

#[test]
fn lang_types() {
    let rust = MultiLab::new("rust");
    parameter_type(&rust, "decide", 32, 1);
    parameter_type(&rust, "kind", 8, 0);
    parameter_type(&MultiLab::new("java"), "Decide.decide", 32, 1);
}

/// MIR UNDEF (uninitialized local/temporary) sentinel: routes through COPY/PHI,
/// never reaches a consuming operation without failing loudly.
const UNDEF: i128 = i128::MIN;

/// Turn an operand into a concrete integer, refusing an uninitialized value.
fn consume(value: i128, source: &i64) -> i128 {
    assert!(
        value != UNDEF,
        "path consumes uninitialized MIR value {source}"
    );
    value
}

/// A deliberately small, bounded interpreter of the *persisted* scalar CIR,
/// independent of the symbolic engine. Unknown operations fail, never disappear.
/// Values are signed i128: Decide only needs signed <=64-bit integers and BOOL.
fn execute_decide(c: &Connection, fid: i64, x: i64) -> i128 {
    let mut values = BTreeMap::<i64, i128>::new();
    let mut stmt = c.prepare("SELECT v.id,v.value_role,v.literal,t.payload FROM values_ir v JOIN cir_types t ON t.id=v.type_id WHERE v.function_id=? OR (v.function_id IS NULL AND v.value_role='CONST')").unwrap();
    let vs = stmt
        .query_map([fid], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<Vec<u8>>>(2)?,
                r.get::<_, Vec<u8>>(3)?,
            ))
        })
        .unwrap();
    for row in vs {
        let (id, role, literal, ty) = row.unwrap();
        if role == "PARAM" {
            values.insert(id, i128::from(x));
        }
        if let Some(literal) = literal {
            let f = wire_fields(&literal);
            if let Some(bytes) = f.get(&2) {
                let t = wire_fields(&ty);
                let width = scalar(&t, 5);
                assert!((1..=64).contains(&width));
                let mut n = bytes.iter().fold(0i128, |n, b| (n << 8) | i128::from(*b));
                if scalar(&t, 6) == 1 && n & (1i128 << (width - 1)) != 0 {
                    n -= 1i128 << width;
                }
                values.insert(id, n);
            } else if f.contains_key(&1) {
                values.insert(id, i128::from(scalar(&f, 1)));
            }
        }
    }
    let mut block: i64 = c
        .query_row(
            "SELECT entry_block_id FROM functions WHERE id=?",
            [fid],
            |r| r.get(0),
        )
        .unwrap();
    let mut previous = None;
    for _ in 0..100 {
        let instructions = rows(c, &format!("SELECT id,opcode,COALESCE(result_value_id,0) FROM instructions WHERE block_id={block} ORDER BY ordinal"));
        let mut next = None;
        for i in instructions {
            let id: i64 = i[0].parse().unwrap();
            let operands = rows(c, &format!("SELECT value_id FROM instruction_operands WHERE instruction_id={id} ORDER BY operand_index"));
            let (operand_ids, args): (Vec<i64>, Vec<i128>) = operands
                .iter()
                .map(|r| {
                    let operand_id: i64 = r[0].parse().unwrap();
                    let value = *values
                        .get(&operand_id)
                        .unwrap_or_else(|| panic!("operand {operand_id} never defined"));
                    (operand_id, value)
                })
                .unzip();
            let value = match i[1].as_str() {
                "COPY" => Some(args[0]),
                "PHI" => {
                    let pred: i64 = previous.expect("PHI needs predecessor");
                    let input: i64 = c.query_row("SELECT value_id FROM phi_inputs WHERE instruction_id=? AND predecessor_block_id=?", [id,pred], |r| r.get(0)).unwrap();
                    // PHI forwards whatever its predecessor routed, including
                    // UNDEF storage markers; only a consuming op may not see it.
                    Some(values[&input])
                }
                "LT" => Some(i128::from(
                    consume(args[0], &operand_ids[0]) < consume(args[1], &operand_ids[1]),
                )),
                "LE" => Some(i128::from(
                    consume(args[0], &operand_ids[0]) <= consume(args[1], &operand_ids[1]),
                )),
                "GT" => Some(i128::from(
                    consume(args[0], &operand_ids[0]) > consume(args[1], &operand_ids[1]),
                )),
                "GE" => Some(i128::from(
                    consume(args[0], &operand_ids[0]) >= consume(args[1], &operand_ids[1]),
                )),
                "EQ" => Some(i128::from(
                    consume(args[0], &operand_ids[0]) == consume(args[1], &operand_ids[1]),
                )),
                "NE" => Some(i128::from(
                    consume(args[0], &operand_ids[0]) != consume(args[1], &operand_ids[1]),
                )),
                "LOGICAL_NOT" => Some(i128::from(consume(args[0], &operand_ids[0]) == 0)),
                "NEG" => Some(-consume(args[0], &operand_ids[0])),
                // MIR UNDEF marker: routing it is fine, but a consuming op on it
                // fails loudly through consume() instead of guessing a number.
                "UNDEF" => {
                    let result: i64 = i[2].parse().unwrap();
                    if result != 0 {
                        values.insert(result, UNDEF);
                    }
                    None
                }
                "RETURN" => {
                    assert_eq!(args.len(), 1);
                    return consume(args[0], &operand_ids[0]);
                }
                "JUMP" | "BRANCH" => {
                    let edges = rows(c, &format!("SELECT target_block_id,edge_kind FROM control_edges WHERE terminator_instruction_id={id} ORDER BY ordinal"));
                    let selected: Vec<_> = edges
                        .iter()
                        .filter(|e| match e[1].as_str() {
                            "UNCONDITIONAL" => true,
                            "BRANCH_TRUE" => consume(args[0], &operand_ids[0]) != 0,
                            "BRANCH_FALSE" => consume(args[0], &operand_ids[0]) == 0,
                            other => panic!("unexpected scalar edge {other}"),
                        })
                        .collect();
                    assert_eq!(selected.len(), 1);
                    next = Some(selected[0][0].parse::<i64>().unwrap());
                    None
                }
                other => panic!("unsupported Decide CIR operation {other}; do not skip it"),
            };
            if let Some(v) = value {
                values.insert(i[2].parse().unwrap(), v);
            }
        }
        previous = Some(block);
        block = next.expect("block must terminate");
    }
    panic!("Decide CIR step bound exceeded")
}

#[test]
fn cross_language_decide() {
    for (lang, suffix) in [
        ("go", "Decide"),
        ("rust", "decide"),
        ("java", "Decide.decide"),
    ] {
        let lab = MultiLab::new(lang);
        let name = lab.name(suffix);
        let c = lab.conn();
        let fid = go_support::function(&c, &name);
        let ds = decisions(&c, fid);
        assert_eq!(ds.len(), 2);
        assert!(ds.iter().all(|d| d[2] == "IF"));
        assert_eq!(
            ds.iter().map(|d| outcomes(&c, &d[0]).len()).sum::<usize>(),
            4
        );
        for x in [i32::MIN as i64, -11, -10, -9, -1, 0, 1, 10, i32::MAX as i64] {
            assert_eq!(
                execute_decide(&c, fid, x),
                if x < 0 { -1 } else { 0 },
                "{lang}: x={x}"
            );
            let context = lab.lab.dir.path().join("context.toml");
            std::fs::write(
                &context,
                format!("name = \"grid\"\n[[param_range]]\nparam = \"x\"\nmin = {x}\nmax = {x}\n"),
            )
            .unwrap();
            let analysis = lab.lab.ok(&[
                "branches",
                "analyze",
                "--entry",
                &name,
                "--context",
                context.to_str().unwrap(),
            ]);
            assert_eq!(analysis["exploration_closed"], true, "{lang}: {analysis}");
            for assumption in ["SEQUENTIAL_EXECUTION", "CLOSED_ENTRY_CONTEXT"] {
                assert!(analysis["assumptions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v == assumption));
            }
            let nav = lab
                .lab
                .ok(&["branches", "list", "--function", &name, "--run", "latest"]);
            let source = nav["source_decisions"].as_array().unwrap();
            for (di, d) in ds.iter().enumerate() {
                for (oi, o) in outcomes(&c, &d[0]).iter().enumerate() {
                    let actual = &source[di]["outcomes"][oi];
                    if o[4] == "1" {
                        assert_eq!(actual["status"], "UNANALYZED");
                        assert_eq!(actual["explanation"], "ELIDED");
                    } else {
                        let taken = if di == 0 {
                            (x < 0) == (oi == 0)
                        } else {
                            x >= 0 && oi == 1
                        };
                        assert_eq!(
                            actual["status"],
                            if taken {
                                "POSSIBLE"
                            } else {
                                "PROVEN_UNREACHABLE"
                            },
                            "{lang} x={x} d={di} o={oi}: {actual}"
                        );
                    }
                }
            }
        }
        lab.lab.ok(&["db", "check"]);
    }
}

#[test]
fn java_switch_ir() {
    let lab = MultiLab::new("java");
    let c = lab.conn();
    let fid = go_support::function(&c, &lab.name("Decide.day"));
    let switches = rows(&c, &format!("SELECT i.id FROM instructions i JOIN basic_blocks b ON b.id=i.block_id WHERE b.function_id={fid} AND i.opcode='SWITCH' AND i.is_terminator=1"));
    assert_eq!(switches.len(), 1);
    let edges = rows(&c, &format!("SELECT id,edge_kind FROM control_edges WHERE terminator_instruction_id={} ORDER BY ordinal", switches[0][0]));
    assert_eq!(edges.iter().filter(|e| e[1] == "SWITCH_DEFAULT").count(), 1);
    let mut cases = Vec::new();
    for e in edges.iter().filter(|e| e[1] == "SWITCH_CASE") {
        let mut stmt = c.prepare("SELECT v.literal FROM control_edge_case_values cv JOIN values_ir v ON v.id=cv.value_id WHERE cv.edge_id=? ORDER BY cv.ordinal").unwrap();
        for blob in stmt.query_map([&e[0]], |r| r.get::<_, Vec<u8>>(0)).unwrap() {
            let f = wire_fields(&blob.unwrap());
            let bytes = f.get(&2).expect("machine case literal");
            cases.push(bytes.iter().fold(0u64, |n, b| (n << 8) | u64::from(*b)));
        }
    }
    cases.sort();
    assert_eq!(cases, [1, 2, 3], "real case values, not just SWITCH count");
    let ds = decisions(&c, fid);
    assert_eq!(ds.len(), 1);
    let os = outcomes(&c, &ds[0][0]);
    assert_eq!(
        os.iter().map(|o| o[3].as_str()).collect::<Vec<_>>(),
        ["CASE:0", "CASE:1", "DEFAULT"]
    );
    for (o, expected) in os.iter().zip([vec![1u64], vec![2, 3], vec![]]) {
        let mut stmt = c.prepare("SELECT DISTINCT v.id,v.literal FROM source_ir_links l JOIN ir_outcomes io ON io.id=l.ir_outcome_id JOIN control_edge_case_values cv ON cv.edge_id=io.control_edge_id JOIN values_ir v ON v.id=cv.value_id WHERE l.source_outcome_id=?").unwrap();
        let mut actual = Vec::new();
        for blob in stmt.query_map([&o[0]], |r| r.get::<_, Vec<u8>>(1)).unwrap() {
            let fields = wire_fields(&blob.unwrap());
            actual.push(
                fields[&2]
                    .iter()
                    .fold(0u64, |n, b| (n << 8) | u64::from(*b)),
            );
        }
        actual.sort();
        assert_eq!(actual, expected, "source case grouping {}", o[3]);
    }
}

fn exception_edges(lab: &MultiLab, suffix: &str) -> Vec<Vec<String>> {
    let c = lab.conn();
    let fid = go_support::function(&c, &lab.name(suffix));
    rows(&c, &format!("SELECT i.id,e.edge_kind,COALESCE(t.descriptor,''),e.exception_catch_all,e.ordinal FROM instructions i JOIN basic_blocks b ON b.id=i.block_id JOIN control_edges e ON e.terminator_instruction_id=i.id LEFT JOIN cir_types t ON t.id=e.exception_filter_type_id WHERE b.function_id={fid} AND i.opcode='INVOKE' ORDER BY i.id,e.ordinal"))
}

#[test]
fn java_invoke_exception() {
    let lab = MultiLab::new("java");
    let edges = exception_edges(&lab, "Decide.guarded");
    assert!(edges.iter().any(|e| e[1] == "NORMAL"), "{edges:?}");
    assert!(
        edges
            .iter()
            .any(|e| e[1] == "EXCEPTION" && e[2].contains("RuntimeException")),
        "{edges:?}"
    );
    for e in edges.iter().filter(|e| e[1] == "EXCEPTION") {
        assert!(
            edges.iter().any(|n| n[0] == e[0] && n[1] == "NORMAL"),
            "same INVOKE must have normal continuation"
        );
    }
}

#[test]
fn java_reflection_unknown() {
    let lab = MultiLab::new("java");
    let c = lab.conn();
    let fid = go_support::function(&c, &lab.name("ReflectionUnknown.load"));
    let calls = rows(&c, &format!("SELECT cs.resolution_status,cs.world_closed FROM call_sites cs JOIN instructions i ON i.id=cs.instruction_id JOIN basic_blocks b ON b.id=i.block_id WHERE b.function_id={fid}"));
    assert!(calls.iter().any(|r| r == &["UNRESOLVED", "0"]), "{calls:?}");
    assert!(
        calls.iter().all(|r| r[1] == "0"),
        "reflection is not a closed world"
    );
    assert!(count(&c, &format!("SELECT COUNT(*) FROM diagnostics WHERE function_id={fid} AND code='CALL_TARGET_UNRESOLVED'")) > 0);
}

#[test]
fn rust_toolchain_mismatch() {
    let lab = MultiLab::prepare("rust");
    // Subprocess-only injection; never mutate process-global environment.
    let rustc = checked(Command::new("rustup").args(["which", "--toolchain", "1.99.0", "rustc"]));
    let rustc = String::from_utf8(rustc.stdout).unwrap();
    // The core deliberately filters its child's environment. Inject into the
    // adapter directly to test its public error protocol, not through that filter.
    let (out, frames) = list_frames_with_env(&lab, 1, Some(("SKE_RUSTC", rustc.trim())));
    assert_eq!(
        out.status.code(),
        Some(3),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(frames.len(), 1);
    let error = wire_fields(frames[0].get(&4).expect("AdapterError"));
    assert_eq!(error.get(&1).unwrap(), b"TOOLCHAIN_MISMATCH");
    assert!(!error.get(&2).unwrap().is_empty());
    assert_eq!(
        count(
            &lab.conn(),
            "SELECT COUNT(*) FROM snapshots WHERE status='CLOSED'"
        ),
        0
    );
}

#[test]
fn multilang_inspect() {
    for lang in ["rust", "java"] {
        let lab = MultiLab::prepare(lang);
        let denied = lab.ingest(false, None);
        assert_eq!(
            denied.status.code(),
            Some(2),
            "noninteractive consent must be required"
        );
        let allowed = lab.ingest(true, None);
        assert!(
            allowed.status.success(),
            "{}",
            String::from_utf8_lossy(&allowed.stdout)
        );
        let consent: Value = serde_json::from_slice(
            &std::fs::read(lab.lab.db.parent().unwrap().join(".ske/consent.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(consent.as_array().unwrap().len(), 1);
        assert_eq!(
            consent[0]["adapter"],
            lab.adapter.canonicalize().unwrap().to_str().unwrap()
        );
        let inspect = lab.lab.ok(&["inspect"]);
        let units = inspect["units"].as_array().expect("inspect units");
        assert!(!units.is_empty());
        for u in units {
            assert_eq!(u["language"], lang);
            assert!(!u["semantic_profile"].as_str().unwrap().is_empty());
            assert!(u["capability_flags"].is_array());
        }
        let sql = rows(&lab.conn(), "SELECT language,semantic_profile,capability_flags,inventory_status FROM compilation_units");
        assert_eq!(sql.len(), units.len());
        for u in sql {
            assert_eq!(u[0], lang);
            assert!(!u[1].is_empty());
            let _: Vec<String> = serde_json::from_str(&u[2]).unwrap();
            assert_ne!(u[3], "FAILED");
        }
        lab.lab.ok(&["db", "check"]);
    }
}

// Independent request encoding and frame splitting (not the production framer).
fn field(tag: u8, data: &[u8], out: &mut Vec<u8>) {
    out.push(tag);
    let mut n = data.len();
    while n >= 128 {
        out.push((n as u8 & 127) | 128);
        n >>= 7;
    }
    out.push(n as u8);
    out.extend_from_slice(data);
}

fn list_frames(lab: &MultiLab, major: u8) -> (Output, Vec<BTreeMap<u64, Vec<u8>>>) {
    list_frames_with_env(lab, major, None)
}

fn list_frames_with_env(
    lab: &MultiLab,
    major: u8,
    env: Option<(&str, &str)>,
) -> (Output, Vec<BTreeMap<u64, Vec<u8>>>) {
    let mut body = Vec::new();
    field(10, lab.repo.to_str().unwrap().as_bytes(), &mut body);
    field(18, profile(lab.lang).to_string().as_bytes(), &mut body);
    let mut request = vec![8, major];
    field(18, &body, &mut request);
    let mut command = Command::new(&lab.adapter);
    if let Some((key, value)) = env {
        command.env(key, value);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input
        .write_all(&(request.len() as u32).to_be_bytes())
        .unwrap();
    input.write_all(&request).unwrap();
    drop(input);
    let out = child.wait_with_output().unwrap();
    let mut bytes = out.stdout.as_slice();
    let mut frames = Vec::new();
    while !bytes.is_empty() {
        assert!(bytes.len() >= 4, "truncated frame header");
        let n = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
        assert!((1..=16_777_216).contains(&n), "frame length {n}");
        bytes = &bytes[4..];
        assert!(bytes.len() >= n, "truncated frame body");
        frames.push(wire_fields(&bytes[..n]));
        bytes = &bytes[n..];
    }
    (out, frames)
}

#[test]
fn multilang_integrity_framing() {
    for lang in ["go", "rust", "java"] {
        let lab = MultiLab::new(lang);
        assert_eq!(
            rows(&lab.conn(), "PRAGMA integrity_check"),
            vec![vec!["ok"]]
        );
        assert!(rows(&lab.conn(), "PRAGMA foreign_key_check").is_empty());
        let (out, frames) = list_frames(&lab, 1);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(frames.len() >= 2);
        assert!(
            frames.last().unwrap().contains_key(&2),
            "ListUnitsDone is last"
        );
        let mut keys = Vec::new();
        for f in &frames[..frames.len() - 1] {
            assert_eq!(f.len(), 1);
            let unit = wire_fields(f.get(&1).expect("UnitDescriptor"));
            assert_eq!(unit.get(&2).unwrap(), lang.as_bytes());
            keys.push(String::from_utf8(unit.get(&1).unwrap().clone()).unwrap());
        }
        let mut sorted = keys.clone();
        // §9.3 sorts Go package paths before their @profile suffix, not
        // raw unit keys ("example" precedes "example/partial").
        sorted.sort_by(|a, b| {
            a.rsplit_once('@')
                .unwrap_or((a.as_str(), ""))
                .cmp(&b.rsplit_once('@').unwrap_or((b.as_str(), "")))
        });
        sorted.dedup();
        assert_eq!(keys, sorted, "unique sorted unit descriptors");
        let (bad, frames) = list_frames(&lab, 99);
        assert!(!bad.status.success(), "unsupported major must fail");
        assert_eq!(frames.len(), 1);
        let error = wire_fields(frames[0].get(&4).expect("AdapterError"));
        assert!(!error.get(&1).unwrap().is_empty());
        assert!(!error.get(&2).unwrap().is_empty());
        lab.lab.ok(&["db", "check"]);
    }
}

#[test]
fn cir_compat_multilang() {
    // Real import exercises the unchanged CIR validator and SQLite schema, not
    // merely serialization against an adapter's own generated protobuf classes.
    for lang in ["go", "rust", "java"] {
        let lab = MultiLab::new(lang);
        let c = lab.conn();
        assert_eq!(
            count(&c, "SELECT COUNT(*) FROM snapshots WHERE status='CLOSED'"),
            1
        );
        assert_eq!(
            count(
                &c,
                "SELECT COUNT(*) FROM diagnostics WHERE severity='ERROR'"
            ),
            0
        );
        assert!(count(&c, "SELECT COUNT(*) FROM instructions") > 0);
        // v0.6 allows a non-elided source outcome whose IR activation is not yet
        // modeled, provided it is not silently closed: an explicit INVENTORY
        // diagnostic must name every such outcome. AC-0.6-06 forbids forcing
        // PROVEN_UNREACHABLE for these outcomes; the diagnostic keeps them
        // visible as incomplete rather than attesting exactness.
        let unlinked = count(
            &c,
            "SELECT COUNT(*) FROM source_outcomes o WHERE o.elided=0 AND NOT EXISTS (SELECT 1 FROM source_ir_links l WHERE l.source_outcome_id=o.id)",
        );
        if unlinked > 0 {
            let diagnosed = count(
                &c,
                "SELECT COUNT(DISTINCT o.id) FROM source_outcomes o JOIN source_decisions d ON d.id=o.decision_id JOIN diagnostics di ON di.function_id=d.function_id WHERE o.elided=0 AND NOT EXISTS (SELECT 1 FROM source_ir_links l WHERE l.source_outcome_id=o.id) AND di.severity IN ('WARNING','ERROR') AND di.code IN ('SOURCE_MISMATCH','RUST_SOURCE_MAPPING_PARTIAL')",
            );
            assert_eq!(diagnosed, unlinked, "every unlinked outcome is diagnosed");
        }
        lab.lab.ok(&["db", "check"]);
    }
}

#[test]
fn java_multi_catch_finally() {
    let lab = MultiLab::new("java");
    let c = lab.conn();
    let fid = go_support::function(&c, &lab.name("MultiCatchFinally.guarded"));
    let edges = exception_edges(&lab, "MultiCatchFinally.guarded");
    let mut witnessed_order = false;
    for first in edges
        .iter()
        .filter(|e| e[1] == "EXCEPTION" && e[2].contains("NumberFormatException"))
    {
        if let Some(second) = edges
            .iter()
            .find(|e| e[0] == first[0] && e[1] == "EXCEPTION" && e[2].contains("RuntimeException"))
        {
            assert!(
                first[4].parse::<u32>().unwrap() < second[4].parse::<u32>().unwrap(),
                "handler order {edges:?}"
            );
            assert!(edges.iter().any(|e| e[0] == first[0] && e[1] == "NORMAL"));
            assert!(
                edges
                    .iter()
                    .any(|e| e[0] == first[0] && e[1] == "EXCEPTION" && e[3] == "1"),
                "uncaught propagation/finally handler must be inventoried"
            );
            witnessed_order = true;
        }
    }
    assert!(
        witnessed_order,
        "ordered typed handlers from actual INVOKE: {edges:?}"
    );
    let diagnostics = rows(&c, &format!("SELECT impact FROM diagnostics WHERE function_id={fid} AND code='EXCEPTION_FLOW_PARTIAL'"));
    // Exact lowering must put finally on every terminating path, not merely
    // contain a println somewhere in the function. Otherwise require the
    // explicit, correctly scoped precision boundary below.
    if diagnostics.is_empty() {
        let entry: i64 = c
            .query_row(
                "SELECT entry_block_id FROM functions WHERE id=?",
                [fid],
                |r| r.get(0),
            )
            .unwrap();
        let mut work = vec![(entry, false)];
        let mut seen = std::collections::BTreeSet::new();
        let mut exits = 0;
        while let Some((block, mut finalized)) = work.pop() {
            if !seen.insert((block, finalized)) {
                continue;
            }
            let targets = rows(&c, &format!("SELECT s.qualified_name FROM call_sites cs JOIN instructions i ON i.id=cs.instruction_id JOIN call_targets ct ON ct.call_site_id=cs.id JOIN functions f ON f.id=ct.target_function_id JOIN symbols s ON s.id=f.symbol_id WHERE i.block_id={block}"));
            finalized |= targets.iter().any(|t| t[0].contains("println"));
            let next = rows(
                &c,
                &format!("SELECT target_block_id FROM control_edges WHERE source_block_id={block}"),
            );
            if next.is_empty() {
                let terminal: String = c
                    .query_row(
                        "SELECT opcode FROM instructions WHERE block_id=? AND is_terminator=1",
                        [block],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert!(
                    ["RETURN", "RAISE", "PANIC"].contains(&terminal.as_str()),
                    "unmodeled exit {terminal}"
                );
                assert!(
                    finalized,
                    "exit bypasses finally without precision diagnostic"
                );
                exits += 1;
            }
            for n in next {
                work.push((n[0].parse().unwrap(), finalized));
            }
        }
        assert!(exits > 0, "must reach actual exits");
    }
    assert!(diagnostics
        .iter()
        .all(|d| ["INVENTORY", "EFFECTS"].contains(&d[0].as_str())));
    let status: String = c
        .query_row(
            "SELECT inventory_status FROM functions WHERE id=?",
            [fid],
            |r| r.get(0),
        )
        .unwrap();
    if diagnostics.iter().any(|d| d[0] == "INVENTORY") {
        assert_eq!(status, "PARTIAL");
    }
    lab.lab.ok(&["db", "check"]);
}
