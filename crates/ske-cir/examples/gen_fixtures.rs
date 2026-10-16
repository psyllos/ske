//! Hand-authored CIR programs from specification §15.2. No Go execution.
use prost::Message;
use ske_cir::proto::{
    literal_value::Representation, operation_payload::Details, record::Content, *,
};
use ske_common::sha256_hex;
use std::{
    fs,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
};

const UNIT: &str = "go:example@linux/amd64";
const INT: &str = "go:int@amd64";
const BOOL: &str = "go:bool";

#[cfg(test)]
#[path = "support/fixture_tests.rs"]
mod fixture_tests;

fn rec(content: Content) -> Record {
    Record {
        content: Some(content),
    }
}
fn strings(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|x| (*x).into()).collect()
}
fn payload(details: Details) -> Option<OperationPayload> {
    Some(OperationPayload {
        details: Some(details),
        ..Default::default()
    })
}
fn binary(op: &str) -> Option<OperationPayload> {
    payload(Details::Binary(BinarySemantics {
        operator: op.into(),
        overflow_mode: "WRAP".into(),
        signedness: "SIGNED".into(),
    }))
}
fn call(kind: &str) -> Option<OperationPayload> {
    payload(Details::Call(CallSemantics {
        dispatch_kind: kind.into(),
        may_raise: Some(true),
        may_mutate_memory: Some(true),
        ..Default::default()
    }))
}

