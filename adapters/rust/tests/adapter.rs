use ske_adapter_rust::{compiler::Compiler, frame, lower, profile::Profile, source};
use ske_cir::{
    adapter_proto as a,
    proto::{self as p, record::Content as C},
};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/rust-branch-lab")
        .canonicalize()
        .unwrap()
}
fn extract_text(text: &str) -> Vec<p::Record> {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::write(root.path().join("src/lib.rs"), text).unwrap();
    let profile = Profile::default();
    let inventory = source::inventory(&profile.entry, text).unwrap();
    let compiler =
        Compiler::discover().expect("pinned compiler is required; this test must not be skipped");
    let mir = compiler.mir(root.path(), &profile).unwrap();
    lower::extract(&compiler, &profile, &inventory, &mir, text).unwrap()
}
fn fixture() -> Vec<p::Record> {
    extract_text(&std::fs::read_to_string(root().join("src/lib.rs")).unwrap())
}
fn snippet(text: &str, s: &p::Span) -> String {
    let lines: Vec<_> = text.split_inclusive('\n').collect();
    let start: usize = lines[..s.start_line as usize - 1]
        .iter()
        .map(|l| l.len())
        .sum::<usize>()
        + s.start_column as usize
        - 1;
    let end: usize = lines[..s.end_line as usize - 1]
        .iter()
        .map(|l| l.len())
        .sum::<usize>()
        + s.end_column as usize
        - 1;
    text[start..end].into()
}

#[test]
fn rust_golden_decisions() {
    let records = fixture();
    let text = std::fs::read_to_string(root().join("src/lib.rs")).unwrap();
    let decisions: Vec<_> = records
        .iter()
        .filter_map(|r| match &r.content {
            Some(C::SourceDecisionDef(d)) => Some((
                d.function_id.as_str(),
                d.decision_kind.as_str(),
                snippet(&text, d.span.as_ref().unwrap()),
                d.outcomes
                    .iter()
                    .map(|o| o.label.as_str())
                    .collect::<Vec<_>>(),
            )),
            _ => None,
        })
        .collect();
    // Hand-transcribed from normative §24, never generated from adapter output.
    assert_eq!(
        decisions,
        vec![
            (
                "rust:fn:decide",
                "IF",
                "x < 0".into(),
                vec!["TRUE", "FALSE"]
            ),
            (
                "rust:fn:decide",
                "IF",
                "x < -10".into(),
                vec!["TRUE", "FALSE"]
            ),
            (
                "rust:fn:kind",
                "MATCH",
                "n".into(),
                vec!["CASE:0", "CASE:1", "DEFAULT"]
            ),
            (
                "rust:fn:parse_len",
                "ERROR_PROPAGATION",
                "s.parse()?".into(),
                vec!["SUCCESS", "ERROR"]
            ),
        ]
    );
    let mapped: Vec<_> = records
        .iter()
        .filter_map(|r| match &r.content {
            Some(C::SourceDecisionDef(d)) if d.function_id == "rust:fn:decide" => Some(d),
            _ => None,
        })
        .collect();
    assert!(mapped
        .iter()
        .flat_map(|d| &d.outcomes)
        .all(|o| o.activation_mapping_complete && !o.elided));
    assert!(records.iter().any(|r| matches!(&r.content, Some(C::EndUnit(e)) if e.expected_source_decisions == 4 && e.emitted_source_decisions == 4 && e.inventory_status == "PARTIAL")));
}

#[test]
fn lang_types() {
    let records = fixture();
    for (id, width, signed) in [("rust:type:i32", 32, true), ("rust:type:u8", 8, false)] {
        assert!(records.iter().any(|r| matches!(&r.content, Some(C::TypeDef(t)) if t.id == id && t.kind == "BITVECTOR" && t.bit_width == width && t.is_signed == signed)));
    }
}

#[test]
fn rust_real_mir_cfg_and_import() {
    let records = fixture();
    assert!(records.iter().any(|r| matches!(&r.content, Some(C::InstructionDef(i)) if i.opcode == "SWITCH" && i.block_id.starts_with("rust:fn:kind/"))));
    assert!(records.iter().any(|r| matches!(&r.content, Some(C::InstructionDef(i)) if i.opcode == "INVOKE" && i.block_id.starts_with("rust:fn:parse_len/"))));
    let scratch = tempfile::tempdir().unwrap();
    let cir = scratch.path().join("unit.pb");
    let mut out = std::fs::File::create(&cir).unwrap();
    for record in &records {
        frame::write(&mut out, record, frame::LIMIT).unwrap();
    }
    let storage = ske_storage::Storage::open(scratch.path().join("ske.db")).unwrap();
    let result =
        ske_ingest::ingest_many(&storage, &[cir], Some(&root()), Some("rust-test")).unwrap();
    assert_eq!(result["inventory_status"], "PARTIAL");
    let integrity: String = storage
        .conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
    let fk: i64 = storage
        .conn
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(fk, 0);
}

