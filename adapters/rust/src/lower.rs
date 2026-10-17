//! Pinned textual MIR reader and conservative CIR lowering.
//!
//! Every local is renamed at each MIR assignment. Block-entry PHIs refer to
//! predecessor exit versions, never to a mutable MIR place.
use crate::{
    compiler::Compiler,
    profile::Profile,
    source::{Function, Inventory},
};
use sha2::{Digest, Sha256};
use ske_cir::proto::{self as p, record::Content as C};
use std::collections::{BTreeMap, BTreeSet, HashMap};

fn record(content: C) -> p::Record {
    p::Record {
        content: Some(content),
    }
}
fn diagnostic(code: &str, impact: &str, message: impl Into<String>, fid: &str) -> p::Record {
    record(C::Diagnostic(p::Diagnostic {
        code: code.into(),
        severity: "WARNING".into(),
        impact: impact.into(),
        message: message.into(),
        affected_function_id: fid.into(),
        ..Default::default()
    }))
}

#[derive(Clone)]
struct Line {
    text: String,
    span: Option<p::Span>,
}
struct MirFunction {
    name: String,
    locals: BTreeMap<String, String>,
    params: Vec<String>,
    blocks: BTreeMap<String, Vec<Line>>,
}

fn split_top(s: &str) -> Vec<&str> {
    let mut depth = 0i32;
    let mut start = 0;
    let mut parts = vec![];
    let mut quoted = false;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if quoted {
            if !escaped && c == '"' {
                quoted = false;
            }
            escaped = !escaped && c == '\\';
            continue;
        }
        match c {
            '"' => quoted = true,
            '(' | '<' | '[' | '{' => depth += 1,
            ')' | '>' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        parts.push(s[start..].trim());
    }
    parts
}

fn parse_span(comment: &str, path: &str, text: &str) -> Option<p::Span> {
    let (_, location) = comment.split_once(" at ")?;
    // Parse from the right: absolute paths can contain ':'.
    let (start, end) = location.trim().rsplit_once(": ")?;
    let mut a = start.rsplitn(3, ':');
    let col: usize = a.next()?.parse().ok()?;
    let line: usize = a.next()?.parse().ok()?;
    let (end_line, end_col) = end.split_once(':')?;
    Some(p::Span {
        path: path.into(),
        start_line: line as u32,
        start_column: crate::source::byte_column(text, line, col.checked_sub(1)?),
        end_line: end_line.parse().ok()?,
        end_column: crate::source::byte_column(
            text,
            end_line.parse().ok()?,
            end_col.parse::<usize>().ok()?.checked_sub(1)?,
        ),
    })
}

fn parse(mir: &str, path: &str, source: &str) -> Result<Vec<MirFunction>, String> {
    let mut functions = vec![];
    let mut current: Option<MirFunction> = None;
    let mut block: Option<String> = None;
    for line in mir.lines() {
        if let Some(signature) = line.strip_prefix("fn ") {
            if current.is_some() {
                return Err("nested MIR function".into());
            }
            let (name, rest) = signature.split_once('(').ok_or("MIR function signature")?;
            let (args, _) = rest.rsplit_once(") -> ").ok_or("MIR return signature")?;
            let mut f = MirFunction {
                name: name.into(),
                locals: BTreeMap::new(),
                params: vec![],
                blocks: BTreeMap::new(),
            };
            for arg in split_top(args) {
                let (local, typ) = arg.split_once(": ").ok_or("MIR parameter")?;
                f.locals.insert(local.into(), typ.into());
                f.params.push(local.into());
            }
            current = Some(f);
            continue;
        }
        if line == "}" {
            if let Some(f) = current.take() {
                functions.push(f);
            }
            block = None;
            continue;
        }
        let Some(f) = current.as_mut() else {
            continue;
        };
        let (text, comment) = line.split_once("//").unwrap_or((line, ""));
        let text = text.trim();
        if let Some(decl) = text.strip_prefix("let ") {
            let decl = decl.strip_prefix("mut ").unwrap_or(decl);
            let (local, typ) = decl
                .trim_end_matches(';')
                .split_once(": ")
                .ok_or("MIR local")?;
            f.locals.insert(local.into(), typ.into());
        } else if text.starts_with("bb") && text.ends_with(": {") {
            let name = text.trim_end_matches(": {").trim_end_matches(" (cleanup)");
            block = Some(name.into());
            f.blocks.insert(name.into(), vec![]);
        } else if text == "}" {
            block = None;
        } else if let Some(b) = &block {
            if !text.is_empty() {
                f.blocks.get_mut(b).unwrap().push(Line {
                    text: text.into(),
                    span: parse_span(comment, path, source),
                });
            }
        }
    }
    if current.is_some() {
        return Err("truncated MIR function".into());
    }
    Ok(functions)
}