struct Fixture {
    records: Vec<Record>,
    function: String,
    path: String,
}
impl Fixture {
    fn new(name: &str, path: &str) -> Self {
        let records = vec![
            rec(Content::Header(Header {
                cir_major: 1,
                cir_minor: 3,
                adapter_name: "cir-fixture-gen".into(),
                adapter_version: "1".into(),
                language: "go".into(),
                toolchain_version: "fixture".into(),
                build_fingerprint: sha256_hex(b"fixture"),
                semantic_profile: "go-fixture-linux-amd64".into(),
                ..Default::default()
            })),
            rec(Content::UnitDef(UnitDef {
                unit_id: UNIT.into(),
                unit_key: UNIT.into(),
                language: "go".into(),
            })),
        ];
        Self {
            records,
            function: format!("example.{name}"),
            path: path.into(),
        }
    }
    fn id(&self, suffix: &str) -> String {
        format!("{}/{suffix}", self.function)
    }
    fn span(&self, s: (u32, u32, u32, u32)) -> Span {
        Span {
            path: self.path.clone(),
            start_line: s.0,
            start_column: s.1,
            end_line: s.2,
            end_column: s.3,
        }
    }
    fn source(&mut self, root: &Path, expected: &str) -> io::Result<()> {
        let bytes = fs::read(root.join(&self.path))?;
        let hash = sha256_hex(&bytes);
        if hash != expected {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("source changed: {}: {hash}", self.path),
            ));
        }
        self.records.push(rec(Content::SourceFileDef(SourceFileDef {
            path: self.path.clone(),
            language: "go".into(),
            content_sha256: hash,
        })));
        Ok(())
    }
    fn ty(&mut self, id: &str, kind: &str, params: &[&str], results: &[&str]) {
        self.records.push(rec(Content::TypeDef(TypeDef {
            id: id.into(),
            kind: kind.into(),
            canonical_descriptor: id.into(),
            param_type_ids: strings(params),
            result_type_ids: strings(results),
            bit_width: if id == INT { 64 } else { 0 },
            is_signed: id == INT,
            nullable: kind == "POINTER",
            ..Default::default()
        })));
    }
    fn symbol(&mut self, id: &str, kind: &str, ty: &str, spans: Vec<Span>) {
        self.records.push(rec(Content::SymbolDef(SymbolDef {
            id: id.into(),
            owner_id: if kind == "PACKAGE" {
                String::new()
            } else {
                "example".into()
            },
            name: id.rsplit('.').next().unwrap().into(),
            qualified_name: id.into(),
            kind: kind.into(),
            type_id: ty.into(),
            spans,
        })));
    }
    fn function(&mut self, ty: &str, params: &[&str], span: Option<(u32, u32, u32, u32)>) {
        self.symbol("example", "PACKAGE", "", vec![]);
        self.symbol(
            &self.function.clone(),
            "FUNCTION",
            ty,
            span.map(|s| vec![self.span(s)]).unwrap_or_default(),
        );
        self.records.push(rec(Content::FunctionDef(FunctionDef {
            id: self.function.clone(),
            symbol_id: self.function.clone(),
            parameter_value_ids: params.iter().map(|x| self.id(x)).collect(),
            return_type_ids: vec![INT.into()],
            entry_block_id: self.id("b0"),
            is_external: false,
        })));
    }
    fn value(
        &mut self,
        suffix: &str,
        ty: &str,
        role: &str,
        literal: Option<Representation>,
        name: &str,
        index: Option<u32>,
    ) {
        self.records.push(rec(Content::ValueDef(ValueDef {
            id: self.id(suffix),
            function_id: self.function.clone(),
            type_id: ty.into(),
            role: role.into(),
            literal: literal.map(|representation| LiteralValue {
                representation: Some(representation),
            }),
            name: name.into(),
            param_index: index,
            ..Default::default()
        })));
    }
    fn constant(&mut self, n: u32, value: i64) {
        self.value(
            &format!("k{n}"),
            INT,
            "CONST",
            Some(Representation::MachineBitsBe(value.to_be_bytes().to_vec())),
            "",
            None,
        );
    }
    fn global(&mut self, symbol: &str, ty: &str) {
        self.records.push(rec(Content::ValueDef(ValueDef {
            id: format!("global:{symbol}"),
            symbol_id: symbol.into(),
            type_id: ty.into(),
            role: "GLOBAL".into(),
            ..Default::default()
        })));
    }
    fn blocks(&mut self, n: u32) {
        for ordinal in 0..n {
            self.records.push(rec(Content::BlockDef(BlockDef {
                id: self.id(&format!("b{ordinal}")),
                function_id: self.function.clone(),
                ordinal,
            })));
        }
    }
    #[allow(clippy::too_many_arguments)] // Literal transcription of a CIR instruction row.
    fn inst(
        &mut self,
        b: u32,
        i: u32,
        op: &str,
        result: &str,
        operands: &[&str],
        data: Option<OperationPayload>,
        span: Option<(u32, u32, u32, u32)>,
    ) {
        let data = if op == "LOAD" && data.is_none() {
            payload(Details::Memory(MemorySemantics {
                access_kind: "LOAD".into(),
                ..Default::default()
            }))
        } else {
            data
        };
        let resolve = |s: &str| {
            if s.starts_with("global:") {
                s.into()
            } else {
                self.id(s)
            }
        };
        self.records
            .push(rec(Content::InstructionDef(InstructionDef {
                id: self.id(&format!("b{b}/i{i}")),
                block_id: self.id(&format!("b{b}")),
                ordinal: i,
                opcode: op.into(),
                result_value_id: if result.is_empty() {
                    String::new()
                } else {
                    self.id(result)
                },
                operand_value_ids: operands.iter().map(|s| resolve(s)).collect(),
                payload: data,
                spans: span.map(|s| vec![self.span(s)]).unwrap_or_default(),
                is_terminator: matches!(op, "RETURN" | "BRANCH" | "JUMP" | "SWITCH" | "INVOKE"),
            })));
    }
    #[allow(clippy::too_many_arguments)] // Literal transcription of a CIR edge row.
    fn edge(
        &mut self,
        b: u32,
        i: u32,
        ordinal: u32,
        target: u32,
        kind: &str,
        label: &str,
        guard: &str,
        cases: &[&str],
    ) -> String {
        let id = self.id(&format!("b{b}/i{i}/e{ordinal}"));
        self.records
            .push(rec(Content::ControlEdgeDef(ControlEdgeDef {
                id: id.clone(),
                source_block_id: self.id(&format!("b{b}")),
                target_block_id: self.id(&format!("b{target}")),
                terminator_instruction_id: self.id(&format!("b{b}/i{i}")),
                outcome_ordinal: ordinal,
                outcome_label: label.into(),
                guard_value_id: if guard.is_empty() {
                    String::new()
                } else {
                    self.id(guard)
                },
                guard_negated: kind == "BRANCH_FALSE",
                edge_kind: kind.into(),
                case_value_ids: cases.iter().map(|s| self.id(s)).collect(),
                exception_catch_all: kind == "EXCEPTION",
                ..Default::default()
            })));
        id
    }
    fn branch(
        &mut self,
        b: u32,
        i: u32,
        guard: &str,
        targets: [u32; 2],
        span: (u32, u32, u32, u32),
    ) -> Vec<String> {
        self.inst(b, i, "BRANCH", "", &[guard], None, Some(span));
        vec![
            self.edge(b, i, 0, targets[0], "BRANCH_TRUE", "TRUE", guard, &[]),
            self.edge(b, i, 1, targets[1], "BRANCH_FALSE", "FALSE", guard, &[]),
        ]
    }
    #[allow(clippy::too_many_arguments)] // Source decision and its direct mapping.
    fn decision(
        &mut self,
        n: u32,
        kind: &str,
        span: (u32, u32, u32, u32),
        labels: &[&str],
        edges: &[String],
        inverted: bool,
    ) {
        let id = self.id(&format!("sd{n}"));
        self.records
            .push(rec(Content::SourceDecisionDef(SourceDecisionDef {
                id: id.clone(),
                decision_kind: kind.into(),
                span: Some(self.span(span)),
                function_id: self.function.clone(),
                outcomes: labels
                    .iter()
                    .enumerate()
                    .map(|(i, label)| SourceOutcomeDef {
                        id: format!("{id}/o{i}"),
                        label: (*label).into(),
                        elided: false,
                        activation_mapping_complete: true,
                    })
                    .collect(),
                ..Default::default()
            })));
        for (i, edge) in edges.iter().enumerate() {
            let outcome = format!("{id}/o{i}");
            self.records
                .push(rec(Content::SourceIrLink(SourceIrLinkDef {
                    source_outcome_id: outcome.clone(),
                    control_edge_id: edge.clone(),
                    mapping_kind: if kind == "SWITCH" {
                        "SWITCH_CASE"
                    } else {
                        "DIRECT"
                    }
                    .into(),
                    quality: "EXACT".into(),
                    inverted,
                })));
            self.records.push(rec(Content::SourceOutcomeActivationCase(
                SourceOutcomeActivationCaseDef {
                    source_outcome_id: outcome,
                    case_ordinal: 0,
                    ordered_control_edge_ids: vec![edge.clone()],
                    quality: "EXACT".into(),
                },
            )));
        }
    }
    fn finish(mut self) -> Vec<Record> {
        let decisions = self
            .records
            .iter()
            .filter(|r| matches!(r.content, Some(Content::SourceDecisionDef(_))))
            .count() as u64;
        let edges = self
            .records
            .iter()
            .filter(|r| matches!(r.content, Some(Content::ControlEdgeDef(_))))
            .count() as u64;
        self.records.push(rec(Content::EndUnit(EndUnit {
            unit_id: UNIT.into(),
            inventory_status: "COMPLETE".into(),
            expected_source_decisions: decisions,
            emitted_source_decisions: decisions,
            emitted_ir_edges: edges,
        })));
        self.records
    }
}

