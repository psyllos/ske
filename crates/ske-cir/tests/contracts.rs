use ske_cir::{proto::*, registry::OPCODES, *};
use std::collections::HashMap;
#[test]
fn null_literals_require_a_core_nilable_kind() {
    let nil = LiteralValue {
        representation: Some(literal_value::Representation::NullValue(true)),
    };
    for kind in ["POINTER", "REFERENCE", "INTERFACE"] {
        let ty = TypeDef {
            kind: kind.into(),
            ..Default::default()
        };
        validate_literal(&nil, &ty).unwrap();
    }
    for descriptor in [
        "go:map[string]int",
        "go:chan int",
        "go:chan<- int",
        "go:<-chan int",
    ] {
        let ty = TypeDef {
            kind: "OPAQUE".into(),
            canonical_descriptor: descriptor.into(),
            ..Default::default()
        };
        assert_eq!(
            validate_literal(&nil, &ty).unwrap_err().code,
            "E_LITERAL_WIDTH"
        );
    }
    for (kind, descriptor) in [
        ("SLICE", "go:[]int"),
        ("FUNCTION", "go:func()"),
        ("OPAQUE", "go:complex128"),
        ("OPAQUE", "foreign"),
        ("BOOL", ""),
        ("BITVECTOR", ""),
        ("STRUCT", ""),
        ("ARRAY", ""),
        ("STRING", ""),
    ] {
        let ty = TypeDef {
            kind: kind.into(),
            canonical_descriptor: descriptor.into(),
            ..Default::default()
        };
        assert_eq!(
            validate_literal(&nil, &ty).unwrap_err().code,
            "E_LITERAL_WIDTH"
        );
    }
    let false_nil = LiteralValue {
        representation: Some(literal_value::Representation::NullValue(false)),
    };
    let ty = TypeDef {
        kind: "SLICE".into(),
        ..Default::default()
    };
    assert!(validate_literal(&false_nil, &ty).is_err());
}

#[test]
fn callable_signature_and_payload_regressions() {
    let (mut types, mut values) = environment();
    types.get_mut("fn").unwrap().param_type_ids = vec!["bool".into()];
    types.get_mut("fn").unwrap().result_type_ids = vec!["bool".into()];
    values.get_mut("ptr").unwrap().role = "GLOBAL".into();
    for name in ["CALL", "INVOKE", "SPAWN", "DEFER_PUSH"] {
        let mut i = specimen(ske_cir::registry::lookup(name).unwrap());
        i.operand_value_ids = vec!["fn".into(), "bool".into()];
        i.result_value_id = if matches!(name, "CALL" | "INVOKE") {
            "bool".into()
        } else {
            String::new()
        };
        validate_instruction(&i, &types, &values).unwrap();
        i.operand_value_ids[1] = "ptr".into();
        assert_eq!(
            validate_instruction(&i, &types, &values).unwrap_err().code,
            "E_TYPE_MISMATCH"
        );
        i.operand_value_ids[1] = "bool".into();
        if matches!(name, "CALL" | "INVOKE") {
            i.result_value_id = "iface".into();
            assert_eq!(
                validate_instruction(&i, &types, &values).unwrap_err().code,
                "E_TYPE_MISMATCH"
            );
            i.result_value_id = "bool".into();
        }
        i.operand_value_ids[0] = "ptr".into();
        assert_eq!(
            validate_instruction(&i, &types, &values).unwrap_err().code,
            "E_TYPE_MISMATCH"
        );
    }
    let mut call = specimen(ske_cir::registry::lookup("CALL").unwrap());
    call.operand_value_ids.push("bool".into());
    call.result_value_id = "bool".into();
    let set_call = |i: &mut InstructionDef, dispatch: &str, pure: bool, referential: bool| {
        i.payload.as_mut().unwrap().details =
            Some(operation_payload::Details::Call(CallSemantics {
                dispatch_kind: dispatch.into(),
                is_pure_proven: pure,
                is_referentially_transparent_proven: referential,
                ..Default::default()
            }));
    };
    set_call(&mut call, "BOGUS", false, false);
    assert_eq!(
        validate_instruction(&call, &types, &values)
            .unwrap_err()
            .code,
        "E_UNKNOWN_KIND"
    );
    set_call(&mut call, "STATIC", false, true);
    assert_eq!(
        validate_instruction(&call, &types, &values)
            .unwrap_err()
            .code,
        "E_TYPE_MISMATCH"
    );
    set_call(&mut call, "STATIC", true, true);
    validate_instruction(&call, &types, &values).unwrap();
    call.operand_value_ids.pop();
    assert_eq!(
        validate_instruction(&call, &types, &values)
            .unwrap_err()
            .code,
        "E_ARITY_MISMATCH"
    );
}