#[test]
fn rust_loops_and_arithmetic_representable() {
    // Real Rust loops increment loop counters (`i += 1`) and accumulate
    // (`acc = acc + i`). Without -Coverflow-checks=off the debug MIR camera
    // injects overflow-check assert terminators into + - * << >> and negation,
    // which the lowerer rejects fail-closed (MIR_UNSUPPORTED): loops and
    // arithmetic like `end - start` were unrepresentable. This guards the flag
    // that keeps those asserts out of the studied MIR.
    let text = "pub fn sum(n: i32) -> i32 {\n    let mut acc = 0;\n    let mut i = 0;\n    while i < n {\n        acc = acc + i;\n        i = i + 1;\n    }\n    acc\n}\npub fn diff(a: i32, b: i32) -> i32 { a - b }\n";
    let records = extract_text(text);
    // The fix's point: the unit no longer aborts on overflow-check asserts.
    assert!(!records
        .iter()
        .any(|r| matches!(&r.content, Some(C::Diagnostic(d)) if d.code == "MIR_UNSUPPORTED")));
    // Arithmetic is conservatively opaque (the adapter models comparisons, not
    // binops), but it must lower instead of killing the whole function.
    assert!(records
        .iter()
        .any(|r| matches!(&r.content, Some(C::InstructionDef(i)) if i.opcode == "OPAQUE_OP")));
    // A loop back-edge is an UNCONDITIONAL edge back to an earlier block.
    let back = |id: &str| -> u32 {
        id.rsplit_once('/')
            .unwrap()
            .1
            .trim_start_matches("bb")
            .parse()
            .unwrap()
    };
    let has_back_edge = records.iter().any(|r| match &r.content {
        Some(C::ControlEdgeDef(e))
            if e.edge_kind == "UNCONDITIONAL" && e.source_block_id.starts_with("rust:fn:sum/") =>
        {
            back(&e.source_block_id) > back(&e.target_block_id)
        }
        _ => false,
    });
    assert!(
        has_back_edge,
        "loop back-edge missing: the while body was not lowered"
    );
}

#[test]
fn rust_division_and_remainder_representable() {
    // Real Rust `/` and `%` compile to MIR runtime-check assert terminators
    // (division by zero, and MIN / -1 overflow). The lowerer used to reject any
    // `assert(...) -> [success: .., unwind ..]` terminator fail-closed
    // (MIR_UNSUPPORTED), so one division killed the whole function: ripgrep,
    // regex and regex-lite all hit it. Those asserts are real Rust panics, and
    // are now lowered as a real BRANCH: the BRANCH_FALSE edge (guarded, divisor
    // proven nonzero) continues to the division, the BRANCH_TRUE edge reaches a
    // per-function synthetic PANIC block that models the real panic.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    let text = "pub fn sum_mod(n: i32, d: i32) -> i32 {\n    let mut acc = 0;\n    let mut i = 0;\n    while i < n {\n        if i % 2 == 0 {\n            acc = acc + i;\n        }\n        i = i + 1;\n    }\n    acc / d + acc % d\n}\n";
    std::fs::write(dir.path().join("src/lib.rs"), text).unwrap();
    let profile = Profile::default();
    let inventory = source::inventory(&profile.entry, text).unwrap();
    let compiler =
        Compiler::discover().expect("pinned compiler is required; this test must not be skipped");
    let mir = compiler.mir(dir.path(), &profile).unwrap();
    let records = lower::extract(&compiler, &profile, &inventory, &mir, text).unwrap();
    // (a) The unit no longer aborts on the division/remainder asserts.
    assert!(!records
        .iter()
        .any(|r| matches!(&r.content, Some(C::Diagnostic(d)) if d.code == "MIR_UNSUPPORTED")));
    // (b) The division panics are modeled as a real PANIC, not silently
    // dropped: a per-function /panic block, a PANIC terminator, and BRANCH_TRUE
    // edges (the panic condition) pointing at it from the assert sites. The
    // guarded BRANCH_FALSE edges carry the negated panic condition, i.e. the
    // division path is the one where the divisor is proven nonzero.
    assert!(
        records
            .iter()
            .any(|r| matches!(&r.content, Some(C::BlockDef(b)) if b.id.ends_with("/panic"))),
        "missing per-function /panic block: the division panics were dropped, not modeled"
    );
    assert!(records.iter().any(|r| matches!(&r.content, Some(C::InstructionDef(i)) if i.opcode == "PANIC" && i.is_terminator)), "missing PANIC terminator: the division panics were dropped, not modeled");
    assert!(records.iter().any(|r| matches!(&r.content, Some(C::ControlEdgeDef(e)) if e.edge_kind == "BRANCH_TRUE" && e.target_block_id.ends_with("/panic") && !e.guard_negated)), "no BRANCH_TRUE edge targets the /panic block");
    assert!(records.iter().any(|r| matches!(&r.content, Some(C::ControlEdgeDef(e)) if e.edge_kind == "BRANCH_FALSE" && e.guard_negated && !e.target_block_id.ends_with("/panic"))), "no guarded BRANCH_FALSE edge continues to the division path");
    // (c) The loop back-edge survives the division asserts: the while body
    // still lowers instead of the whole function failing.
    let back = |id: &str| -> u32 {
        id.rsplit_once('/')
            .unwrap()
            .1
            .trim_start_matches("bb")
            .parse()
            .unwrap()
    };
    assert!(
        records.iter().any(|r| match &r.content {
            Some(C::ControlEdgeDef(e))
                if e.edge_kind == "UNCONDITIONAL"
                    && e.source_block_id.starts_with("rust:fn:sum_mod/") =>
            {
                back(&e.source_block_id) > back(&e.target_block_id)
            }
            _ => false,
        }),
        "loop back-edge missing: the while body was not lowered"
    );
    // The authoritative relational/dominance import must accept the new CFG
    // (block with only a PANIC terminator, BRANCH guards referencing SSA temps).
    let scratch = tempfile::tempdir().unwrap();
    let cir = scratch.path().join("unit.pb");
    let mut out = std::fs::File::create(&cir).unwrap();
    for record in &records {
        frame::write(&mut out, record, frame::LIMIT).unwrap();
    }
    let storage = ske_storage::Storage::open(scratch.path().join("ske.db")).unwrap();
    let result =
        ske_ingest::ingest_many(&storage, &[cir], Some(dir.path()), Some("rust-test")).unwrap();
    assert_eq!(result["inventory_status"], "PARTIAL");
}