fn decide(root: &Path, memory: bool, opaque: bool) -> io::Result<Vec<Record>> {
    let name = if opaque {
        "MemOpaque"
    } else if memory {
        "MemNoCall"
    } else {
        "Decide"
    };
    let mut f = Fixture::new(name, if memory { "memory.go" } else { "decide.go" });
    f.source(
        root,
        if memory {
            "9d76d23b9d1eb104e209a97fcfbc6b3e15856cf44b7395976cc68828b94ae887"
        } else {
            "39c3272b8bffb72c31fe2b395b2129f183760662b42a2a8d07aa44e08a9b6972"
        },
    )?;
    f.ty(BOOL, "BOOL", &[], &[]);
    f.ty(INT, "BITVECTOR", &[], &[]);
    if memory {
        f.ty("go:*int@amd64", "POINTER", &[INT], &[]);
    }
    let param_ty = if memory { "go:*int@amd64" } else { INT };
    f.ty("fixture:fn", "FUNCTION", &[param_ty], &[INT]);
    if opaque {
        f.ty("fixture:fn()", "FUNCTION", &[], &[]);
        f.ty("fixture:*fn()", "POINTER", &["fixture:fn()"], &[]);
    }
    let start = if opaque { 15 } else { 3 };
    let end = if opaque { 24 } else { 11 };
    f.function("fixture:fn", &["v0"], Some((start, 1, end, 2)));
    if opaque {
        f.symbol(
            "example.Hook",
            "GLOBAL",
            "fixture:*fn()",
            vec![f.span((13, 5, 13, 9))],
        );
        f.global("example.Hook", "fixture:*fn()");
    }
    f.value(
        "v0",
        param_ty,
        "PARAM",
        None,
        if memory { "p" } else { "x" },
        Some(0),
    );
    for (i, k) in [0, -1, -10, -2].iter().enumerate() {
        f.constant(i as u32, *k);
    }
    let (c1, c2) = if memory {
        f.value("v1", INT, "SSA", None, "", None);
        f.value("v2", BOOL, "SSA", None, "", None);
        if opaque {
            f.value("v3", "fixture:fn()", "SSA", None, "", None);
        }
        f.value(if opaque { "v4" } else { "v3" }, INT, "SSA", None, "", None);
        f.value(
            if opaque { "v5" } else { "v4" },
            BOOL,
            "SSA",
            None,
            "",
            None,
        );
        ("v2", if opaque { "v5" } else { "v4" })
    } else {
        f.value("v1", BOOL, "SSA", None, "", None);
        f.value("v2", BOOL, "SSA", None, "", None);
        ("v1", "v2")
    };
    f.blocks(5);
    let l1 = start + 1;
    let l2 = if opaque { 20 } else { 7 };
    let s1 = (l1, 5, l1, if memory { 11 } else { 10 });
    let s2 = (l2, 5, l2, if memory { 13 } else { 12 });
    if memory {
        f.inst(0, 0, "LOAD", "v1", &["v0"], None, None);
    }
    f.inst(
        0,
        u32::from(memory),
        "LT",
        c1,
        &[if memory { "v1" } else { "v0" }, "k0"],
        None,
        Some(s1),
    );
    let e1 = f.branch(0, 1 + u32::from(memory), c1, [1, 2], s1);
    f.inst(
        1,
        0,
        "RETURN",
        "",
        &["k1"],
        None,
        Some((l1 + 1, 3, l1 + 1, 12)),
    );
    if opaque {
        f.inst(2, 0, "LOAD", "v3", &["global:example.Hook"], None, None);
        f.inst(2, 1, "CALL", "", &["v3"], call("DYNAMIC"), None);
    }
    let offset = if opaque { 2 } else { 0 };
    if memory {
        f.inst(
            2,
            offset,
            "LOAD",
            if opaque { "v4" } else { "v3" },
            &["v0"],
            None,
            None,
        );
    }
    f.inst(
        2,
        offset + u32::from(memory),
        "LT",
        c2,
        &[
            if opaque {
                "v4"
            } else if memory {
                "v3"
            } else {
                "v0"
            },
            "k2",
        ],
        None,
        Some(s2),
    );
    let e2 = f.branch(2, offset + 1 + u32::from(memory), c2, [3, 4], s2);
    f.inst(
        3,
        0,
        "RETURN",
        "",
        &["k3"],
        None,
        Some((l2 + 1, 3, l2 + 1, 12)),
    );
    f.inst(
        4,
        0,
        "RETURN",
        "",
        &["k0"],
        None,
        Some((end - 1, 2, end - 1, 10)),
    );
    f.decision(0, "IF", s1, &["TRUE", "FALSE"], &e1, false);
    f.decision(1, "IF", s2, &["TRUE", "FALSE"], &e2, false);
    Ok(f.finish())
}