struct Emitter {
    records: Vec<p::Record>,
    types: BTreeMap<String, p::TypeDef>,
    value_types: BTreeMap<String, String>,
    pointer_width: u32,
}
impl Emitter {
    fn typ(&mut self, text: &str) -> String {
        let id = format!("rust:type:{text}");
        if self.types.contains_key(&id) {
            return id;
        }
        let mut t = p::TypeDef {
            id: id.clone(),
            canonical_descriptor: text.into(),
            kind: "OPAQUE".into(),
            ..Default::default()
        };
        if text == "bool" {
            t.kind = "BOOL".into();
        } else if text == "&str" {
            t.kind = "STRING".into();
        } else if text == "()" {
            t.kind = "VOID".into();
        } else if let Some(width) = text.strip_prefix('i').or_else(|| text.strip_prefix('u')) {
            if let Some(bits) = if width == "size" {
                Some(self.pointer_width)
            } else {
                width.parse::<u32>().ok()
            } {
                if [8, 16, 32, 64, 128].contains(&bits) {
                    t.kind = "BITVECTOR".into();
                    t.bit_width = bits;
                    t.is_signed = text.starts_with('i');
                }
            }
        }
        self.types.insert(id.clone(), t);
        id
    }
    fn value(
        &mut self,
        id: String,
        fid: &str,
        typ: String,
        role: &str,
        literal: Option<p::LiteralValue>,
    ) -> String {
        self.value_types.insert(id.clone(), typ.clone());
        self.records.push(record(C::ValueDef(p::ValueDef {
            id: id.clone(),
            function_id: fid.into(),
            type_id: typ,
            role: role.into(),
            literal,
            ..Default::default()
        })));
        id
    }
    fn constant(&mut self, fid: &str, text: &str) -> Result<String, String> {
        use p::literal_value::Representation as R;
        let (typ, literal) = if text == "true" || text == "false" {
            (self.typ("bool"), R::BoolValue(text == "true"))
        } else if text.starts_with('"') {
            let s: String = serde_json::from_str(text)
                .map_err(|_| format!("unsupported string literal {text}"))?;
            (self.typ("&str"), R::StringBytes(s.into_bytes()))
        } else {
            let (n, t) = text
                .rsplit_once('_')
                .ok_or_else(|| format!("unsupported MIR constant {text}"))?;
            let typ = self.typ(t);
            let def = &self.types[&typ];
            if def.kind != "BITVECTOR" {
                return Err(format!("unsupported MIR scalar {text}"));
            }
            let bits = if n.starts_with('-') {
                n.parse::<i128>().map_err(|_| "integer literal")? as u128
            } else {
                n.parse::<u128>().map_err(|_| "integer literal")?
            };
            let bytes = bits.to_be_bytes()[16 - (def.bit_width as usize).div_ceil(8)..].to_vec();
            (typ, R::MachineBitsBe(bytes))
        };
        let id = format!("{fid}/const:{}", hex_digest(text.as_bytes()));
        if !self.value_types.contains_key(&id) {
            self.value(
                id.clone(),
                fid,
                typ,
                "CONST",
                Some(p::LiteralValue {
                    representation: Some(literal),
                }),
            );
        }
        Ok(id)
    }
    fn operand(
        &mut self,
        fid: &str,
        text: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<String, String> {
        if let Some(c) = text.strip_prefix("const ") {
            self.constant(fid, c)
        } else if let Some(l) = text
            .strip_prefix("copy ")
            .or_else(|| text.strip_prefix("move "))
        {
            env.get(l)
                .cloned()
                .ok_or_else(|| format!("unsupported MIR place {l}"))
        } else {
            Err(format!("unsupported MIR operand {text}"))
        }
    }
}

pub fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn destination(text: &str) -> Option<&str> {
    let (lhs, _) = text.split_once(" = ")?;
    (lhs.starts_with('_') && lhs[1..].chars().all(|c| c.is_ascii_digit())).then_some(lhs)
}
fn targets(text: &str) -> Vec<String> {
    let mut result = vec![];
    let mut rest = text;
    while let Some((_, after)) = rest.split_once("bb") {
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            result.push(format!("bb{digits}"));
        }
        rest = after;
    }
    result
}

