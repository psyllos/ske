use crate::proto as proto_alias;
use crate::{proto::*, registry};
use proto_alias::{literal_value::Representation as L, operation_payload::Details as D};
use ske_common::{Error, Result};
use std::collections::{HashMap, HashSet};
fn check(ok: bool, code: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(Error::new(code, "CIR local contract violation"))
    }
}
fn get<'a, T>(map: &'a HashMap<String, T>, id: &str) -> Result<&'a T> {
    map.get(id).ok_or_else(|| Error::new("E_DANGLING_REF", id))
}
pub fn validate_type(t: &TypeDef) -> Result<()> {
    let k = t.kind.as_str();
    check(
        matches!(
            k,
            "VOID"
                | "BOOL"
                | "BITVECTOR"
                | "IEEE_FLOAT"
                | "CHAR"
                | "STRING"
                | "POINTER"
                | "REFERENCE"
                | "ARRAY"
                | "SLICE"
                | "TUPLE"
                | "STRUCT"
                | "ENUM"
                | "INTERFACE"
                | "FUNCTION"
                | "OPAQUE"
        ),
        "E_UNKNOWN_KIND",
    )?;
    check(!t.id.is_empty(), "E_EMPTY_ID")?;
    check(
        match k {
            "BITVECTOR" => t.bit_width >= 1,
            "IEEE_FLOAT" => [16, 32, 64, 128].contains(&t.bit_width),
            _ => t.bit_width == 0,
        },
        "E_TYPE_MISMATCH",
    )?;
    check(
        (k == "BITVECTOR" || !t.is_signed) && (matches!(k, "POINTER" | "REFERENCE") || !t.nullable),
        "E_TYPE_MISMATCH",
    )?;
    check(
        match k {
            "POINTER" | "REFERENCE" | "ARRAY" | "SLICE" => t.param_type_ids.len() == 1,
            "TUPLE" | "STRUCT" | "FUNCTION" => true,
            _ => t.param_type_ids.is_empty(),
        },
        "E_TYPE_MISMATCH",
    )?;
    check(
        (k == "FUNCTION" || t.result_type_ids.is_empty())
            && (matches!(k, "STRUCT" | "ENUM" | "INTERFACE") || t.component_names.is_empty()),
        "E_TYPE_MISMATCH",
    )?;
    check(
        k != "STRUCT" || t.component_names.len() == t.param_type_ids.len(),
        "E_TYPE_MISMATCH",
    )?;
    check(
        if k == "ARRAY" {
            t.extra.is_empty() || t.extra.len() == 8
        } else {
            t.extra.is_empty()
        },
        "E_TYPE_MISMATCH",
    )?;
    check(
        k != "OPAQUE" || !t.canonical_descriptor.is_empty(),
        "E_TYPE_MISMATCH",
    )
}
pub fn validate_literal(l: &LiteralValue, t: &TypeDef) -> Result<()> {
    let ok = match (&l.representation, t.kind.as_str()) {
        (Some(L::BoolValue(_)), "BOOL") | (Some(L::StringBytes(_)), "STRING") => true,
        // Descriptors are adapter-supplied strings, not a nilability witness.
        (Some(L::NullValue(true)), "POINTER" | "REFERENCE" | "INTERFACE") => true,
        (Some(L::MachineBitsBe(b)), "BITVECTOR" | "IEEE_FLOAT") => {
            b.len() == t.bit_width.div_ceil(8) as usize
                && (t.bit_width.is_multiple_of(8)
                    || b.first()
                        .is_some_and(|x| (*x as u32) < (1 << (t.bit_width % 8))))
        }
        _ => false,
    };
    check(ok, "E_LITERAL_WIDTH")
}
pub fn validate_value(v: &ValueDef, types: &HashMap<String, TypeDef>) -> Result<()> {
    check(!v.id.is_empty(), "E_EMPTY_ID")?;
    check(
        matches!(
            v.role.as_str(),
            "PARAM" | "CONST" | "SSA" | "GLOBAL" | "MEMORY"
        ),
        "E_UNKNOWN_KIND",
    )?;
    let t = get(types, &v.type_id)?;
    check(
        (v.role == "PARAM") == v.param_index.is_some(),
        "E_PARAM_ORDER",
    )?;
    check(
        (v.role == "CONST") == v.literal.is_some()
            && (v.role == "GLOBAL") == !v.symbol_id.is_empty(),
        "E_TYPE_MISMATCH",
    )?;
    if let Some(l) = &v.literal {
        validate_literal(l, t)?;
    }
    if v.role == "GLOBAL" {
        check(
            matches!(t.kind.as_str(), "POINTER" | "FUNCTION"),
            "E_TYPE_MISMATCH",
        )?;
    }
    Ok(())
}
pub fn validate_instruction(
    i: &InstructionDef,
    types: &HashMap<String, TypeDef>,
    values: &HashMap<String, ValueDef>,
) -> Result<()> {
    let op =
        registry::lookup(&i.opcode).ok_or_else(|| Error::new("E_UNKNOWN_OPCODE", &i.opcode))?;
    check(
        (op.min_operands..=op.max_operands).contains(&i.operand_value_ids.len()),
        "E_ARITY_MISMATCH",
    )?;
    check(op.terminator == i.is_terminator, "E_TYPE_MISMATCH")?;
    check(
        match op.result {
            0 => i.result_value_id.is_empty(),
            1 => !i.result_value_id.is_empty(),
            _ => true,
        },
        "E_TYPE_MISMATCH",
    )?;
    let payload = i.payload.as_ref();
    let actual = match payload.and_then(|p| p.details.as_ref()) {
        Some(D::Binary(_)) => "binary",
        Some(D::Call(_)) => "call",
        Some(D::Memory(_)) => "memory",
        Some(D::Opaque(_)) => "opaque",
        Some(D::Field(_)) => "field",
        Some(D::TypeTest(_)) => "type_test",
        None => "",
    };
    if !matches!(op.payload, "" | "phi" | "extension") && actual.is_empty() {
        return Err(Error::new("E_PAYLOAD_MISSING", op.name));
    }
    check(
        actual
            == if matches!(op.payload, "phi" | "extension") {
                ""
            } else {
                op.payload
            },
        "E_PAYLOAD_KIND",
    )?;
    if op.payload == "phi" {
        check(
            payload.is_some_and(|p| !p.phi_inputs.is_empty()),
            "E_PAYLOAD_MISSING",
        )?;
    }
    if op.payload == "extension" {
        check(
            payload.is_some_and(|p| !p.semantic_extension_id.is_empty()),
            "E_PAYLOAD_MISSING",
        )?;
    }
    if let Some(p) = payload {
        check(
            op.payload == "phi" || p.phi_inputs.is_empty(),
            "E_PAYLOAD_KIND",
        )?;
        check(
            op.payload == "extension" || p.semantic_extension_id.is_empty(),
            "E_PAYLOAD_KIND",
        )?;
        if let Some(D::Binary(b)) = &p.details {
            check(
                matches!(
                    b.overflow_mode.as_str(),
                    "WRAP" | "CHECKED" | "SATURATE" | "UNDEFINED"
                ) && matches!(b.signedness.as_str(), "SIGNED" | "UNSIGNED" | "N_A"),
                "E_UNKNOWN_KIND",
            )?;
            check(
                op.name != "NEG" || b.overflow_mode == "WRAP",
                "E_TYPE_MISMATCH",
            )?;
        }
        if let Some(D::Call(c)) = &p.details {
            check(
                matches!(
                    c.dispatch_kind.as_str(),
                    "STATIC"
                        | "VIRTUAL"
                        | "INTERFACE"
                        | "TRAIT"
                        | "REFLECTIVE"
                        | "DYNAMIC"
                        | "UNKNOWN"
                ),
                "E_UNKNOWN_KIND",
            )?;
            check(
                !c.is_referentially_transparent_proven || c.is_pure_proven,
                "E_TYPE_MISMATCH",
            )?;
        }
    }
    let args: Vec<&TypeDef> = i
        .operand_value_ids
        .iter()
        .map(|id| get(values, id).and_then(|v| get(types, &v.type_id)))
        .collect::<Result<_>>()?;
    let result = if i.result_value_id.is_empty() {
        None
    } else {
        Some(get(types, &get(values, &i.result_value_id)?.type_id)?)
    };
    let same = |a: &TypeDef, b: &TypeDef| a.id == b.id;
    let kind = |t: &TypeDef, k: &str| t.kind == k;
    let r = result;
    let ok = match op.name {
        "COPY" => same(args[0], r.unwrap()),
        "PHI" => {
            for input in &payload.unwrap().phi_inputs {
                check(
                    get(values, &input.value_id)?.type_id == r.unwrap().id,
                    "E_TYPE_MISMATCH",
                )?;
            }
            true
        }
        "SELECT" => kind(args[0], "BOOL") && same(args[1], args[2]) && same(args[1], r.unwrap()),
        "ADD" | "SUB" | "MUL" | "DIV" | "REM" | "BIT_AND" | "BIT_OR" | "XOR" => {
            kind(args[0], "BITVECTOR") && same(args[0], args[1]) && same(args[0], r.unwrap())
        }
        "NEG" | "BIT_NOT" => kind(args[0], "BITVECTOR") && same(args[0], r.unwrap()),
        "EQ" | "NE" => {
            same(args[0], args[1]) && !kind(args[0], "IEEE_FLOAT") && kind(r.unwrap(), "BOOL")
        }
        "LT" | "LE" | "GT" | "GE" => {
            kind(args[0], "BITVECTOR") && same(args[0], args[1]) && kind(r.unwrap(), "BOOL")
        }
        "SHL" | "SHR" => args.iter().all(|t| kind(t, "BITVECTOR")) && same(args[0], r.unwrap()),
        "LOGICAL_NOT" => kind(args[0], "BOOL") && kind(r.unwrap(), "BOOL"),
        "SIGN_EXTEND" | "ZERO_EXTEND" | "TRUNCATE" | "BITCAST" => {
            kind(args[0], "BITVECTOR")
                && kind(r.unwrap(), "BITVECTOR")
                && match op.name {
                    "TRUNCATE" => args[0].bit_width > r.unwrap().bit_width,
                    "BITCAST" => args[0].bit_width == r.unwrap().bit_width,
                    _ => args[0].bit_width < r.unwrap().bit_width,
                }
        }
        "BRANCH" => kind(args[0], "BOOL"),
        "SWITCH" => kind(args[0], "BITVECTOR"),
        "ALLOC" => kind(r.unwrap(), "POINTER"),
        "ADDRESS_OF" => {
            kind(r.unwrap(), "POINTER") && r.unwrap().param_type_ids.first() == Some(&args[0].id)
        }
        "FIELD_ADDR" => {
            let index = match payload.unwrap().details.as_ref().unwrap() {
                D::Field(f) => f.field_index as usize,
                _ => unreachable!(),
            };
            if kind(args[0], "POINTER") && kind(r.unwrap(), "POINTER") {
                let aggregate = get(
                    types,
                    args[0]
                        .param_type_ids
                        .first()
                        .ok_or_else(|| Error::new("E_TYPE_MISMATCH", "missing pointee"))?,
                )?;
                kind(aggregate, "STRUCT")
                    && aggregate.param_type_ids.get(index) == r.unwrap().param_type_ids.first()
            } else {
                false
            }
        }
        "INDEX_ADDR" => {
            matches!(args[0].kind.as_str(), "POINTER" | "SLICE")
                && kind(args[1], "BITVECTOR")
                && kind(r.unwrap(), "POINTER")
        }
        "LOAD" | "STORE" => {
            matches!(args[0].kind.as_str(), "POINTER" | "REFERENCE")
                && args[0].param_type_ids.first()
                    == Some(&if op.name == "STORE" {
                        args[1].id.clone()
                    } else {
                        r.unwrap().id.clone()
                    })
        }
        "EXTRACT_FIELD" | "INSERT_FIELD" => {
            let index = match payload.unwrap().details.as_ref().unwrap() {
                D::Field(f) => f.field_index as usize,
                _ => unreachable!(),
            };
            matches!(args[0].kind.as_str(), "STRUCT" | "TUPLE")
                && args[0].param_type_ids.get(index)
                    == Some(&if op.name == "EXTRACT_FIELD" {
                        r.unwrap().id.clone()
                    } else {
                        args[1].id.clone()
                    })
                && (op.name == "EXTRACT_FIELD" || same(args[0], r.unwrap()))
        }
        "INDEX" | "LENGTH" => {
            matches!(args[0].kind.as_str(), "ARRAY" | "SLICE" | "STRING")
                && if op.name == "LENGTH" {
                    kind(r.unwrap(), "BITVECTOR")
                } else {
                    kind(args[1], "BITVECTOR")
                        && (kind(args[0], "STRING")
                            || args[0].param_type_ids.first() == Some(&r.unwrap().id))
                }
        }
        "GET_TAG" => {
            matches!(args[0].kind.as_str(), "ENUM" | "INTERFACE") && kind(r.unwrap(), "BITVECTOR")
        }
        "RECOVER" => kind(r.unwrap(), "INTERFACE"),
        "MAKE_INTERFACE" => {
            let target = match payload.unwrap().details.as_ref().unwrap() {
                D::TypeTest(t) => get(types, &t.target_type_id)?,
                _ => unreachable!(),
            };
            kind(r.unwrap(), "INTERFACE") && same(args[0], target)
        }
        "MAKE_CLOSURE" => kind(args[0], "FUNCTION") && kind(r.unwrap(), "FUNCTION"),
        "CALL" | "INVOKE" | "SPAWN" | "DEFER_PUSH" => {
            if kind(args[0], "FUNCTION") {
                let function = args[0];
                check(
                    function.param_type_ids.len() == args.len() - 1,
                    "E_ARITY_MISMATCH",
                )?;
                let parameters_match = function
                    .param_type_ids
                    .iter()
                    .zip(&args[1..])
                    .all(|(id, arg)| id == &arg.id);
                let result_matches = if matches!(op.name, "SPAWN" | "DEFER_PUSH") {
                    true // Runtime discards the deferred/spawned return values.
                } else {
                    match (function.result_type_ids.as_slice(), r) {
                        ([], None) => true,
                        ([id], Some(t)) => id == &t.id,
                        (ids, Some(t)) if ids.len() > 1 => {
                            kind(t, "TUPLE") && t.param_type_ids == ids
                        }
                        _ => false,
                    }
                };
                parameters_match && result_matches
            } else {
                // Interface receivers have no concrete FUNCTION signature here.
                // Dynamic target resolution remains conservative, not pointer-callable.
                kind(args[0], "INTERFACE")
            }
        }
        "NEW_OBJECT" | "TYPE_TEST" | "DOWNCAST" => {
            let tt = match payload.unwrap().details.as_ref().unwrap() {
                D::TypeTest(t) => t,
                _ => unreachable!(),
            };
            let target = get(types, &tt.target_type_id)?;
            match op.name {
                "NEW_OBJECT" => {
                    matches!(r.unwrap().kind.as_str(), "POINTER" | "REFERENCE")
                        && r.unwrap().param_type_ids.first() == Some(&target.id)
                }
                "DOWNCAST" => same(r.unwrap(), target),
                _ => {
                    kind(args[0], "INTERFACE")
                        && if tt.comma_ok {
                            kind(r.unwrap(), "TUPLE")
                                && r.unwrap().param_type_ids.len() == 2
                                && r.unwrap().param_type_ids[0] == target.id
                                && kind(get(types, &r.unwrap().param_type_ids[1])?, "BOOL")
                        } else {
                            kind(r.unwrap(), "BOOL")
                        }
                }
            }
        }
        _ => true, // Registry imposes all constraints for unconstrained T/effect/control operations.
    };
    check(ok, "E_TYPE_MISMATCH")
}
pub fn validate_edges(
    i: &InstructionDef,
    edges: &[ControlEdgeDef],
    values: &HashMap<String, ValueDef>,
) -> Result<()> {
    let op =
        registry::lookup(&i.opcode).ok_or_else(|| Error::new("E_UNKNOWN_OPCODE", &i.opcode))?;
    // Shape validation can precede instruction validation during streaming import.
    // Check operand availability independently; never assume another validator ran.
    check(
        (op.min_operands..=op.max_operands).contains(&i.operand_value_ids.len()),
        "E_ARITY_MISMATCH",
    )?;
    check(
        (op.min_edges..=op.max_edges).contains(&edges.len()),
        "E_EDGE_SHAPE",
    )?;
    let mut ordered: Vec<_> = edges.iter().collect();
    ordered.sort_by_key(|e| e.outcome_ordinal);
    let mut cases = HashSet::new();
    for (n, e) in ordered.iter().enumerate() {
        check(
            matches!(
                e.edge_kind.as_str(),
                "UNCONDITIONAL"
                    | "BRANCH_TRUE"
                    | "BRANCH_FALSE"
                    | "SWITCH_CASE"
                    | "SWITCH_DEFAULT"
                    | "NORMAL"
                    | "EXCEPTION"
            ),
            "E_UNKNOWN_KIND",
        )?;
        check(
            e.source_block_id == i.block_id && e.terminator_instruction_id == i.id,
            "E_EDGE_SOURCE_MISMATCH",
        )?;
        check(e.outcome_ordinal as usize == n, "E_EDGE_SHAPE")?;
        let (k, label) = match i.opcode.as_str() {
            "JUMP" => ("UNCONDITIONAL", "NEXT".to_owned()),
            "BRANCH" => {
                if n == 0 {
                    ("BRANCH_TRUE", "TRUE".into())
                } else {
                    ("BRANCH_FALSE", "FALSE".into())
                }
            }
            "SWITCH" => {
                if n + 1 == edges.len() {
                    ("SWITCH_DEFAULT", "DEFAULT".into())
                } else {
                    ("SWITCH_CASE", format!("CASE:{n}"))
                }
            }
            "INVOKE" => {
                if n == 0 {
                    ("NORMAL", "NORMAL".into())
                } else {
                    ("EXCEPTION", "EXCEPTION".into())
                }
            }
            _ => return Err(Error::new("E_EDGE_SHAPE", &i.opcode)),
        };
        check(e.edge_kind == k && e.outcome_label == label, "E_EDGE_SHAPE")?;
        check(
            if i.opcode == "BRANCH" {
                i.operand_value_ids.first() == Some(&e.guard_value_id)
                    && e.guard_negated == (n == 1)
            } else {
                e.guard_value_id.is_empty() && !e.guard_negated
            },
            "E_EDGE_SHAPE",
        )?;
        check(
            k == "EXCEPTION" || (e.exception_filter_type_id.is_empty() && !e.exception_catch_all),
            "E_EDGE_SHAPE",
        )?;
        check(
            !e.exception_catch_all || n + 1 == edges.len(),
            "E_EDGE_SHAPE",
        )?;
        if k == "SWITCH_CASE" {
            check(!e.case_value_ids.is_empty(), "E_EDGE_SHAPE")?;
            let tag_id = i
                .operand_value_ids
                .first()
                .ok_or_else(|| Error::new("E_ARITY_MISMATCH", "SWITCH requires tag"))?;
            let tag = get(values, tag_id)?;
            for id in &e.case_value_ids {
                let v = get(values, id)?;
                check(
                    v.role == "CONST" && v.type_id == tag.type_id,
                    "E_TYPE_MISMATCH",
                )?;
                let bits = v.literal.as_ref().and_then(|l| match &l.representation {
                    Some(L::MachineBitsBe(b)) => Some(b.clone()),
                    _ => None,
                });
                check(bits.is_some() && cases.insert(bits), "E_EDGE_SHAPE")?;
            }
        } else {
            check(e.case_value_ids.is_empty(), "E_EDGE_SHAPE")?;
        }
    }
    Ok(())
}
