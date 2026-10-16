//! Independent concrete execution of hand-authored scalar CIR (§19 v0.4).
use ske_cir::proto::{self, operation_payload::Details};
use ske_symbolic::{
    concrete::{self, Value},
    engine::Budget,
    program::{Block, Edge, Program},
    summary::FunctionSummary,
    BinaryOp, Constant, ExprArena, ExprId, ExprKind, Sort, UnaryOp,
};
use std::collections::BTreeMap;

fn value(n: i64) -> Value {
    Value::integer(64, true, n as u128).unwrap()
}

struct Fixture(Program);
impl Fixture {
    fn new(name: &str, params: usize) -> Self {
        let mut p = Program {
            function: proto::FunctionDef {
                id: name.into(),
                symbol_id: name.into(),
                entry_block_id: "b0".into(),
                return_type_ids: vec!["int".into()],
                parameter_value_ids: (0..params).map(|i| format!("p{i}")).collect(),
                ..Default::default()
            },
            types: BTreeMap::new(),
            values: BTreeMap::new(),
            blocks: BTreeMap::new(),
            symbols: BTreeMap::new(),
        };
        for (id, kind) in [
            ("int", "BITVECTOR"),
            ("bool", "BOOL"),
            ("err", "INTERFACE"),
            ("ptr", "POINTER"),
            ("fn", "FUNCTION"),
        ] {
            p.types.insert(
                id.into(),
                proto::TypeDef {
                    id: id.into(),
                    kind: kind.into(),
                    bit_width: if id == "int" { 64 } else { 0 },
                    is_signed: id == "int",
                    ..Default::default()
                },
            );
        }
        for i in 0..params {
            p.values.insert(
                format!("p{i}"),
                proto::ValueDef {
                    id: format!("p{i}"),
                    function_id: name.into(),
                    type_id: "int".into(),
                    role: "PARAM".into(),
                    param_index: Some(i as u32),
                    ..Default::default()
                },
            );
        }
        for (id, n) in [("zero", 0_i64), ("one", 1), ("hundred", 100)] {
            p.values.insert(
                id.into(),
                proto::ValueDef {
                    id: id.into(),
                    function_id: name.into(),
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
        p.values.insert(
            "nil".into(),
            proto::ValueDef {
                id: "nil".into(),
                type_id: "err".into(),
                role: "CONST".into(),
                literal: Some(proto::LiteralValue {
                    representation: Some(proto::literal_value::Representation::NullValue(true)),
                }),
                ..Default::default()
            },
        );
        Self(p)
    }
    fn global(&mut self, id: &str, ty: &str, symbol: &str) {
        self.0.values.insert(
            id.into(),
            proto::ValueDef {
                id: id.into(),
                type_id: ty.into(),
                role: "GLOBAL".into(),
                symbol_id: symbol.into(),
                ..Default::default()
            },
        );
        self.0.symbols.insert(symbol.into(), symbol.into());
    }
    fn ins(&mut self, block: &str, op: &str, result: &str, ty: &str, operands: &[&str]) {
        if !result.is_empty() {
            self.0.values.insert(
                result.into(),
                proto::ValueDef {
                    id: result.into(),
                    type_id: ty.into(),
                    role: "SSA".into(),
                    ..Default::default()
                },
            );
        }
        let ordinal = self.0.blocks.len() as u32;
        let body = self.0.blocks.entry(block.into()).or_insert_with(|| Block {
            ordinal,
            instructions: vec![],
            edges: vec![],
        });
        let details = if op == "CALL" {
            Some(Details::Call(proto::CallSemantics {
                dispatch_kind: "STATIC".into(),
                world_closed: true,
                ..Default::default()
            }))
        } else if matches!(op, "ADD" | "SUB" | "MUL") {
            Some(Details::Binary(proto::BinarySemantics {
                overflow_mode: "WRAP".into(),
                ..Default::default()
            }))
        } else {
            None
        };
        body.instructions.push(proto::InstructionDef {
            id: format!("{}:{block}:{}", self.0.function.id, body.instructions.len()),
            block_id: block.into(),
            opcode: op.into(),
            result_value_id: result.into(),
            operand_value_ids: operands.iter().map(|s| (*s).into()).collect(),
            payload: Some(proto::OperationPayload {
                details,
                ..Default::default()
            }),
            is_terminator: matches!(op, "RETURN" | "BRANCH"),
            ..Default::default()
        });
    }
    fn branch(&mut self) {
        self.ins("b0", "BRANCH", "", "", &["condition"]);
        self.0.blocks.get_mut("b0").unwrap().edges = ["yes", "no"]
            .into_iter()
            .enumerate()
            .map(|(i, target)| Edge {
                definition: proto::ControlEdgeDef {
                    id: format!("{}:edge{i}", self.0.function.id),
                    target_block_id: target.into(),
                    outcome_ordinal: i as u32,
                    edge_kind: if i == 0 {
                        "BRANCH_TRUE"
                    } else {
                        "BRANCH_FALSE"
                    }
                    .into(),
                    ..Default::default()
                },
                outcome_id: Some(format!("{}:outcome{i}", self.0.function.id)),
            })
            .collect();
    }
}

fn programs() -> BTreeMap<String, Program> {
    let mut helper = Fixture::new("helper", 1);
    helper.ins("b0", "GT", "condition", "bool", &["p0", "hundred"]);
    helper.branch();
    helper.ins("yes", "RETURN", "", "", &["one"]);
    helper.ins("no", "RETURN", "", "", &["zero"]);
    let mut fact = Fixture::new("fact", 1);
    fact.global("callee", "fn", "fact");
    fact.ins("b0", "LE", "condition", "bool", &["p0", "one"]);
    fact.branch();
    fact.ins("yes", "RETURN", "", "", &["one"]);
    fact.ins("no", "SUB", "smaller", "int", &["p0", "one"]);
    fact.ins("no", "CALL", "recursive", "int", &["callee", "smaller"]);
    fact.ins("no", "MUL", "product", "int", &["p0", "recursive"]);
    fact.ins("no", "RETURN", "", "", &["product"]);
    let mut div = Fixture::new("div", 2);
    div.0.function.return_type_ids = vec!["int".into(), "err".into()];
    div.global("global-error", "ptr", "ErrZero");
    div.ins("b0", "EQ", "condition", "bool", &["p1", "zero"]);
    div.branch();
    div.ins("yes", "LOAD", "error", "err", &["global-error"]);
    div.ins("yes", "RETURN", "", "", &["zero", "error"]);
    div.ins("no", "DIV", "quotient", "int", &["p0", "p1"]);
    div.ins("no", "RETURN", "", "", &["quotient", "nil"]);
    [helper.0, fact.0, div.0]
        .into_iter()
        .map(|p| (p.function.id.clone(), p))
        .collect()
}

fn evaluate(arena: &ExprArena, id: ExprId, bindings: &BTreeMap<String, Value>) -> Value {
    let node = arena.get(id).unwrap();
    match &node.kind {
        ExprKind::Const(Constant::Bool(b)) => Value::Bool(*b),
        ExprKind::Const(Constant::Null) => Value::Nil,
        ExprKind::Const(Constant::Bits(bits)) => {
            let Sort::BitVec { width, signed } = node.ty.sort else {
                panic!("integer sort");
            };
            Value::integer(width, signed, *bits).unwrap()
        }
        ExprKind::Symbol(name) => bindings
            .get(name)
            .unwrap_or_else(|| panic!("unbound {name}"))
            .clone(),
        ExprKind::Unary {
            op: UnaryOp::Not,
            arg,
        } => {
            let Value::Bool(b) = evaluate(arena, *arg, bindings) else {
                panic!("boolean guard")
            };
            Value::Bool(!b)
        }
        ExprKind::Binary { op, left, right } => {
            let a = evaluate(arena, *left, bindings);
            let b = evaluate(arena, *right, bindings);
            if *op == BinaryOp::Eq {
                return Value::Bool(a == b);
            }
            if *op == BinaryOp::Ne {
                return Value::Bool(a != b);
            }
            let (Value::Bits { value: a, signed }, Value::Bits { value: b, .. }) = (a, b) else {
                panic!("integer operands")
            };
            match op {
                BinaryOp::Gt => Value::Bool(b.lt(a, signed).unwrap()),
                BinaryOp::Le => Value::Bool(!b.lt(a, signed).unwrap()),
                BinaryOp::Ge => Value::Bool(!a.lt(b, signed).unwrap()),
                BinaryOp::Lt => Value::Bool(a.lt(b, signed).unwrap()),
                _ => Value::Bits {
                    value: a
                        .binary(
                            match op {
                                BinaryOp::Sub => "SUB",
                                BinaryOp::Mul => "MUL",
                                BinaryOp::Div => "DIV",
                                _ => panic!("unexpected arithmetic in reference fixture"),
                            },
                            b,
                            signed,
                        )
                        .unwrap(),
                    signed,
                },
            }
        }
        _ => panic!("unexecutable summary expression {:?}", node.kind),
    }
}

#[test]
fn summary_matches_concrete() {
    let programs = programs();
    let globals = BTreeMap::from([("ErrZero".into(), Value::Object("predefined-error".into()))]);
    let mut seed = 0x12345678_u64;
    for name in ["helper", "fact", "div"] {
        let summary: FunctionSummary = ske_summary::compute_bounded(
            name,
            &programs,
            &Budget::default(),
            &mut ske_solver::Z3Solver::new(),
        )
        .unwrap();
        assert_eq!(summary.cases.len(), if name == "fact" { 4 } else { 2 });
        for sample in 0..207 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let n = if sample < 7 {
                sample as i64
            } else {
                (seed % 31) as i64 - 10
            };
            let args = match name {
                "helper" => vec![value(n * 10)],
                "fact" => vec![value(n)],
                _ => vec![value(n * 7), value((seed % 7) as i64 - 3)],
            };
            let (concrete, _) = concrete::run(
                &programs[name],
                &programs,
                &args,
                &globals,
                &concrete::Limits::default(),
            )
            .unwrap();
            let mut bindings: BTreeMap<_, _> = summary
                .parameter_symbols
                .iter()
                .cloned()
                .zip(args)
                .collect();
            for (symbol, global, _) in &summary.global_reads {
                bindings.insert(symbol.clone(), globals[global].clone());
            }
            let cases: Vec<_> = summary
                .cases
                .iter()
                .filter(|case| {
                    case.constraints
                        .iter()
                        .all(|id| evaluate(&summary.arena, *id, &bindings) == Value::Bool(true))
                })
                .collect();
            assert_eq!(cases.len(), 1, "{name} n={n}");
            if !cases[0].partial {
                let expected: Vec<_> = cases[0]
                    .values
                    .iter()
                    .map(|id| evaluate(&summary.arena, *id, &bindings))
                    .collect();
                assert_eq!(expected, concrete, "{name} n={n}");
            }
        }
    }
}