#[test]
fn rust_generic_not_fixture_constants() {
    let text = "pub fn unrelated(value: i16) -> i16 { if value >= 37 { 12 } else { -8 } }\n";
    let records = extract_text(text);
    assert!(records
        .iter()
        .any(|r| matches!(&r.content, Some(C::InstructionDef(i)) if i.opcode == "GE")));
    assert!(records.iter().any(|r| matches!(&r.content, Some(C::SourceDecisionDef(d)) if snippet(text, d.span.as_ref().unwrap()) == "value >= 37")));
    assert!(!records
        .iter()
        .any(|r| matches!(&r.content, Some(C::SymbolDef(s)) if s.name == "decide")));
}

fn request(extract: bool) -> a::AdapterRequest {
    let body = if extract {
        a::adapter_request::Body::ExtractUnit(a::ExtractUnitRequest {
            project_root: root().to_string_lossy().into(),
            build_profile_json: "{}".into(),
            unit_key: "rust:src/lib.rs".into(),
            ..Default::default()
        })
    } else {
        a::adapter_request::Body::ListUnits(a::ListUnitsRequest {
            project_root: root().to_string_lossy().into(),
            build_profile_json: "{}".into(),
            ..Default::default()
        })
    };
    a::AdapterRequest {
        protocol_major: 1,
        body: Some(body),
    }
}

fn invoke(request: &a::AdapterRequest, compiler: Option<&str>) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ske-adapter-rust"));
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(compiler) = compiler {
        command.env("SKE_RUSTC", compiler);
    }
    let mut child = command.spawn().unwrap();
    frame::write(&mut child.stdin.take().unwrap(), request, frame::LIMIT).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn rust_toolchain_mismatch() {
    let build_compiler = Command::new("rustup")
        .args(["which", "--toolchain", "1.99.0", "rustc"])
        .output()
        .unwrap();
    assert!(build_compiler.status.success());
    let build_compiler = String::from_utf8(build_compiler.stdout).unwrap();
    for compiler in ["/nonexistent/ske-rustc", build_compiler.trim()] {
        let output = invoke(&request(false), Some(compiler));
        assert_eq!(output.status.code(), Some(3));
        let response: a::AdapterResponse = frame::read(&mut output.stdout.as_slice()).unwrap();
        assert!(
            matches!(response.body, Some(a::adapter_response::Body::Error(e)) if e.code == "TOOLCHAIN_MISMATCH")
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("TOOLCHAIN_MISMATCH"));
    }
}