pub fn extract(
    compiler: &Compiler,
    profile: &Profile,
    inventory: &Inventory,
    mir: &str,
    text: &str,
) -> Result<Vec<p::Record>, String> {
    let parsed = parse(mir, &profile.entry, text)?;
    let names: BTreeSet<_> = parsed.iter().map(|f| f.name.as_str()).collect();
    let expected_names: BTreeSet<_> = inventory
        .functions
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    if names != expected_names {
        return Err(
            "MIR/source function inventory differs (implicit functions, cfg, closures, etc.)"
                .into(),
        );
    }
    // Target-dependent usize is only accepted for the host until target layout
    // querying is implemented. Never guess from a target-triple substring.
    if profile.target.is_some() {
        return Err("cross-target layout is not yet supported".into());
    }
    let mut e = Emitter {
        records: vec![],
        types: BTreeMap::new(),
        value_types: BTreeMap::new(),
        pointer_width: usize::BITS,
    };
    let fingerprint = hex_digest(serde_json::to_string(profile).unwrap().as_bytes());
    let unit = format!("rust:{}", profile.entry);
    e.records.push(record(C::Header(p::Header {
        cir_major: 1,
        cir_minor: 3,
        adapter_name: "ske-adapter-rust".into(),
        adapter_version: "0.6.0".into(),
        language: "rust".into(),
        toolchain_version: compiler.version.clone(),
        build_fingerprint: fingerprint,
        semantic_profile: "rust-mir-2026-10-15-unoptimized-unwind-partial".into(),
        capability_flags: vec![
            "SOURCE_AST".into(),
            "MIR_CFG".into(),
            "MIR_SOURCE_SPANS".into(),
            "RUST_SUBSET_PARTIAL".into(),
        ],
    })));
    e.records.push(record(C::UnitDef(p::UnitDef {
        unit_id: unit.clone(),
        unit_key: unit.clone(),
        language: "rust".into(),
    })));
    e.records.push(record(C::SourceFileDef(p::SourceFileDef {
        path: profile.entry.clone(),
        language: "rust".into(),
        content_sha256: hex_digest(text.as_bytes()),
    })));
    for f in &parsed {
        let source = inventory
            .functions
            .iter()
            .find(|s| s.name == f.name)
            .unwrap();
        lower_function(&mut e, f, source)?;
    }
    let edges = e
        .records
        .iter()
        .filter(|r| matches!(r.content, Some(C::ControlEdgeDef(_))))
        .count() as u64;
    let decisions = e
        .records
        .iter()
        .filter(|r| matches!(r.content, Some(C::SourceDecisionDef(_))))
        .count() as u64;
    // This pilot does not yet inventory every implicit panic/drop site; global
    // completeness is deliberately withheld even when the scalar CFG is exact.
    e.records.push(diagnostic("RUST_INVENTORY_PARTIAL", "INVENTORY",
        "implicit panic/Drop, trait dispatch and aggregate semantics are not fully inventoried; no closed-world/completeness claim", ""));
    let type_records = e
        .types
        .into_values()
        .map(|t| record(C::TypeDef(t)))
        .collect::<Vec<_>>();
    e.records.splice(3..3, type_records);
    e.records.push(record(C::EndUnit(p::EndUnit {
        unit_id: unit,
        inventory_status: "PARTIAL".into(),
        expected_source_decisions: inventory.expected,
        emitted_source_decisions: decisions,
        emitted_ir_edges: edges,
    })));
    validate_records(&e.records)?;
    Ok(e.records)
}

