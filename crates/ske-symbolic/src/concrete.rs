//! Bounded concrete execution of validated scalar CIR.
//!
//! No host code is executed. Globals and callable CIR bodies are supplied by
//! the caller; unsupported effects fail explicitly rather than being skipped.
use crate::program::Program;
use ske_cir::{
    bitvec::BitVec,
    proto::{self, operation_payload::Details},
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bits {
        value: BitVec,
        signed: bool,
    },
    Bool(bool),
    Nil,
    /// A caller-supplied, non-nil identity (for example an error object).
    Object(String),
    Tuple(Vec<Value>),
}

impl Value {
    pub fn integer(width: u32, signed: bool, bits: u128) -> Result<Self, Error> {
        Ok(Self::Bits {
            value: BitVec::new(width, bits).map_err(|e| Error::Arithmetic(format!("{e:?}")))?,
            signed,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Missing(String),
    Type(String),
    Unsupported(String),
    Arithmetic(String),
    StepLimit,
    RecursionLimit,
}

#[derive(Clone, Debug)]
pub struct Limits {
    pub max_steps: usize,
    pub max_recursion_depth: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_steps: 100_000,
            max_recursion_depth: 64,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trace {
    pub instructions: Vec<String>,
    /// Values at instruction results, including successive loop iterations.
    pub values: Vec<(String, Value)>,
}

/// Execute an entry and its supplied callable bodies. `functions` is keyed by
/// qualified symbol name; `globals` supplies the contents of global locations.
pub fn run(
    entry: &Program,
    functions: &BTreeMap<String, Program>,
    parameters: &[Value],
    globals: &BTreeMap<String, Value>,
    limits: &Limits,
) -> Result<(Vec<Value>, Trace), Error> {
    let mut machine = Machine {
        functions,
        globals,
        limits,
        trace: Trace::default(),
    };
    let result = machine.execute(entry, parameters, 0)?;
    Ok((result, machine.trace))
}

struct Machine<'a> {
    functions: &'a BTreeMap<String, Program>,
    globals: &'a BTreeMap<String, Value>,
    limits: &'a Limits,
    trace: Trace,
}

fn lookup(env: &BTreeMap<String, Value>, id: &str) -> Result<Value, Error> {
    env.get(id)
        .cloned()
        .ok_or_else(|| Error::Missing(id.into()))
}

impl Machine<'_> {
    fn execute(
        &mut self,
        p: &Program,
        parameters: &[Value],
        depth: usize,
    ) -> Result<Vec<Value>, Error> {
        if depth > self.limits.max_recursion_depth {
            return Err(Error::RecursionLimit);
        }
        if parameters.len() != p.function.parameter_value_ids.len() {
            return Err(Error::Type("parameter arity".into()));
        }
        let mut env: BTreeMap<String, Value> = p
            .function
            .parameter_value_ids
            .iter()
            .cloned()
            .zip(parameters.iter().cloned())
            .collect();
        for v in p.values.values().filter(|v| v.role == "CONST") {
            let ty = p
                .types
                .get(&v.type_id)
                .ok_or_else(|| Error::Missing(v.type_id.clone()))?;
            let value = match v.literal.as_ref().and_then(|l| l.representation.as_ref()) {
                Some(proto::literal_value::Representation::BoolValue(b)) => Value::Bool(*b),
                Some(proto::literal_value::Representation::NullValue(true)) => Value::Nil,
                Some(proto::literal_value::Representation::MachineBitsBe(bytes))
                    if ty.kind == "BITVECTOR" && bytes.len() <= 16 =>
                {
                    Value::integer(
                        ty.bit_width,
                        ty.is_signed,
                        bytes.iter().fold(0, |n, b| (n << 8) | u128::from(*b)),
                    )?
                }
                _ => return Err(Error::Unsupported(format!("literal {}", v.id))),
            };
            env.insert(v.id.clone(), value);
        }
        let mut block_id = p.function.entry_block_id.clone();
        let mut predecessor = None::<String>;
        loop {
            let block = p
                .blocks
                .get(&block_id)
                .ok_or_else(|| Error::Missing(block_id.clone()))?;
            // PHIs are simultaneous: all incoming reads use the predecessor's env.
            let incoming = env.clone();
            let mut next = None;
            for ins in &block.instructions {
                if self.trace.instructions.len() >= self.limits.max_steps {
                    return Err(Error::StepLimit);
                }
                self.trace.instructions.push(ins.id.clone());
                let details = ins.payload.as_ref().and_then(|p| p.details.as_ref());
                let result = match ins.opcode.as_str() {
                    "RETURN" => {
                        return ins
                            .operand_value_ids
                            .iter()
                            .map(|id| lookup(&env, id))
                            .collect()
                    }
                    "JUMP" | "BRANCH" | "SWITCH" => {
                        let mut chosen = None;
                        let mut edges: Vec<_> = block.edges.iter().collect();
                        edges.sort_by_key(|e| e.definition.outcome_ordinal);
                        for edge in edges {
                            let e = &edge.definition;
                            let take = match ins.opcode.as_str() {
                                "JUMP" => true,
                                "BRANCH" => {
                                    let Value::Bool(b) = lookup(&env, &ins.operand_value_ids[0])?
                                    else {
                                        return Err(Error::Type(ins.id.clone()));
                                    };
                                    b != (e.edge_kind == "BRANCH_FALSE")
                                }
                                _ => {
                                    let tag = lookup(&env, &ins.operand_value_ids[0])?;
                                    let matched = block
                                        .edges
                                        .iter()
                                        .flat_map(|e| &e.definition.case_value_ids)
                                        .map(|id| lookup(&env, id))
                                        .collect::<Result<Vec<_>, _>>()?
                                        .contains(&tag);
                                    if e.edge_kind == "SWITCH_DEFAULT" {
                                        !matched
                                    } else {
                                        e.case_value_ids
                                            .iter()
                                            .map(|id| lookup(&env, id))
                                            .collect::<Result<Vec<_>, _>>()?
                                            .contains(&tag)
                                    }
                                }
                            };
                            if take {
                                chosen = Some(e.target_block_id.clone());
                                break;
                            }
                        }
                        next = Some(
                            chosen
                                .ok_or_else(|| Error::Missing(format!("successor {}", ins.id)))?,
                        );
                        break;
                    }
                    "PHI" => {
                        let input = ins
                            .payload
                            .as_ref()
                            .and_then(|p| {
                                p.phi_inputs
                                    .iter()
                                    .find(|i| Some(&i.predecessor_block_id) == predecessor.as_ref())
                            })
                            .ok_or_else(|| Error::Missing(format!("PHI input {}", ins.id)))?;
                        lookup(&incoming, &input.value_id)?
                    }
                    "LOAD" => {
                        let id = &ins.operand_value_ids[0];
                        let v = p.values.get(id).ok_or_else(|| Error::Missing(id.clone()))?;
                        if v.role != "GLOBAL" {
                            return Err(Error::Unsupported(format!("non-global LOAD {}", ins.id)));
                        }
                        let name = p.symbols.get(&v.symbol_id).unwrap_or(&v.symbol_id);
                        self.globals
                            .get(name)
                            .cloned()
                            .ok_or_else(|| Error::Missing(name.clone()))?
                    }
                    "CALL" => {
                        if !matches!(details, Some(Details::Call(c)) if c.dispatch_kind == "STATIC")
                        {
                            return Err(Error::Unsupported(format!("unresolved CALL {}", ins.id)));
                        }
                        let callee_value = p
                            .values
                            .get(&ins.operand_value_ids[0])
                            .ok_or_else(|| Error::Missing(ins.id.clone()))?;
                        let name = p
                            .symbols
                            .get(&callee_value.symbol_id)
                            .unwrap_or(&callee_value.symbol_id);
                        let callee = if name == &p.function.id {
                            p
                        } else {
                            self.functions
                                .get(name)
                                .ok_or_else(|| Error::Missing(name.clone()))?
                        };
                        let args = ins.operand_value_ids[1..]
                            .iter()
                            .map(|id| lookup(&env, id))
                            .collect::<Result<Vec<_>, _>>()?;
                        let mut returns = self.execute(callee, &args, depth + 1)?;
                        if returns.len() == 1 {
                            returns.remove(0)
                        } else {
                            Value::Tuple(returns)
                        }
                    }
                    "EXTRACT_FIELD" => {
                        let Some(Details::Field(field)) = details else {
                            return Err(Error::Type(ins.id.clone()));
                        };
                        let Value::Tuple(values) = lookup(&env, &ins.operand_value_ids[0])? else {
                            return Err(Error::Type(ins.id.clone()));
                        };
                        values
                            .get(field.field_index as usize)
                            .cloned()
                            .ok_or_else(|| Error::Type(ins.id.clone()))?
                    }
                    "COPY" => lookup(&env, &ins.operand_value_ids[0])?,
                    "SELECT" => {
                        let Value::Bool(b) = lookup(&env, &ins.operand_value_ids[0])? else {
                            return Err(Error::Type(ins.id.clone()));
                        };
                        lookup(&env, &ins.operand_value_ids[if b { 1 } else { 2 }])?
                    }
                    op => {
                        let operands = ins
                            .operand_value_ids
                            .iter()
                            .map(|id| lookup(&env, id))
                            .collect::<Result<Vec<_>, _>>()?;
                        evaluate(op, &operands)?
                    }
                };
                if !ins.result_value_id.is_empty() {
                    self.trace
                        .values
                        .push((ins.result_value_id.clone(), result.clone()));
                    env.insert(ins.result_value_id.clone(), result);
                }
            }
            predecessor = Some(block_id);
            block_id = next.ok_or_else(|| Error::Missing("block terminator".into()))?;
        }
    }
}

fn evaluate(op: &str, args: &[Value]) -> Result<Value, Error> {
    if matches!(op, "EQ" | "NE") && args.len() == 2 {
        return Ok(Value::Bool((args[0] == args[1]) == (op == "EQ")));
    }
    if let [Value::Bool(b)] = args {
        if op == "LOGICAL_NOT" {
            return Ok(Value::Bool(!b));
        }
    }
    if let [Value::Bits { value, signed }] = args {
        let result = match op {
            "NEG" => -*value,
            "BIT_NOT" => value.bit_not(),
            _ => return Err(Error::Unsupported(op.into())),
        };
        return Ok(Value::Bits {
            value: result,
            signed: *signed,
        });
    }
    if let [Value::Bits { value: a, signed }, Value::Bits {
        value: b,
        signed: bs,
    }] = args
    {
        let error = |e| Error::Arithmetic(format!("{e:?}"));
        let comparison = match op {
            "LT" => Some(a.lt(*b, *signed).map_err(error)?),
            "LE" => Some(!b.lt(*a, *signed).map_err(error)?),
            "GT" => Some(b.lt(*a, *signed).map_err(error)?),
            "GE" => Some(!a.lt(*b, *signed).map_err(error)?),
            _ => None,
        };
        if let Some(b) = comparison {
            return Ok(Value::Bool(b));
        }
        let value = match op {
            "SHL" | "SHR" => a
                .shift(*b, *bs, op == "SHR", op == "SHR" && *signed)
                .map_err(error)?,
            "ADD" | "SUB" | "MUL" | "DIV" | "REM" | "BIT_AND" | "BIT_OR" | "XOR" => {
                a.binary(op, *b, *signed).map_err(error)?
            }
            _ => return Err(Error::Unsupported(op.into())),
        };
        return Ok(Value::Bits {
            value,
            signed: *signed,
        });
    }
    Err(Error::Unsupported(op.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::Block;

    fn increment() -> Program {
        let int = proto::TypeDef {
            id: "int".into(),
            kind: "BITVECTOR".into(),
            bit_width: 8,
            is_signed: true,
            ..Default::default()
        };
        let parameter = proto::ValueDef {
            id: "p".into(),
            type_id: "int".into(),
            role: "PARAM".into(),
            ..Default::default()
        };
        let one = proto::ValueDef {
            id: "one".into(),
            type_id: "int".into(),
            role: "CONST".into(),
            literal: Some(proto::LiteralValue {
                representation: Some(proto::literal_value::Representation::MachineBitsBe(vec![1])),
            }),
            ..Default::default()
        };
        Program {
            function: proto::FunctionDef {
                id: "increment".into(),
                entry_block_id: "entry".into(),
                parameter_value_ids: vec!["p".into()],
                ..Default::default()
            },
            types: BTreeMap::from([("int".into(), int)]),
            values: BTreeMap::from([("p".into(), parameter), ("one".into(), one)]),
            blocks: BTreeMap::from([(
                "entry".into(),
                Block {
                    ordinal: 0,
                    edges: vec![],
                    instructions: vec![
                        proto::InstructionDef {
                            id: "add".into(),
                            opcode: "ADD".into(),
                            result_value_id: "sum".into(),
                            operand_value_ids: vec!["p".into(), "one".into()],
                            ..Default::default()
                        },
                        proto::InstructionDef {
                            id: "return".into(),
                            opcode: "RETURN".into(),
                            operand_value_ids: vec!["sum".into()],
                            is_terminator: true,
                            ..Default::default()
                        },
                    ],
                },
            )]),
            symbols: BTreeMap::new(),
        }
    }

    #[test]
    fn concrete_wraps_and_is_deterministic() {
        let program = increment();
        let execute = || {
            run(
                &program,
                &BTreeMap::new(),
                &[Value::integer(8, true, 127).unwrap()],
                &BTreeMap::new(),
                &Limits::default(),
            )
            .unwrap()
        };
        let first = execute();
        assert_eq!(first, execute());
        assert_eq!(first.0, vec![Value::integer(8, true, 128).unwrap()]);
    }

    #[test]
    fn concrete_step_limit_is_explicit() {
        assert_eq!(
            run(
                &increment(),
                &BTreeMap::new(),
                &[Value::integer(8, true, 0).unwrap()],
                &BTreeMap::new(),
                &Limits {
                    max_steps: 1,
                    max_recursion_depth: 1
                }
            ),
            Err(Error::StepLimit)
        );
    }

    #[test]
    fn concrete_unknown_opcode_is_not_skipped() {
        let mut program = increment();
        program.blocks.get_mut("entry").unwrap().instructions[0].opcode = "OPAQUE_OP".into();
        assert_eq!(
            run(
                &program,
                &BTreeMap::new(),
                &[Value::integer(8, true, 0).unwrap()],
                &BTreeMap::new(),
                &Limits::default()
            ),
            Err(Error::Unsupported("OPAQUE_OP".into()))
        );
    }

    fn factorial() -> Program {
        use crate::program::Edge;
        let mut p = increment();
        p.function.id = "fact".into();
        p.values.insert(
            "callee".into(),
            proto::ValueDef {
                id: "callee".into(),
                role: "GLOBAL".into(),
                symbol_id: "fact".into(),
                ..Default::default()
            },
        );
        let instruction = |id: &str, op: &str, result: &str, args: &[&str]| proto::InstructionDef {
            id: id.into(),
            opcode: op.into(),
            result_value_id: result.into(),
            operand_value_ids: args.iter().map(|a| (*a).into()).collect(),
            ..Default::default()
        };
        let entry = p.blocks.get_mut("entry").unwrap();
        entry.instructions = vec![
            instruction("compare", "LE", "condition", &["p", "one"]),
            instruction("branch", "BRANCH", "", &["condition"]),
        ];
        entry.edges = ["base", "recurse"]
            .into_iter()
            .enumerate()
            .map(|(ordinal, target)| Edge {
                definition: proto::ControlEdgeDef {
                    target_block_id: target.into(),
                    outcome_ordinal: ordinal as u32,
                    edge_kind: if ordinal == 0 {
                        "BRANCH_TRUE"
                    } else {
                        "BRANCH_FALSE"
                    }
                    .into(),
                    ..Default::default()
                },
                outcome_id: None,
            })
            .collect();
        p.blocks.insert(
            "base".into(),
            Block {
                ordinal: 1,
                edges: vec![],
                instructions: vec![instruction("base-return", "RETURN", "", &["one"])],
            },
        );
        let mut call = instruction("call", "CALL", "recursive", &["callee", "smaller"]);
        call.payload = Some(proto::OperationPayload {
            details: Some(Details::Call(proto::CallSemantics {
                dispatch_kind: "STATIC".into(),
                ..Default::default()
            })),
            ..Default::default()
        });
        p.blocks.insert(
            "recurse".into(),
            Block {
                ordinal: 2,
                edges: vec![],
                instructions: vec![
                    instruction("subtract", "SUB", "smaller", &["p", "one"]),
                    call,
                    instruction("multiply", "MUL", "product", &["p", "recursive"]),
                    instruction("recursive-return", "RETURN", "", &["product"]),
                ],
            },
        );
        p
    }

    #[test]
    fn concrete_factorial_calls_and_branches() {
        let p = factorial();
        for (n, expected) in [1, 1, 2, 6, 24, 120, 720].into_iter().enumerate() {
            let (returns, _) = run(
                &p,
                &BTreeMap::new(),
                &[Value::integer(8, true, n as u128).unwrap()],
                &BTreeMap::new(),
                &Limits::default(),
            )
            .unwrap();
            // This program deliberately uses int8, including wrap at 6!.
            assert_eq!(returns, vec![Value::integer(8, true, expected).unwrap()]);
        }
    }

    #[test]
    fn concrete_recursion_limit_is_explicit() {
        assert_eq!(
            run(
                &factorial(),
                &BTreeMap::new(),
                &[Value::integer(8, true, 6).unwrap()],
                &BTreeMap::new(),
                &Limits {
                    max_steps: 100,
                    max_recursion_depth: 2
                }
            ),
            Err(Error::RecursionLimit)
        );
    }

    #[test]
    fn concrete_global_and_multiple_returns_are_preserved() {
        let mut callee = increment();
        callee.function.id = "pair".into();
        callee.values.insert(
            "global".into(),
            proto::ValueDef {
                id: "global".into(),
                role: "GLOBAL".into(),
                symbol_id: "ErrZero".into(),
                ..Default::default()
            },
        );
        let body = &mut callee.blocks.get_mut("entry").unwrap().instructions;
        body[0] = proto::InstructionDef {
            id: "load".into(),
            opcode: "LOAD".into(),
            result_value_id: "error".into(),
            operand_value_ids: vec!["global".into()],
            ..Default::default()
        };
        body[1].operand_value_ids = vec!["p".into(), "error".into()];

        let mut caller = increment();
        caller.values.insert(
            "function".into(),
            proto::ValueDef {
                id: "function".into(),
                role: "GLOBAL".into(),
                symbol_id: "pair".into(),
                ..Default::default()
            },
        );
        caller.blocks.get_mut("entry").unwrap().instructions = vec![
            proto::InstructionDef {
                id: "call".into(),
                opcode: "CALL".into(),
                result_value_id: "tuple".into(),
                operand_value_ids: vec!["function".into(), "p".into()],
                payload: Some(proto::OperationPayload {
                    details: Some(Details::Call(proto::CallSemantics {
                        dispatch_kind: "STATIC".into(),
                        ..Default::default()
                    })),
                    ..Default::default()
                }),
                ..Default::default()
            },
            proto::InstructionDef {
                id: "extract".into(),
                opcode: "EXTRACT_FIELD".into(),
                result_value_id: "second".into(),
                operand_value_ids: vec!["tuple".into()],
                payload: Some(proto::OperationPayload {
                    details: Some(Details::Field(proto::FieldAccess {
                        field_index: 1,
                        ..Default::default()
                    })),
                    ..Default::default()
                }),
                ..Default::default()
            },
            proto::InstructionDef {
                id: "return".into(),
                opcode: "RETURN".into(),
                operand_value_ids: vec!["second".into()],
                ..Default::default()
            },
        ];
        let globals = BTreeMap::from([("ErrZero".into(), Value::Object("error-identity".into()))]);
        let functions = BTreeMap::from([("pair".into(), callee)]);
        assert_eq!(
            run(
                &caller,
                &functions,
                &[Value::integer(8, true, 7).unwrap()],
                &globals,
                &Limits::default()
            )
            .unwrap()
            .0,
            vec![Value::Object("error-identity".into())]
        );
        assert_eq!(
            run(
                &caller,
                &functions,
                &[Value::integer(8, true, 7).unwrap()],
                &BTreeMap::new(),
                &Limits::default()
            ),
            Err(Error::Missing("ErrZero".into()))
        );
    }

    #[test]
    fn concrete_factorial_matches_reference_on_200_seeded_inputs() {
        let mut p = factorial();
        p.types.get_mut("int").unwrap().bit_width = 64;
        let mut seed = 0xc0ffee_u64;
        for _ in 0..200 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let n = (seed % 31) as i64 - 10;
            // Fact's source returns 1 for all n <= 1, including negatives.
            let expected = (2..=n).product::<i64>();
            let (returns, _) = run(
                &p,
                &BTreeMap::new(),
                &[Value::integer(64, true, n as u128).unwrap()],
                &BTreeMap::new(),
                &Limits::default(),
            )
            .unwrap();
            assert_eq!(
                returns,
                vec![Value::integer(64, true, expected as u128).unwrap()],
                "n={n}"
            );
        }
    }
}