#[test]
fn neg_wrap_and_add_other_modes() {
    let (types, values) = environment();
    for name in ["NEG", "ADD"] {
        let mut i = specimen(ske_cir::registry::lookup(name).unwrap());
        for mode in ["WRAP", "CHECKED", "SATURATE", "UNDEFINED"] {
            if let Some(operation_payload::Details::Binary(b)) =
                &mut i.payload.as_mut().unwrap().details
            {
                b.overflow_mode = mode.into();
            }
            let result = validate_instruction(&i, &types, &values);
            assert_eq!(
                result.is_ok(),
                name == "ADD" || mode == "WRAP",
                "{name} {mode}"
            );
        }
    }
}

#[test]
fn call_multiple_returns_require_matching_tuple() {
    let (mut types, mut values) = environment();
    types.get_mut("fn").unwrap().result_type_ids = vec!["bv".into(), "bool".into()];
    types.insert(
        "tuple".into(),
        TypeDef {
            id: "tuple".into(),
            kind: "TUPLE".into(),
            param_type_ids: vec!["bv".into(), "bool".into()],
            ..Default::default()
        },
    );
    values.insert(
        "tuple".into(),
        ValueDef {
            id: "tuple".into(),
            type_id: "tuple".into(),
            role: "SSA".into(),
            ..Default::default()
        },
    );
    let mut call = specimen(ske_cir::registry::lookup("CALL").unwrap());
    call.result_value_id = "tuple".into();
    validate_instruction(&call, &types, &values).unwrap();
    types.get_mut("tuple").unwrap().param_type_ids.reverse();
    assert_eq!(
        validate_instruction(&call, &types, &values)
            .unwrap_err()
            .code,
        "E_TYPE_MISMATCH"
    );
    call.result_value_id.clear();
    assert_eq!(
        validate_instruction(&call, &types, &values)
            .unwrap_err()
            .code,
        "E_TYPE_MISMATCH"
    );
}

#[test]
fn bitvec_constructor_enforces_invariant() {
    use ske_cir::bitvec::{ArithmeticError, BitVec};
    assert_eq!(BitVec::new(0, 0), Err(ArithmeticError::InvalidWidth));
    assert_eq!(BitVec::new(129, 0), Err(ArithmeticError::InvalidWidth));
    let b = BitVec::new(8, 256).unwrap();
    assert_eq!(b.width(), 8);
    assert_eq!(b.bits(), 0);
    assert_eq!(b.signed(), 0);
    assert!(b.lt(BitVec::new(8, 1).unwrap(), false).unwrap());
    assert_eq!((-BitVec::new(8, 128).unwrap()).bits(), 128);
}

#[test]
fn edge_validation_empty_operands_never_panics() {
    for opcode in ["BRANCH", "SWITCH"] {
        let instruction = InstructionDef {
            opcode: opcode.into(),
            is_terminator: true,
            ..Default::default()
        };
        let edges = vec![ControlEdgeDef::default(); 2];
        let error = validate_edges(&instruction, &edges, &HashMap::new()).unwrap_err();
        assert_eq!(error.code, "E_ARITY_MISMATCH", "{opcode}");
    }
}