fn lower_function(e: &mut Emitter, f: &MirFunction, source: &Function) -> Result<(), String> {
    let fid = format!("rust:fn:{}", f.name);
    let sym = format!("{fid}/symbol");
    let bid = |b: &str| format!("{fid}/{b}");
    if !f.blocks.contains_key("bb0") || !f.locals.contains_key("_0") {
        return Err("MIR body missing entry/return place".into());
    }
    let mut local_types = BTreeMap::new();
    for (l, t) in &f.locals {
        local_types.insert(l.clone(), e.typ(t));
    }
    let returns = if f.locals["_0"] == "()" {
        vec![]
    } else {
        vec![local_types["_0"].clone()]
    };
    let signature = format!("{fid}/signature");
    e.types.insert(
        signature.clone(),
        p::TypeDef {
            id: signature.clone(),
            kind: "FUNCTION".into(),
            canonical_descriptor: signature.clone(),
            param_type_ids: f.params.iter().map(|l| local_types[l].clone()).collect(),
            result_type_ids: returns.clone(),
            ..Default::default()
        },
    );
    e.records.push(record(C::SymbolDef(p::SymbolDef {
        id: sym.clone(),
        name: f.name.clone(),
        qualified_name: f.name.clone(),
        kind: "FUNCTION".into(),
        type_id: signature,
        spans: vec![source.span.clone()],
        ..Default::default()
    })));
    let params: Vec<_> = f
        .params
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let id = format!("{fid}/param:{i}");
            e.value_types.insert(id.clone(), local_types[l].clone());
            e.records.push(record(C::ValueDef(p::ValueDef {
                id: id.clone(),
                function_id: fid.clone(),
                type_id: local_types[l].clone(),
                role: "PARAM".into(),
                param_index: Some(i as u32),
                name: source.parameters.get(i).cloned().unwrap_or_default(),
                ..Default::default()
            })));
            id
        })
        .collect();
    e.records.push(record(C::FunctionDef(p::FunctionDef {
        id: fid.clone(),
        symbol_id: sym,
        parameter_value_ids: params.clone(),
        return_type_ids: returns.clone(),
        entry_block_id: bid("bb0"),
        is_external: false,
    })));
    let mut preds: BTreeMap<String, BTreeSet<String>> = f
        .blocks
        .keys()
        .map(|b| (b.clone(), BTreeSet::new()))
        .collect();
    let mut exits: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (b, lines) in &f.blocks {
        for target in lines.iter().flat_map(|l| targets(&l.text)) {
            preds
                .get_mut(&target)
                .ok_or("MIR unknown block target")?
                .insert(b.clone());
        }
        let mut env: BTreeMap<_, _> = f
            .locals
            .keys()
            .map(|l| {
                let val = if b == "bb0" {
                    f.params
                        .iter()
                        .position(|p| p == l)
                        .map(|i| params[i].clone())
                } else {
                    None
                };
                (
                    l.clone(),
                    val.unwrap_or_else(|| format!("{fid}/{b}/in:{l}")),
                )
            })
            .collect();
        for (i, l) in lines.iter().enumerate() {
            if let Some(dest) = destination(&l.text) {
                env.insert(dest.into(), format!("{fid}/{b}/v:{i}"));
            }
        }
        exits.insert(b.clone(), env);
    }
    let mut branch_edges: Vec<(p::Span, String, String)> = vec![];
    for (ordinal, (b, lines)) in f.blocks.iter().enumerate() {
        e.records.push(record(C::BlockDef(p::BlockDef {
            id: bid(b),
            function_id: fid.clone(),
            ordinal: ordinal as u32,
        })));
        let mut env = BTreeMap::new();
        let mut ins_ordinal = 0;
        for local in f.locals.keys() {
            let param = if b == "bb0" {
                f.params.iter().position(|p| p == local)
            } else {
                None
            };
            if let Some(i) = param {
                env.insert(local.clone(), params[i].clone());
                continue;
            }
            let v = e.value(
                format!("{fid}/{b}/in:{local}"),
                &fid,
                local_types[local].clone(),
                "SSA",
                None,
            );
            let inputs: Vec<_> = preds[b]
                .iter()
                .map(|pred| p::PhiInput {
                    predecessor_block_id: bid(pred),
                    value_id: exits[pred][local].clone(),
                })
                .collect();
            let (opcode, payload) = if inputs.is_empty() {
                ("UNDEF", None)
            } else {
                (
                    "PHI",
                    Some(p::OperationPayload {
                        phi_inputs: inputs,
                        ..Default::default()
                    }),
                )
            };
            e.records.push(record(C::InstructionDef(p::InstructionDef {
                id: format!("{fid}/{b}/init:{local}"),
                block_id: bid(b),
                ordinal: ins_ordinal,
                opcode: opcode.into(),
                result_value_id: v.clone(),
                payload,
                ..Default::default()
            })));
            ins_ordinal += 1;
            env.insert(local.clone(), v);
        }
        let mut terminated = false;
        for (i, line) in lines.iter().enumerate() {
            let text = line.text.trim_end_matches(';');
            if text.starts_with("StorageLive(") || text.starts_with("StorageDead(") || text == "nop"
            {
                continue;
            }
            let iid = format!("{fid}/{b}/i:{i}");
            let mut ins = p::InstructionDef {
                id: iid.clone(),
                block_id: bid(b),
                ordinal: ins_ordinal,
                spans: line.span.iter().cloned().collect(),
                ..Default::default()
            };
            ins_ordinal += 1;
            let dest = destination(text);
            if let Some(d) = dest {
                ins.result_value_id = e.value(
                    format!("{fid}/{b}/v:{i}"),
                    &fid,
                    local_types[d].clone(),
                    "SSA",
                    None,
                );
            }
            let rhs = text.split_once(" = ").map(|(_, r)| r).unwrap_or(text);
            let mut edges = vec![];
            if let Some(body) = rhs.strip_prefix("switchInt(") {
                let (arg, choices) = body.split_once(") -> [").ok_or("MIR switch")?;
                let operand = e.operand(&fid, arg, &env)?;
                let typ = e.value_types[&operand].clone();
                let boolean = e.types[&typ].kind == "BOOL";
                ins.opcode = if boolean {
                    "BRANCH".into()
                } else {
                    "SWITCH".into()
                };
                ins.operand_value_ids = vec![operand.clone()];
                ins.is_terminator = true;
                let mut choices_list = split_top(choices.trim_end_matches(']'));
                if boolean {
                    choices_list.reverse();
                }
                for (n, choice) in choices_list.iter().enumerate() {
                    let (case, target) = choice.split_once(": ").ok_or("MIR switch target")?;
                    let mut edge = p::ControlEdgeDef {
                        id: format!("{iid}/e:{n}"),
                        source_block_id: bid(b),
                        target_block_id: bid(target),
                        terminator_instruction_id: iid.clone(),
                        outcome_ordinal: n as u32,
                        ..Default::default()
                    };
                    if boolean {
                        if choices.split(',').count() != 2 || !matches!(case, "0" | "otherwise") {
                            return Err("noncanonical boolean switch".into());
                        }
                        edge.edge_kind = if case == "0" {
                            "BRANCH_FALSE"
                        } else {
                            "BRANCH_TRUE"
                        }
                        .into();
                        edge.outcome_label = if case == "0" { "FALSE" } else { "TRUE" }.into();
                        edge.guard_value_id = operand.clone();
                        edge.guard_negated = case == "0";
                        if let Some(span) = &line.span {
                            branch_edges.push((
                                span.clone(),
                                edge.outcome_label.clone(),
                                edge.id.clone(),
                            ));
                        }
                    } else if case == "otherwise" {
                        edge.edge_kind = "SWITCH_DEFAULT".into();
                        edge.outcome_label = "DEFAULT".into();
                    } else {
                        edge.edge_kind = "SWITCH_CASE".into();
                        edge.outcome_label = format!("CASE:{n}");
                        let rust_type =
                            &f.locals[arg.trim_start_matches("copy ").trim_start_matches("move ")];
                        edge.case_value_ids
                            .push(e.constant(&fid, &format!("{case}_{rust_type}"))?);
                    }
                    edges.push(edge);
                }
            } else if rhs.starts_with("goto -> ") {
                ins.opcode = "JUMP".into();
                ins.is_terminator = true;
                edges.push(p::ControlEdgeDef {
                    id: format!("{iid}/e:0"),
                    source_block_id: bid(b),
                    target_block_id: bid(rhs.trim_start_matches("goto -> ")),
                    terminator_instruction_id: iid.clone(),
                    edge_kind: "UNCONDITIONAL".into(),
                    outcome_label: "NEXT".into(),
                    ..Default::default()
                });
            } else if rhs == "return" {
                ins.opcode = "RETURN".into();
                ins.is_terminator = true;
                if !returns.is_empty() {
                    ins.operand_value_ids.push(env["_0"].clone());
                }
            } else if rhs == "unreachable" {
                ins.opcode = "UNREACHABLE".into();
                ins.is_terminator = true;
            } else if rhs.contains(" -> [") {
                if dest.is_none() || rhs.starts_with("assert(") || rhs.starts_with("drop(") {
                    return Err(format!(
                        "unsupported MIR assert/drop terminator in {}: {text}",
                        f.name
                    ));
                }
                let (call, flow) = rhs.split_once(" -> [").ok_or("MIR call")?;
                let (callee, args) = call.rsplit_once('(').ok_or("MIR call arguments")?;
                let args = args.strip_suffix(')').ok_or("MIR call closing paren")?;
                let arguments = split_top(args)
                    .into_iter()
                    .map(|a| e.operand(&fid, a, &env))
                    .collect::<Result<Vec<_>, _>>()?;
                let signature = format!("{iid}/callee-type");
                let return_type = e.value_types[&ins.result_value_id].clone();
                e.types.insert(
                    signature.clone(),
                    p::TypeDef {
                        id: signature.clone(),
                        kind: "FUNCTION".into(),
                        canonical_descriptor: callee.into(),
                        param_type_ids: arguments
                            .iter()
                            .map(|a| e.value_types[a].clone())
                            .collect(),
                        result_type_ids: vec![return_type],
                        ..Default::default()
                    },
                );
                let callee_value = e.value(format!("{iid}/callee"), &fid, signature, "SSA", None);
                e.records.push(record(C::InstructionDef(p::InstructionDef {
                    id: format!("{iid}/callee-unknown"),
                    block_id: bid(b),
                    ordinal: ins.ordinal,
                    opcode: "UNDEF".into(),
                    result_value_id: callee_value.clone(),
                    ..Default::default()
                })));
                ins.ordinal += 1;
                ins_ordinal += 1;
                ins.opcode = "INVOKE".into();
                ins.is_terminator = true;
                ins.operand_value_ids = std::iter::once(callee_value).chain(arguments).collect();
                ins.payload = Some(p::OperationPayload {
                    details: Some(p::operation_payload::Details::Call(p::CallSemantics {
                        dispatch_kind: "UNKNOWN".into(),
                        may_raise: Some(true),
                        may_mutate_memory: Some(true),
                        world_closed: false,
                        ..Default::default()
                    })),
                    ..Default::default()
                });
                let flow = flow.trim_end_matches(']');
                let (normal, unwind) = flow.split_once(", unwind ").ok_or("MIR call unwind")?;
                let normal = normal
                    .strip_prefix("return: ")
                    .ok_or("MIR call normal target")?;
                let exception = if unwind == "continue" {
                    format!("{fid}/unwind")
                } else if let Some(target) = unwind.strip_prefix(": ") {
                    bid(target)
                } else {
                    return Err(format!("unsupported unwind action {unwind}"));
                };
                edges.push(p::ControlEdgeDef {
                    id: format!("{iid}/e:0"),
                    source_block_id: bid(b),
                    target_block_id: bid(normal),
                    terminator_instruction_id: iid.clone(),
                    outcome_ordinal: 0,
                    edge_kind: "NORMAL".into(),
                    outcome_label: "NORMAL".into(),
                    ..Default::default()
                });
                edges.push(p::ControlEdgeDef {
                    id: format!("{iid}/e:1"),
                    source_block_id: bid(b),
                    target_block_id: exception,
                    terminator_instruction_id: iid.clone(),
                    outcome_ordinal: 1,
                    edge_kind: "EXCEPTION".into(),
                    outcome_label: "EXCEPTION".into(),
                    exception_catch_all: true,
                    ..Default::default()
                });
                e.records.push(diagnostic(
                    "CALL_TARGET_UNRESOLVED",
                    "CALLGRAPH",
                    callee,
                    &fid,
                ));
                e.records.push(diagnostic("RUST_CALL_EFFECTS_PARTIAL", "EFFECTS",
                    "external calls preserve normal/unwind paths but return values and memory effects remain unknown", &fid));
            } else if dest.is_some() {
                if let Ok(v) = e.operand(&fid, rhs, &env) {
                    ins.opcode = "COPY".into();
                    ins.operand_value_ids = vec![v];
                } else if let Some((op, args)) = rhs.split_once('(') {
                    let opcode = match op {
                        "Lt" => Some("LT"),
                        "Le" => Some("LE"),
                        "Gt" => Some("GT"),
                        "Ge" => Some("GE"),
                        "Eq" => Some("EQ"),
                        "Ne" => Some("NE"),
                        _ => None,
                    };
                    if let Some(opcode) = opcode {
                        ins.opcode = opcode.into();
                        ins.operand_value_ids = split_top(args.trim_end_matches(')'))
                            .into_iter()
                            .map(|a| e.operand(&fid, a, &env))
                            .collect::<Result<_, _>>()?;
                    } else {
                        opaque(e, &fid, &mut ins, text);
                    }
                } else {
                    opaque(e, &fid, &mut ins, text);
                }
            } else {
                return Err(format!("unrecognized MIR statement {text}"));
            }
            if let Some(d) = dest {
                env.insert(d.into(), ins.result_value_id.clone());
            }
            terminated = ins.is_terminator;
            e.records.push(record(C::InstructionDef(ins)));
            e.records.extend(
                edges
                    .into_iter()
                    .map(|edge| record(C::ControlEdgeDef(edge))),
            );
        }
        if !terminated {
            return Err(format!("MIR block {b} has no recognized terminator"));
        }
    }
    if f.blocks
        .values()
        .flatten()
        .any(|l| l.text.contains(", unwind continue]"))
    {
        let block = format!("{fid}/unwind");
        let typ = e.typ("rust-unwind-payload");
        let value = e.value(format!("{fid}/unwind/value"), &fid, typ, "SSA", None);
        e.records.push(record(C::BlockDef(p::BlockDef {
            id: block.clone(),
            function_id: fid.clone(),
            ordinal: f.blocks.len() as u32,
        })));
        e.records.push(record(C::InstructionDef(p::InstructionDef {
            id: format!("{fid}/unwind/unknown"),
            block_id: block.clone(),
            ordinal: 0,
            opcode: "UNDEF".into(),
            result_value_id: value.clone(),
            ..Default::default()
        })));
        e.records.push(record(C::InstructionDef(p::InstructionDef {
            id: format!("{fid}/unwind/resume"),
            block_id: block,
            ordinal: 1,
            opcode: "THROW".into(),
            operand_value_ids: vec![value],
            is_terminator: true,
            ..Default::default()
        })));
    }
    for source_decision in &source.decisions {
        let mut decision = source_decision.clone();
        if decision.decision_kind == "IF" && source.direct_ifs.contains(&decision.id) {
            let matches: Vec<_> = branch_edges
                .iter()
                .filter(|(s, _, _)| Some(s) == decision.span.as_ref())
                .collect();
            if matches.len() == 2 {
                for outcome in &mut decision.outcomes {
                    if let Some((_, _, edge)) =
                        matches.iter().find(|(_, label, _)| label == &outcome.label)
                    {
                        e.records.push(record(C::SourceIrLink(p::SourceIrLinkDef {
                            source_outcome_id: outcome.id.clone(),
                            control_edge_id: edge.clone(),
                            mapping_kind: "DIRECT".into(),
                            quality: "EXACT".into(),
                            inverted: false,
                        })));
                        e.records.push(record(C::SourceOutcomeActivationCase(
                            p::SourceOutcomeActivationCaseDef {
                                source_outcome_id: outcome.id.clone(),
                                case_ordinal: 0,
                                ordered_control_edge_ids: vec![edge.clone()],
                                quality: "EXACT".into(),
                            },
                        )));
                        outcome.activation_mapping_complete = true;
                    }
                }
            }
        }
        if decision
            .outcomes
            .iter()
            .any(|o| !o.activation_mapping_complete)
        {
            e.records.push(diagnostic(
                "RUST_SOURCE_MAPPING_PARTIAL",
                "INVENTORY",
                format!(
                    "no proven complete MIR activation mapping for {}",
                    decision.id
                ),
                &fid,
            ));
        }
        e.records.push(record(C::SourceDecisionDef(decision)));
    }
    Ok(())
}