fn adjusted(root: &Path) -> io::Result<Vec<Record>> {
    let mut f = Fixture::new("Adjusted", "adjusted.go");
    f.source(
        root,
        "b2b2a9a1d4da9452dbe7b58484dfb316d479a39e09146e2357cbce746c112631",
    )?;
    f.ty(BOOL, "BOOL", &[], &[]);
    f.ty(INT, "BITVECTOR", &[], &[]);
    f.ty("fixture:fn", "FUNCTION", &[INT], &[INT]);
    f.function("fixture:fn", &["v0"], Some((3, 1, 9, 2)));
    f.value("v0", INT, "PARAM", None, "x", Some(0));
    for (i, k) in [0, 10, 3].iter().enumerate() {
        f.constant(i as u32, *k);
    }
    for (v, t) in [("v1", BOOL), ("v2", INT), ("v3", INT)] {
        f.value(v, t, "SSA", None, "", None);
    }
    f.blocks(3);
    f.inst(0, 0, "GT", "v1", &["v0", "k1"], None, Some((5, 5, 5, 11)));
    let edges = f.branch(0, 1, "v1", [1, 2], (5, 5, 5, 11));
    f.inst(1, 0, "JUMP", "", &[], None, None);
    f.edge(1, 0, 0, 2, "UNCONDITIONAL", "NEXT", "", &[]);
    let phi = OperationPayload {
        phi_inputs: vec![
            PhiInput {
                predecessor_block_id: f.id("b0"),
                value_id: f.id("k0"),
            },
            PhiInput {
                predecessor_block_id: f.id("b1"),
                value_id: f.id("k2"),
            },
        ],
        ..Default::default()
    };
    f.inst(2, 0, "PHI", "v2", &[], Some(phi), None);
    f.inst(2, 1, "SUB", "v3", &["v0", "v2"], binary("SUB"), None);
    f.inst(2, 2, "RETURN", "", &["v3"], None, Some((8, 2, 8, 14)));
    f.decision(0, "IF", (5, 5, 5, 11), &["TRUE", "FALSE"], &edges, false);
    Ok(f.finish())
}

fn check(root: &Path) -> io::Result<Vec<Record>> {
    let mut f = Fixture::new("Check", "check.go");
    f.source(
        root,
        "eb40f7f8e96a0bbe76aa9e25dfa239c5fd45f806a055acb61f6049b4bf13b4c0",
    )?;
    f.ty(BOOL, "BOOL", &[], &[]);
    f.ty(INT, "BITVECTOR", &[], &[]);
    f.ty("go:error", "INTERFACE", &[], &[]);
    f.ty("go:*error", "POINTER", &["go:error"], &[]);
    f.ty("fixture:fn", "FUNCTION", &[INT, BOOL], &["go:error"]);
    f.function("fixture:fn", &["v0", "v1"], Some((8, 1, 16, 2)));
    for r in &mut f.records {
        if let Some(Content::FunctionDef(d)) = &mut r.content {
            d.return_type_ids = vec!["go:error".into()];
        }
    }
    for (name, line) in [("ErrUnderage", 5), ("ErrDisabled", 6)] {
        let id = format!("example.{name}");
        f.symbol(
            &id,
            "GLOBAL",
            "go:*error",
            vec![f.span((line, 5, line, 16))],
        );
        f.global(&id, "go:*error");
    }
    f.value("v0", INT, "PARAM", None, "age", Some(0));
    f.value("v1", BOOL, "PARAM", None, "enabled", Some(1));
    f.constant(0, 18);
    f.value(
        "k1",
        "go:error",
        "CONST",
        Some(Representation::NullValue(true)),
        "",
        None,
    );
    for (id, t) in [("v2", BOOL), ("v3", "go:error"), ("v4", "go:error")] {
        f.value(id, t, "SSA", None, "", None);
    }
    f.blocks(5);
    f.inst(0, 0, "LT", "v2", &["v0", "k0"], None, Some((9, 5, 9, 13)));
    let e = f.branch(0, 1, "v2", [1, 2], (9, 5, 9, 13));
    f.decision(0, "IF", (9, 5, 9, 13), &["TRUE", "FALSE"], &e, false);
    f.inst(
        1,
        0,
        "LOAD",
        "v3",
        &["global:example.ErrUnderage"],
        None,
        None,
    );
    f.inst(1, 1, "RETURN", "", &["v3"], None, Some((10, 3, 10, 21)));
    let mut e = f.branch(2, 0, "v1", [3, 4], (12, 5, 12, 13));
    e.reverse();
    f.decision(1, "IF", (12, 5, 12, 13), &["TRUE", "FALSE"], &e, true);
    f.inst(3, 0, "RETURN", "", &["k1"], None, Some((15, 2, 15, 12)));
    f.inst(
        4,
        0,
        "LOAD",
        "v4",
        &["global:example.ErrDisabled"],
        None,
        None,
    );
    f.inst(4, 1, "RETURN", "", &["v4"], None, Some((13, 3, 13, 21)));
    Ok(f.finish())
}