fn environment() -> (HashMap<String, TypeDef>, HashMap<String, ValueDef>) {
    let mut ts: HashMap<String, TypeDef> = HashMap::new();
    for (id, kind, width, params) in [
        ("bv", "BITVECTOR", 8, vec![]),
        ("wide", "BITVECTOR", 16, vec![]),
        ("bool", "BOOL", 0, vec![]),
        ("struct", "STRUCT", 0, vec!["bv"]),
        ("ptr", "POINTER", 0, vec!["bv"]),
        ("ptrstruct", "POINTER", 0, vec!["struct"]),
        ("arr", "ARRAY", 0, vec!["bv"]),
        ("fn", "FUNCTION", 0, vec![]),
        ("iface", "INTERFACE", 0, vec![]),
    ] {
        ts.insert(
            id.into(),
            TypeDef {
                id: id.into(),
                kind: kind.into(),
                bit_width: width,
                param_type_ids: params.into_iter().map(String::from).collect(),
                component_names: if id == "struct" {
                    vec!["x".into()]
                } else {
                    vec![]
                },
                ..Default::default()
            },
        );
    }
    let vs = ts
        .keys()
        .map(|id| {
            (
                id.clone(),
                ValueDef {
                    id: id.clone(),
                    type_id: id.clone(),
                    role: "SSA".into(),
                    ..Default::default()
                },
            )
        })
        .collect();
    (ts, vs)
}
fn specimen(op: &ske_cir::registry::Opcode) -> InstructionDef {
    use operation_payload::Details as D;
    let (args, result): (Vec<&str>, &str) = match op.name {
        "SELECT" => (vec!["bool", "bv", "bv"], "bv"),
        "EQ" | "NE" | "LT" | "LE" | "GT" | "GE" => (vec!["bv", "bv"], "bool"),
        "LOGICAL_NOT" => (vec!["bool"], "bool"),
        "BRANCH" => (vec!["bool"], ""),
        "SIGN_EXTEND" | "ZERO_EXTEND" => (vec!["bv"], "wide"),
        "TRUNCATE" => (vec!["wide"], "bv"),
        "EXTRACT_FIELD" => (vec!["struct"], "bv"),
        "INSERT_FIELD" => (vec!["struct", "bv"], "struct"),
        "INDEX" => (vec!["arr", "bv"], "bv"),
        "LENGTH" => (vec!["arr"], "bv"),
        "GET_TAG" => (vec!["iface"], "bv"),
        "ALLOC" | "NEW_OBJECT" => (vec![], "ptr"),
        "LOAD" => (vec!["ptr"], "bv"),
        "STORE" => (vec!["ptr", "bv"], ""),
        "FIELD_ADDR" => (vec!["ptrstruct"], "ptr"),
        "INDEX_ADDR" => (vec!["ptr", "bv"], "ptr"),
        "ADDRESS_OF" => (vec!["bv"], "ptr"),
        "CALL" | "INVOKE" | "SPAWN" | "DEFER_PUSH" => (vec!["fn"], ""),
        "MAKE_CLOSURE" => (vec!["fn"], "fn"),
        "TYPE_TEST" => (vec!["iface"], "bool"),
        "DOWNCAST" => (vec!["iface"], "bv"),
        "MAKE_INTERFACE" => (vec!["bv"], "iface"),
        "RECOVER" => (vec![], "iface"),
        _ => (
            vec!["bv"; op.min_operands],
            if op.result == 1 { "bv" } else { "" },
        ),
    };
    let mut p = OperationPayload {
        details: match op.payload {
            "binary" => Some(D::Binary(BinarySemantics {
                overflow_mode: "WRAP".into(),
                signedness: "UNSIGNED".into(),
                ..Default::default()
            })),
            "memory" => Some(D::Memory(MemorySemantics::default())),
            "field" => Some(D::Field(FieldAccess::default())),
            "call" => Some(D::Call(CallSemantics {
                dispatch_kind: "STATIC".into(),
                ..Default::default()
            })),
            "opaque" => Some(D::Opaque(OpaqueSemantics::default())),
            "type_test" => Some(D::TypeTest(TypeTest {
                target_type_id: "bv".into(),
                comma_ok: false,
            })),
            _ => None,
        },
        ..Default::default()
    };
    if op.payload == "phi" {
        p.phi_inputs.push(PhiInput {
            predecessor_block_id: "b".into(),
            value_id: "bv".into(),
        });
    }
    if op.payload == "extension" {
        p.semantic_extension_id = "example.effect".into();
    }
    InstructionDef {
        id: "i".into(),
        block_id: "b".into(),
        opcode: op.name.into(),
        operand_value_ids: args.into_iter().map(String::from).collect(),
        result_value_id: result.into(),
        payload: Some(p),
        is_terminator: op.terminator,
        ..Default::default()
    }
}
#[test]
fn registry_documented() {
    let (types, values) = environment();
    let docs = include_str!("../../../docs/cir-semantics.md");
    for op in OPCODES {
        assert!(
            docs.lines().any(|l| l == format!("### {}", op.name)),
            "{}",
            op.name
        );
        let good = specimen(op);
        validate_instruction(&good, &types, &values).unwrap_or_else(|e| panic!("{}: {e}", op.name));
        let mut bad = good.clone();
        // Every fixed arity has an arity negative; variable forms use payload/type negatives.
        if op.max_operands != usize::MAX {
            bad.operand_value_ids = vec!["bv".into(); op.max_operands + 1];
        } else if !op.payload.is_empty() {
            bad.payload = None;
        } else {
            bad.payload = Some(OperationPayload {
                details: Some(operation_payload::Details::Opaque(
                    OpaqueSemantics::default(),
                )),
                ..Default::default()
            });
        }
        assert!(
            validate_instruction(&bad, &types, &values).is_err(),
            "negative {}",
            op.name
        );
    }
}
#[test]
fn optional_effects_conservative() {
    assert_eq!(call_effects(&CallSemantics::default()), (true, true));
    assert_eq!(opaque_effects(&OpaqueSemantics::default()), (true, true));
    assert_eq!(
        call_effects(&CallSemantics {
            may_raise: Some(false),
            may_mutate_memory: Some(false),
            ..Default::default()
        }),
        (false, false)
    );
    assert_eq!(
        opaque_effects(&OpaqueSemantics {
            may_exit_abnormally: Some(false),
            may_mutate_memory: Some(false),
            ..Default::default()
        }),
        (false, false)
    );
}
#[test]
fn type_and_literal_contracts() {
    let (ts, _) = environment();
    for t in ts.values() {
        validate_type(t).unwrap();
    }
    let mut b = ts["bool"].clone();
    b.bit_width = 8;
    assert_eq!(validate_type(&b).unwrap_err().code, "E_TYPE_MISMATCH");
    let l = LiteralValue {
        representation: Some(literal_value::Representation::MachineBitsBe(vec![255])),
    };
    validate_literal(&l, &ts["bv"]).unwrap();
    assert_eq!(
        validate_literal(&l, &ts["wide"]).unwrap_err().code,
        "E_LITERAL_WIDTH"
    );
    b = ts["bv"].clone();
    b.bit_width = 3;
    assert!(validate_literal(&l, &b).is_err());
    for name in ["CONST", "CAST", "RESUME", "SUSPEND", "CMP_EQ", "BINARY"] {
        assert!(ske_cir::registry::lookup(name).is_none());
    }
}
#[test]
fn branch_edges_validate_guards_and_order() {
    let (_, vs) = environment();
    let i = specimen(ske_cir::registry::lookup("BRANCH").unwrap());
    let mut edges: Vec<_> = ["BRANCH_TRUE", "BRANCH_FALSE"]
        .iter()
        .enumerate()
        .map(|(n, k)| ControlEdgeDef {
            id: format!("e{n}"),
            source_block_id: "b".into(),
            terminator_instruction_id: "i".into(),
            edge_kind: k.to_string(),
            outcome_ordinal: n as u32,
            outcome_label: if n == 0 { "TRUE" } else { "FALSE" }.into(),
            guard_value_id: "bool".into(),
            guard_negated: n == 1,
            ..Default::default()
        })
        .collect();
    validate_edges(&i, &edges, &vs).unwrap();
    edges[1].guard_negated = false;
    assert_eq!(
        validate_edges(&i, &edges, &vs).unwrap_err().code,
        "E_EDGE_SHAPE"
    );
}