fn opaque(e: &mut Emitter, fid: &str, ins: &mut p::InstructionDef, text: &str) {
    ins.opcode = "OPAQUE_OP".into();
    ins.payload = Some(p::OperationPayload {
        details: Some(p::operation_payload::Details::Opaque(p::OpaqueSemantics {
            source_construct: text.into(),
            reason: "unmodeled MIR rvalue".into(),
            may_mutate_memory: Some(true),
            may_exit_abnormally: Some(true),
        })),
        ..Default::default()
    });
    e.records
        .push(diagnostic("RUST_MIR_RVALUE_PARTIAL", "SYMBOLIC", text, fid));
}

/// Catch a changed textual MIR shape before publishing any records. The importer
/// still performs the authoritative relational/dominance/unit validation.
fn validate_records(records: &[p::Record]) -> Result<(), String> {
    let types: HashMap<_, _> = records
        .iter()
        .filter_map(|r| match &r.content {
            Some(C::TypeDef(t)) => Some((t.id.clone(), t.clone())),
            _ => None,
        })
        .collect();
    let values: HashMap<_, _> = records
        .iter()
        .filter_map(|r| match &r.content {
            Some(C::ValueDef(v)) => Some((v.id.clone(), v.clone())),
            _ => None,
        })
        .collect();
    for t in types.values() {
        ske_cir::validate_type(t).map_err(|e| e.to_string())?;
    }
    for v in values.values() {
        ske_cir::validate_value(v, &types).map_err(|e| e.to_string())?;
    }
    for r in records {
        if let Some(C::InstructionDef(i)) = &r.content {
            ske_cir::validate_instruction(i, &types, &values)
                .map_err(|e| format!("{}: {e}", i.id))?;
            let mut edges: Vec<_> = records
                .iter()
                .filter_map(|r| match &r.content {
                    Some(C::ControlEdgeDef(e)) if e.terminator_instruction_id == i.id => {
                        Some(e.clone())
                    }
                    _ => None,
                })
                .collect();
            edges.sort_by_key(|e| e.outcome_ordinal);
            ske_cir::validate_edges(i, &edges, &values).map_err(|e| format!("{}: {e}", i.id))?;
        }
    }
    Ok(())
}