fn switch3(root: &Path) -> io::Result<Vec<Record>> {
    let mut f = Fixture::new("DaySwitch", "switch_fixture.go");
    f.source(
        root,
        "1fec02c9436ca998a91a04fc4bce54129a283ced6e0ec5a81dc893f433e42867",
    )?;
    f.ty(INT, "BITVECTOR", &[], &[]);
    f.ty("fixture:fn", "FUNCTION", &[INT], &[INT]);
    f.function("fixture:fn", &["v0"], Some((3, 1, 12, 2)));
    f.value("v0", INT, "PARAM", None, "n", Some(0));
    for (i, k) in [1, 2, 3, 10, 20, 30].iter().enumerate() {
        f.constant(i as u32, *k);
    }
    f.blocks(4);
    f.inst(0, 0, "SWITCH", "", &["v0"], None, Some((4, 9, 4, 10)));
    let edges = vec![
        f.edge(0, 0, 0, 1, "SWITCH_CASE", "CASE:0", "", &["k0"]),
        f.edge(0, 0, 1, 2, "SWITCH_CASE", "CASE:1", "", &["k1", "k2"]),
        f.edge(0, 0, 2, 3, "SWITCH_DEFAULT", "DEFAULT", "", &[]),
    ];
    for (b, k) in [(1, "k3"), (2, "k4"), (3, "k5")] {
        f.inst(b, 0, "RETURN", "", &[k], None, None);
    }
    f.decision(
        0,
        "SWITCH",
        (4, 9, 4, 10),
        &["CASE:0", "CASE:1", "DEFAULT"],
        &edges,
        false,
    );
    Ok(f.finish())
}

fn invoke() -> Vec<Record> {
    let mut f = Fixture::new("Try", "");
    f.ty(INT, "BITVECTOR", &[], &[]);
    f.ty("fixture:fn()->int", "FUNCTION", &[], &[INT]);
    f.ty("fixture:fn()", "FUNCTION", &[], &[]);
    f.function("fixture:fn()->int", &[], None);
    f.symbol("example.risky", "FUNCTION", "fixture:fn()", vec![]);
    f.records.push(rec(Content::FunctionDef(FunctionDef {
        id: "example.risky".into(),
        symbol_id: "example.risky".into(),
        is_external: true,
        ..Default::default()
    })));
    f.global("example.risky", "fixture:fn()");
    f.constant(0, 0);
    f.constant(1, -1);
    f.blocks(3);
    f.inst(
        0,
        0,
        "INVOKE",
        "",
        &["global:example.risky"],
        call("STATIC"),
        None,
    );
    f.edge(0, 0, 0, 1, "NORMAL", "NORMAL", "", &[]);
    f.edge(0, 0, 1, 2, "EXCEPTION", "EXCEPTION", "", &[]);
    f.inst(1, 0, "RETURN", "", &["k0"], None, None);
    f.inst(2, 0, "RETURN", "", &["k1"], None, None);
    f.records.push(rec(Content::CallTargetDef(CallTargetDef {
        call_instruction_id: f.id("b0/i0"),
        target_function_id: "example.risky".into(),
        resolution_kind: "STATIC".into(),
    })));
    f.finish()
}

fn verify_source_spans(root: &Path, records: &[Record]) -> io::Result<()> {
    for record in records {
        let spans: Vec<&Span> = match &record.content {
            Some(Content::SymbolDef(d)) => d.spans.iter().collect(),
            Some(Content::InstructionDef(d)) => d.spans.iter().collect(),
            Some(Content::SourceDecisionDef(d)) => d.span.iter().collect(),
            _ => vec![],
        };
        for span in spans {
            let bytes = fs::read(root.join(&span.path))?;
            let lines: Vec<_> = bytes.split(|b| *b == b'\n').collect();
            let valid_position = |line: u32, column: u32| {
                line > 0
                    && column > 0
                    && lines
                        .get(line as usize - 1)
                        .is_some_and(|text| column as usize <= text.len() + 1)
            };
            if !valid_position(span.start_line, span.start_column)
                || !valid_position(span.end_line, span.end_column)
                || (span.start_line, span.start_column) > (span.end_line, span.end_column)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("fixture span outside source: {span:?}"),
                ));
            }
        }
    }
    Ok(())
}

fn write_records(path: &Path, records: &[Record]) -> io::Result<()> {
    let mut out = BufWriter::new(fs::File::create(path)?);
    for record in records {
        write_record(&mut out, record)?;
    }
    out.flush()
}
fn write_record(out: &mut impl Write, record: &Record) -> io::Result<()> {
    let bytes = record.encode_to_vec();
    let len = u32::try_from(bytes.len()).map_err(io::Error::other)?;
    out.write_all(&len.to_be_bytes())?;
    out.write_all(&bytes)
}