#[test]
fn switch_and_invoke_edges() {
    let (_, mut vs) = environment();
    for n in 0..2 {
        vs.insert(
            format!("c{n}"),
            ValueDef {
                id: format!("c{n}"),
                type_id: "bv".into(),
                role: "CONST".into(),
                literal: Some(LiteralValue {
                    representation: Some(literal_value::Representation::MachineBitsBe(vec![n])),
                }),
                ..Default::default()
            },
        );
    }
    let switch = specimen(ske_cir::registry::lookup("SWITCH").unwrap());
    let mut edges: Vec<_> = (0..3)
        .map(|n| ControlEdgeDef {
            id: format!("e{n}"),
            source_block_id: "b".into(),
            terminator_instruction_id: "i".into(),
            edge_kind: if n < 2 {
                "SWITCH_CASE"
            } else {
                "SWITCH_DEFAULT"
            }
            .into(),
            outcome_ordinal: n,
            outcome_label: if n < 2 {
                format!("CASE:{n}")
            } else {
                "DEFAULT".into()
            },
            case_value_ids: if n < 2 { vec![format!("c{n}")] } else { vec![] },
            ..Default::default()
        })
        .collect();
    validate_edges(&switch, &edges, &vs).unwrap();
    edges[1].case_value_ids = vec!["c0".into()];
    assert_eq!(
        validate_edges(&switch, &edges, &vs).unwrap_err().code,
        "E_EDGE_SHAPE"
    );
    edges[1].case_value_ids = vec!["c1".into()];
    edges[2].outcome_ordinal = 1;
    assert!(validate_edges(&switch, &edges, &vs).is_err());
    let invoke = specimen(ske_cir::registry::lookup("INVOKE").unwrap());
    let mut edges: Vec<_> = (0..3)
        .map(|n| ControlEdgeDef {
            source_block_id: "b".into(),
            terminator_instruction_id: "i".into(),
            outcome_ordinal: n,
            edge_kind: if n == 0 { "NORMAL" } else { "EXCEPTION" }.into(),
            outcome_label: if n == 0 { "NORMAL" } else { "EXCEPTION" }.into(),
            exception_catch_all: n == 2,
            ..Default::default()
        })
        .collect();
    validate_edges(&invoke, &edges, &vs).unwrap();
    edges[0].exception_catch_all = true;
    assert!(validate_edges(&invoke, &edges, &vs).is_err());
    edges[0].exception_catch_all = false;
    edges[1].exception_catch_all = true;
    assert_eq!(
        validate_edges(&invoke, &edges, &vs).unwrap_err().code,
        "E_EDGE_SHAPE"
    );
    edges[2].exception_catch_all = false;
    assert_eq!(
        validate_edges(&invoke, &edges, &vs).unwrap_err().code,
        "E_EDGE_SHAPE"
    );
    edges[1].exception_catch_all = false;
    edges[2].exception_catch_all = true;
    validate_edges(&invoke, &edges, &vs).unwrap();
}

#[test]
fn payload_type_and_reference_negatives() {
    let (types, values) = environment();
    let mut add = specimen(ske_cir::registry::lookup("ADD").unwrap());
    add.payload = None;
    assert_eq!(
        validate_instruction(&add, &types, &values)
            .unwrap_err()
            .code,
        "E_PAYLOAD_MISSING"
    );
    add.payload = Some(OperationPayload {
        details: Some(operation_payload::Details::Memory(
            MemorySemantics::default(),
        )),
        ..Default::default()
    });
    assert_eq!(
        validate_instruction(&add, &types, &values)
            .unwrap_err()
            .code,
        "E_PAYLOAD_KIND"
    );
    let mut add = specimen(ske_cir::registry::lookup("ADD").unwrap());
    add.operand_value_ids[1] = "bool".into();
    assert_eq!(
        validate_instruction(&add, &types, &values)
            .unwrap_err()
            .code,
        "E_TYPE_MISMATCH"
    );
    add.operand_value_ids[1] = "absent".into();
    assert_eq!(
        validate_instruction(&add, &types, &values)
            .unwrap_err()
            .code,
        "E_DANGLING_REF"
    );
}