#[test]
fn rust_adapter_framing() {
    for extract in [false, true] {
        let output = invoke(&request(extract), None);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut bytes = output.stdout.as_slice();
        let mut responses = vec![];
        while !bytes.is_empty() {
            responses.push(frame::read::<a::AdapterResponse>(&mut bytes).unwrap());
        }
        if extract {
            assert!(matches!(
                responses.first().unwrap().body,
                Some(a::adapter_response::Body::Record(p::Record {
                    content: Some(C::Header(_))
                }))
            ));
            assert!(matches!(
                responses.last().unwrap().body,
                Some(a::adapter_response::Body::Record(p::Record {
                    content: Some(C::EndUnit(_))
                }))
            ));
        } else {
            assert_eq!(responses.len(), 2);
            assert!(matches!(
                responses[0].body,
                Some(a::adapter_response::Body::Unit(_))
            ));
            assert!(matches!(
                responses[1].body,
                Some(a::adapter_response::Body::ListDone(_))
            ));
        }
    }
    assert!(frame::read::<a::AdapterRequest>(&mut [255u8; 4].as_slice()).is_err());
    assert!(frame::read::<a::AdapterRequest>(&mut [0, 0, 0, 4, 0].as_slice()).is_err());
}

#[test]
fn rust_no_untrusted_cargo_or_macro_execution() {
    assert!(source::inventory("src/lib.rs", "include!(\"/etc/passwd\");").is_err());
    assert!(source::inventory("src/lib.rs", "#[derive(Evil)] struct S;").is_err());
    assert!(source::inventory("src/lib.rs", "mod outside;").is_err());
    assert!(source::inventory("src/lib.rs", "#[cfg(unix)] pub fn f() {}").is_err());
    assert!(Profile::parse("{\"rustflags\":[\"--extern=evil\"]}").is_err());
    assert!(Profile::parse("{\"entry\":\"../outside.rs\"}").is_err());
    let scratch = tempfile::tempdir().unwrap();
    std::fs::create_dir(scratch.path().join("src")).unwrap();
    std::fs::write(scratch.path().join("src/lib.rs"), "pub fn f() {}").unwrap();
    std::fs::write(
        scratch.path().join("build.rs"),
        "compile_error!(\"must not run\");",
    )
    .unwrap();
    let mut req = request(false);
    if let Some(a::adapter_request::Body::ListUnits(r)) = req.body.as_mut() {
        r.project_root = scratch.path().to_string_lossy().into();
    }
    let output = invoke(&req, None);
    assert_eq!(output.status.code(), Some(1));
    let response: a::AdapterResponse = frame::read(&mut output.stdout.as_slice()).unwrap();
    assert!(
        matches!(response.body, Some(a::adapter_response::Body::Error(e)) if e.code == "RUST_BUILD_SCRIPT_UNSUPPORTED")
    );
}

#[test]
fn rust_profile_closed_contract() {
    let p = Profile::parse(r#"{"profile_version":1,"language":"rust","target":null,"features":["b","a","a"],"cfg":[],"entry":"src/lib.rs"}"#).unwrap();
    assert_eq!(p.features, ["a", "b"]);
    assert!(Profile::parse("{\"language\":\"go\"}").is_err());
    assert!(Profile::parse("{\"profile_version\":2}").is_err());
    assert!(Profile::parse("{} {}").is_err());
}

#[test]
fn rust_utf8_source_provenance() {
    let text = "pub fn café(x: i32) -> i32 { if x < 0 { -1 } else { 0 } }\n";
    let records = extract_text(text);
    let decision = records
        .iter()
        .find_map(|r| match &r.content {
            Some(C::SourceDecisionDef(d)) => Some(d),
            _ => None,
        })
        .unwrap();
    assert_eq!(snippet(text, decision.span.as_ref().unwrap()), "x < 0");
    assert!(decision
        .outcomes
        .iter()
        .all(|o| o.activation_mapping_complete));
}

#[test]
fn rust_compound_mapping_never_guessed() {
    let text = "pub fn compound(a: bool, b: bool) -> bool { if a && b { true } else { false } }\n";
    let records = extract_text(text);
    let decision = records
        .iter()
        .find_map(|r| match &r.content {
            Some(C::SourceDecisionDef(d)) if d.decision_kind == "IF" => Some(d),
            _ => None,
        })
        .unwrap();
    assert!(decision
        .outcomes
        .iter()
        .all(|o| !o.activation_mapping_complete && !o.elided));
    assert!(records.iter().any(|r| matches!(&r.content,
        Some(C::Diagnostic(d)) if d.code == "RUST_SOURCE_MAPPING_PARTIAL")));
}

#[test]
fn rust_timeout_and_pin_consistency() {
    let compiler = Compiler::discover().unwrap();
    let error = compiler
        .mir_source(
            "pub fn f() {}",
            &Profile::default(),
            std::time::Duration::ZERO,
        )
        .unwrap_err();
    assert_eq!(error.code, "LIMIT_EXCEEDED");
    let pin = include_str!("../compiler-pin.toml");
    assert!(pin.contains(ske_adapter_rust::compiler::HASH));
    assert!(pin.contains(ske_adapter_rust::compiler::RELEASE));
    assert!(pin.contains(ske_adapter_rust::compiler::PIN));
}
