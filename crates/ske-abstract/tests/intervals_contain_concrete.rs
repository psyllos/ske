use ske_abstract::{cir::infer, Interval};
use ske_cir::proto;
use ske_symbolic::{
    concrete::{run, Limits, Value},
    program::{Block, Edge, Program},
};
use std::collections::BTreeMap;

fn sum() -> Program {
    let mut p = Program {
        function: proto::FunctionDef {
            id: "Sum".into(),
            entry_block_id: "entry".into(),
            parameter_value_ids: vec!["n".into()],
            return_type_ids: vec!["int".into()],
            ..Default::default()
        },
        types: BTreeMap::from([
            (
                "int".into(),
                proto::TypeDef {
                    id: "int".into(),
                    kind: "BITVECTOR".into(),
                    bit_width: 64,
                    is_signed: true,
                    ..Default::default()
                },
            ),
            (
                "bool".into(),
                proto::TypeDef {
                    id: "bool".into(),
                    kind: "BOOL".into(),
                    ..Default::default()
                },
            ),
        ]),
        values: BTreeMap::new(),
        blocks: BTreeMap::new(),
        symbols: BTreeMap::new(),
    };
    for (id, role) in [
        ("n", "PARAM"),
        ("i", "SSA"),
        ("s", "SSA"),
        ("next-i", "SSA"),
        ("next-s", "SSA"),
        ("test", "SSA"),
    ] {
        p.values.insert(
            id.into(),
            proto::ValueDef {
                id: id.into(),
                type_id: if id == "test" { "bool" } else { "int" }.into(),
                role: role.into(),
                param_index: if id == "n" { Some(0) } else { None },
                ..Default::default()
            },
        );
    }
    for (id, n) in [("zero", 0_i64), ("one", 1)] {
        p.values.insert(
            id.into(),
            proto::ValueDef {
                id: id.into(),
                type_id: "int".into(),
                role: "CONST".into(),
                literal: Some(proto::LiteralValue {
                    representation: Some(proto::literal_value::Representation::MachineBitsBe(
                        n.to_be_bytes().to_vec(),
                    )),
                }),
                ..Default::default()
            },
        );
    }
    let ins = |id: &str, op: &str, result: &str, args: &[&str]| proto::InstructionDef {
        id: id.into(),
        opcode: op.into(),
        result_value_id: result.into(),
        operand_value_ids: args.iter().map(|s| (*s).into()).collect(),
        is_terminator: matches!(op, "JUMP" | "BRANCH" | "RETURN"),
        ..Default::default()
    };
    let phi = |id: &str, back: &str| {
        let mut phi = ins(id, "PHI", id, &[]);
        phi.payload = Some(proto::OperationPayload {
            phi_inputs: vec![
                proto::PhiInput {
                    predecessor_block_id: "entry".into(),
                    value_id: "zero".into(),
                },
                proto::PhiInput {
                    predecessor_block_id: "body".into(),
                    value_id: back.into(),
                },
            ],
            ..Default::default()
        });
        phi
    };
    let edge = |target: &str, kind: &str| Edge {
        definition: proto::ControlEdgeDef {
            target_block_id: target.into(),
            edge_kind: kind.into(),
            ..Default::default()
        },
        outcome_id: None,
    };
    for (id, instructions, edges) in [
        (
            "entry",
            vec![ins("start", "JUMP", "", &[])],
            vec![edge("header", "UNCONDITIONAL")],
        ),
        (
            "header",
            vec![
                phi("i", "next-i"),
                phi("s", "next-s"),
                ins("test", "LT", "test", &["i", "n"]),
                ins("branch", "BRANCH", "", &["test"]),
            ],
            vec![edge("body", "BRANCH_TRUE"), edge("exit", "BRANCH_FALSE")],
        ),
        (
            "body",
            vec![
                ins("accumulate", "ADD", "next-s", &["s", "i"]),
                ins("increment", "ADD", "next-i", &["i", "one"]),
                ins("back", "JUMP", "", &[]),
            ],
            vec![edge("header", "UNCONDITIONAL")],
        ),
        ("exit", vec![ins("return", "RETURN", "", &["s"])], vec![]),
    ] {
        p.blocks.insert(
            id.into(),
            Block {
                ordinal: p.blocks.len() as u32,
                instructions,
                edges,
            },
        );
    }
    p
}

#[test]
fn intervals_contain_concrete_cir_sum_on_200_inputs() {
    let program = sum();
    let instructions: Vec<_> = program
        .blocks
        .values()
        .flat_map(|b| b.instructions.iter().cloned())
        .collect();
    let invariant = infer(&program.types, &program.values, &instructions).unwrap();
    assert!(!matches!(invariant["i"], Interval::Bottom));
    assert!(!matches!(invariant["s"], Interval::Bottom));
    let mut seed = 12345_u64;
    for _ in 0..200 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let n = (seed % 41) as i64 - 10;
        let (returns, trace) = run(
            &program,
            &BTreeMap::new(),
            &[Value::integer(64, true, n as u128).unwrap()],
            &BTreeMap::new(),
            &Limits::default(),
        )
        .unwrap();
        let expected = (0..n).sum::<i64>();
        assert_eq!(
            returns,
            vec![Value::integer(64, true, expected as u128).unwrap()]
        );
        for (id, value) in trace.values.iter().filter(|(id, _)| id == "i" || id == "s") {
            let Value::Bits { value, .. } = value else {
                panic!("integer loop variable");
            };
            assert!(
                invariant[id].contains(value.signed()),
                "{id}={} outside {:?}",
                value.signed(),
                invariant[id]
            );
        }
    }
}