fn instruction<'a>(records: &'a mut [Record], id: &str) -> &'a mut InstructionDef {
    records
        .iter_mut()
        .find_map(|r| match &mut r.content {
            Some(Content::InstructionDef(d)) if d.id == id => Some(d),
            _ => None,
        })
        .expect("known fixture instruction")
}
fn append(records: &mut Vec<Record>, content: Content) {
    records.insert(records.len() - 1, rec(content));
}
fn bool_constant(records: &mut Vec<Record>) {
    append(
        records,
        Content::ValueDef(ValueDef {
            id: "example.Decide/k4".into(),
            function_id: "example.Decide".into(),
            type_id: BOOL.into(),
            role: "CONST".into(),
            literal: Some(LiteralValue {
                representation: Some(Representation::BoolValue(true)),
            }),
            ..Default::default()
        }),
    );
}
const NEGATIVES: &[&str] = &[
    "proto_decode",
    "msg_too_large",
    "header_missing",
    "record_order",
    "cir_major_unsupported",
    "end_unit_missing",
    "duplicate_id",
    "ordinal_duplicate",
    "param_order",
    "empty_id",
    "unknown_opcode",
    "unknown_kind",
    "dangling_ref",
    "arity_mismatch",
    "type_mismatch",
    "payload_missing",
    "payload_kind",
    "no_terminator",
    "multi_terminator",
    "terminator_not_last",
    "edge_source_mismatch",
    "edge_cross_function",
    "edge_shape",
    "entry_block",
    "ssa_multi_def",
    "ssa_dominance",
    "phi_pred_mismatch",
    "path_invalid",
    "span_invalid",
    "literal_width",
    "counter_mismatch",
    "source_hash_mismatch",
    "source_activation_invalid",
];
fn write_negatives(out: &Path, decide: &[Record], adjusted: &[Record]) -> io::Result<()> {
    for code in NEGATIVES {
        let base = if matches!(
            *code,
            "payload_missing" | "payload_kind" | "phi_pred_mismatch"
        ) {
            adjusted
        } else {
            decide
        };
        let mut records = base.to_vec();
        match *code {
            "proto_decode"|"msg_too_large"=>{},
            "header_missing"=>{records.remove(0);},
            "record_order"=>records.swap(1,2),
            "cir_major_unsupported"=>if let Some(Content::Header(d))=&mut records[0].content {d.cir_major=2;},
            "end_unit_missing"=>{records.pop();},
            "duplicate_id"|"ordinal_duplicate"=>{
                let mut block=records.iter().find_map(|r|match &r.content {Some(Content::BlockDef(d)) if d.ordinal==1=>Some(d.clone()),_=>None}).unwrap();
                if *code=="ordinal_duplicate" {block.id="example.Decide/b5".into();}
                append(&mut records,Content::BlockDef(block));
            },
            "param_order"|"empty_id"|"unknown_kind"|"literal_width"=>{
                for r in &mut records {if let Some(Content::ValueDef(d))=&mut r.content {
                    if d.id=="example.Decide/v0" {
                        match *code {
                            "param_order"=>d.param_index=Some(1),
                            "empty_id"=>d.id.clear(),
                            "unknown_kind"=>d.role="WEIRD".into(),
                            _=>{},
                        }
                    }
                    if *code=="literal_width" && d.id=="example.Decide/k0" {
                        d.literal=Some(LiteralValue {representation:Some(Representation::MachineBitsBe(vec![0;4]))});
                    }
                }}
            },
            "unknown_opcode"=>instruction(&mut records,"example.Decide/b0/i0").opcode="FROBNICATE".into(),
            "dangling_ref"=>instruction(&mut records,"example.Decide/b0/i0").operand_value_ids[1]="example.Decide/v99".into(),
            "arity_mismatch"=>{instruction(&mut records,"example.Decide/b0/i0").operand_value_ids.pop();},
            "type_mismatch"=>{bool_constant(&mut records); instruction(&mut records,"example.Decide/b0/i0").operand_value_ids[1]="example.Decide/k4".into();},
            "payload_missing"=>instruction(&mut records,"example.Adjusted/b2/i1").payload=None,
            "payload_kind"=>instruction(&mut records,"example.Adjusted/b2/i1").payload=call("STATIC"),
            "no_terminator"=>records.retain(|r|!matches!(&r.content,Some(Content::InstructionDef(d)) if d.id=="example.Decide/b1/i0")),
            "multi_terminator"=>{
                let mut d=instruction(&mut records,"example.Decide/b1/i0").clone();
                d.id="example.Decide/b1/i1".into(); d.ordinal=1;
                append(&mut records,Content::InstructionDef(d));
            },
            "terminator_not_last"=>{
                records.retain(|r|!matches!(&r.content,Some(Content::InstructionDef(d)) if d.id=="example.Decide/b0/i0") && !matches!(&r.content,Some(Content::ValueDef(d)) if d.id=="example.Decide/v1"));
                bool_constant(&mut records);
                let d=instruction(&mut records,"example.Decide/b0/i1");
                d.ordinal=0; d.operand_value_ids=vec!["example.Decide/k4".into()];
                for r in &mut records {if let Some(Content::ControlEdgeDef(d))=&mut r.content {if d.source_block_id=="example.Decide/b0" {d.guard_value_id="example.Decide/k4".into();}}}
                append(&mut records,Content::InstructionDef(InstructionDef {id:"example.Decide/b0/i2".into(),block_id:"example.Decide/b0".into(),ordinal:1,opcode:"OPAQUE_OP".into(),payload:payload(Details::Opaque(OpaqueSemantics{source_construct:"fixture".into(),reason:"negative".into(),..Default::default()})),..Default::default()}));
            },
            "edge_source_mismatch"=>for r in &mut records {if let Some(Content::ControlEdgeDef(d))=&mut r.content {if d.id=="example.Decide/b0/i1/e0" {d.source_block_id="example.Decide/b1".into();}}},
            "edge_cross_function"=>{
                append(&mut records,Content::SymbolDef(SymbolDef {id:"example.Other".into(),owner_id:"example".into(),name:"Other".into(),qualified_name:"example.Other".into(),kind:"FUNCTION".into(),type_id:"fixture:fn".into(),..Default::default()}));
                // Same valid signature; PARAM is independent of the mutated edge.
                append(&mut records,Content::FunctionDef(FunctionDef {id:"example.Other".into(),symbol_id:"example.Other".into(),parameter_value_ids:vec!["example.Other/v0".into()],return_type_ids:vec![INT.into()],entry_block_id:"example.Other/b0".into(),..Default::default()}));
                append(&mut records,Content::ValueDef(ValueDef{id:"example.Other/v0".into(),function_id:"example.Other".into(),type_id:INT.into(),role:"PARAM".into(),name:"x".into(),param_index:Some(0),..Default::default()}));
                append(&mut records,Content::BlockDef(BlockDef{id:"example.Other/b0".into(),function_id:"example.Other".into(),ordinal:0}));
                append(&mut records,Content::InstructionDef(InstructionDef{id:"example.Other/b0/i0".into(),block_id:"example.Other/b0".into(),opcode:"RETURN".into(),operand_value_ids:vec!["example.Other/v0".into()],is_terminator:true,..Default::default()}));
                for r in &mut records {if let Some(Content::ControlEdgeDef(d))=&mut r.content {if d.id=="example.Decide/b0/i1/e0" {d.target_block_id="example.Other/b0".into();}}}
            },
            "edge_shape"=>{
                let edge="example.Decide/b0/i1/e1";
                records.retain(|r|match &r.content {
                    Some(Content::ControlEdgeDef(d))=>d.id!=edge,
                    Some(Content::SourceIrLink(d))=>d.control_edge_id!=edge,
                    Some(Content::SourceOutcomeActivationCase(d))=>!d.ordered_control_edge_ids.iter().any(|id|id==edge),
                    _=>true,
                });
                for r in &mut records {match &mut r.content {
                    Some(Content::SourceDecisionDef(d)) if d.id=="example.Decide/sd0"=>d.outcomes[1].activation_mapping_complete=false,
                    Some(Content::EndUnit(d))=>d.emitted_ir_edges=3,
                    _=>{},
                }}
            },
            "entry_block"=>for r in &mut records {if let Some(Content::FunctionDef(d))=&mut r.content {d.entry_block_id.clear();}},
            "ssa_multi_def"=>{
                instruction(&mut records,"example.Decide/b2/i0").result_value_id="example.Decide/v1".into();
                records.retain(|r|!matches!(&r.content,Some(Content::ValueDef(d)) if d.id=="example.Decide/v2"));
                for r in &mut records {match &mut r.content {
                    Some(Content::InstructionDef(d))=>for v in &mut d.operand_value_ids {if v=="example.Decide/v2" {*v="example.Decide/v1".into();}},
                    Some(Content::ControlEdgeDef(d)) if d.guard_value_id=="example.Decide/v2"=>d.guard_value_id="example.Decide/v1".into(),
                    _=>{},
                }}
            },
            "ssa_dominance"=>{
                append(&mut records,Content::ValueDef(ValueDef{id:"example.Decide/v3".into(),function_id:"example.Decide".into(),type_id:INT.into(),role:"SSA".into(),..Default::default()}));
                let d=instruction(&mut records,"example.Decide/b1/i0"); d.id="example.Decide/b1/i1".into(); d.ordinal=1;
                append(&mut records,Content::InstructionDef(InstructionDef{id:"example.Decide/b1/i0".into(),block_id:"example.Decide/b1".into(),opcode:"NEG".into(),result_value_id:"example.Decide/v3".into(),operand_value_ids:vec!["example.Decide/v0".into()],payload:binary("NEG"),..Default::default()}));
                instruction(&mut records,"example.Decide/b3/i0").operand_value_ids=vec!["example.Decide/v3".into()];
            },
            "phi_pred_mismatch"=>{instruction(&mut records,"example.Adjusted/b2/i0").payload.as_mut().unwrap().phi_inputs.pop();},
            "path_invalid"=>for r in &mut records {match &mut r.content {
                Some(Content::SourceFileDef(d))=>d.path="../decide.go".into(),
                Some(Content::SymbolDef(d))=>for s in &mut d.spans {s.path="../decide.go".into();},
                Some(Content::InstructionDef(d))=>for s in &mut d.spans {s.path="../decide.go".into();},
                Some(Content::SourceDecisionDef(d))=>d.span.as_mut().unwrap().path="../decide.go".into(),
                _=>{},
            }},
            "span_invalid"=>{instruction(&mut records,"example.Decide/b0/i0").spans[0].end_line=3;},
            "counter_mismatch"=>for r in &mut records {if let Some(Content::EndUnit(d))=&mut r.content {d.emitted_ir_edges=5;}},
            "source_hash_mismatch"=>for r in &mut records {if let Some(Content::SourceFileDef(d))=&mut r.content {d.content_sha256="0".repeat(64);}},
            "source_activation_invalid"=>for r in &mut records {if let Some(Content::SourceOutcomeActivationCase(d))=&mut r.content {if d.source_outcome_id=="example.Decide/sd0/o0" {d.ordered_control_edge_ids.clear();}}},
            _=>unreachable!(),
        }
        let path = out.join(format!("bad_{code}.cir.pb"));
        write_records(&path, &records)?;
        if *code == "proto_decode" {
            let file = fs::OpenOptions::new().write(true).open(&path)?;
            file.set_len(file.metadata()?.len() - 3)?;
        } else if *code == "msg_too_large" {
            let mut file = fs::OpenOptions::new().write(true).open(&path)?;
            file.write_all(&[255; 4])?;
        }
    }
    Ok(())
}

// §17: 2,000 independent, well-typed linear CFGs × 100 = 200,000 instructions.
// Stream records rather than retaining the entire synthetic unit in a Vec.
fn write_big(path: &Path) -> io::Result<()> {
    let mut out = BufWriter::new(fs::File::create(path)?);
    let mut f = Fixture::new("Big", "");
    f.ty(INT, "BITVECTOR", &[], &[]);
    f.ty("fixture:big-fn", "FUNCTION", &[INT], &[INT]);
    f.symbol("example", "PACKAGE", "", vec![]);
    for r in &f.records {
        write_record(&mut out, r)?;
    }
    for n in 0..2_000 {
        let id = format!("example.Big{n}");
        write_record(
            &mut out,
            &rec(Content::SymbolDef(SymbolDef {
                id: id.clone(),
                owner_id: "example".into(),
                name: format!("Big{n}"),
                qualified_name: id.clone(),
                kind: "FUNCTION".into(),
                type_id: "fixture:big-fn".into(),
                ..Default::default()
            })),
        )?;
        write_record(
            &mut out,
            &rec(Content::FunctionDef(FunctionDef {
                id: id.clone(),
                symbol_id: id.clone(),
                parameter_value_ids: vec![format!("{id}/v0")],
                return_type_ids: vec![INT.into()],
                entry_block_id: format!("{id}/b0"),
                ..Default::default()
            })),
        )?;
        write_record(
            &mut out,
            &rec(Content::BlockDef(BlockDef {
                id: format!("{id}/b0"),
                function_id: id.clone(),
                ordinal: 0,
            })),
        )?;
        for v in 0..100 {
            write_record(
                &mut out,
                &rec(Content::ValueDef(ValueDef {
                    id: format!("{id}/v{v}"),
                    function_id: id.clone(),
                    type_id: INT.into(),
                    role: if v == 0 { "PARAM" } else { "SSA" }.into(),
                    name: if v == 0 { "x" } else { "" }.into(),
                    param_index: if v == 0 { Some(0) } else { None },
                    ..Default::default()
                })),
            )?;
        }
        for i in 0..100 {
            let ret = i == 99;
            write_record(
                &mut out,
                &rec(Content::InstructionDef(InstructionDef {
                    id: format!("{id}/b0/i{i}"),
                    block_id: format!("{id}/b0"),
                    ordinal: i,
                    opcode: if ret { "RETURN" } else { "NEG" }.into(),
                    result_value_id: if ret {
                        String::new()
                    } else {
                        format!("{id}/v{}", i + 1)
                    },
                    operand_value_ids: vec![format!("{id}/v{i}")],
                    payload: if ret { None } else { binary("NEG") },
                    is_terminator: ret,
                    ..Default::default()
                })),
            )?;
        }
    }
    write_record(
        &mut out,
        &rec(Content::EndUnit(EndUnit {
            unit_id: UNIT.into(),
            inventory_status: "COMPLETE".into(),
            ..Default::default()
        })),
    )?;
    out.flush()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut out = PathBuf::from("tests/fixtures/cir");
    let mut negative = false;
    let mut big = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = PathBuf::from(args.next().ok_or("--out needs a path")?),
            "--negative" => negative = true,
            "--big" => big = true,
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/go-branch-lab");
    fs::create_dir_all(&out)?;
    let fixtures = vec![
        ("decide", decide(&root, false, false)?),
        ("check", check(&root)?),
        ("adjusted", adjusted(&root)?),
        ("switch3", switch3(&root)?),
        ("invoke", invoke()),
        ("mem_nocall", decide(&root, true, false)?),
        ("mem_opaque", decide(&root, true, true)?),
    ];
    for (name, records) in &fixtures {
        verify_source_spans(&root, records)?;
        write_records(&out.join(format!("{name}.cir.pb")), records)?;
    }
    if negative {
        write_negatives(&out, &fixtures[0].1, &fixtures[2].1)?;
    }
    if big {
        write_big(&out.join("big.cir.pb"))?;
    }
    Ok(())
}
