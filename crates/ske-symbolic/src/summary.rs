//! Pure summary data and memoized, cross-arena DAG substitution.
//!
//! Expression IDs are local to a summary's arena. Applying a summary must
//! import its DAG, not reuse its numeric IDs in the caller.
use crate::{
    engine::{Exit, Report, Soundness},
    expression::{ExprArena, ExprId, ExprKind, Node},
    program::Program,
};
use std::collections::BTreeMap;

/// Summary precision is distinct from the engine's model soundness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    Exact,
    Overapprox,
    Partial,
    Unknown,
}

pub struct FunctionSummary {
    pub function_id: String,
    pub parameter_symbols: Vec<String>,
    pub arena: ExprArena,
    pub cases: Vec<Exit>,
    pub precision: Precision,
    pub complete: bool,
    /// Exhaustive case coverage may survive a residual recursion abstraction,
    /// unlike a worklist budget cut. Only exhaustive cases may be applied.
    pub covers_inputs: bool,
    pub residual: bool,
    pub assumptions: Vec<String>,
    pub unknowns: Vec<String>,
    /// Scalar bodies and explicit global reads/writes can be composed.
    /// Other effects remain opaque and are never silently omitted.
    pub composable: bool,
    pub global_reads: Vec<(String, String, crate::Type)>,
}

impl FunctionSummary {
    pub fn from_report(program: &Program, report: Report) -> Self {
        let global_reads: Vec<_> = report
            .global_reads
            .iter()
            .filter_map(|(id, global)| {
                let node = report.arena.get(*id)?;
                if let ExprKind::Symbol(name) = &node.kind {
                    Some((name.clone(), global.clone(), node.ty.clone()))
                } else {
                    None
                }
            })
            .collect();
        let composable = global_reads.len() == report.global_reads.len()
            && program
                .blocks
                .values()
                .flat_map(|b| &b.instructions)
                .all(|i| {
                    (i.opcode == "CALL" && report.composed_calls.contains(&i.id))
                        || (i.opcode == "EXTRACT_FIELD" && report.tuple_extractions.contains(&i.id))
                        || (matches!(i.opcode.as_str(), "LOAD" | "STORE")
                            && i.operand_value_ids
                                .first()
                                .and_then(|id| program.values.get(id))
                                .is_some_and(|v| v.role == "GLOBAL"))
                        || matches!(
                            i.opcode.as_str(),
                            "RETURN"
                                | "BRANCH"
                                | "JUMP"
                                | "SWITCH"
                                | "PHI"
                                | "COPY"
                                | "ADD"
                                | "SUB"
                                | "MUL"
                                | "DIV"
                                | "REM"
                                | "EQ"
                                | "NE"
                                | "LT"
                                | "LE"
                                | "GT"
                                | "GE"
                                | "BIT_AND"
                                | "BIT_OR"
                                | "XOR"
                                | "SHL"
                                | "SHR"
                                | "NEG"
                                | "BIT_NOT"
                                | "LOGICAL_NOT"
                                | "SELECT"
                                | "SIGN_EXTEND"
                                | "ZERO_EXTEND"
                                | "TRUNCATE"
                                | "BITCAST"
                        )
                });
        let precision = if !report.exploration_closed {
            Precision::Partial
        } else if composable && report.exits.iter().all(|e| e.soundness == Soundness::Exact) {
            Precision::Exact
        } else {
            Precision::Overapprox
        };
        let unknowns = report.diagnostics.iter().cloned().collect();
        let covers_inputs = report.cut_count == 0
            && !matches!(
                report.status,
                crate::engine::RunStatus::Cancelled | crate::engine::RunStatus::Failed
            )
            && !report.diagnostics.contains("BUDGET_EXHAUSTED")
            && !report.diagnostics.contains("UNSUPPORTED_TERMINATOR")
            && !report.diagnostics.contains("LOOP_BOUND_REACHED");
        let mut unknowns: Vec<String> = unknowns;
        unknowns.extend(report.summary_unknowns);
        unknowns.sort();
        unknowns.dedup();
        if report.exits.iter().any(|e| e.partial) {
            unknowns.push("RECURSION_BOUND".into());
        }
        Self {
            function_id: program.function.id.clone(),
            parameter_symbols: (0..program.function.parameter_value_ids.len())
                .map(|index| format!("param:{}:{index}", program.function.id))
                .collect(),
            arena: report.arena,
            cases: report.exits,
            precision,
            complete: report.exploration_closed,
            covers_inputs,
            residual: false,
            assumptions: report.assumptions.into_iter().collect(),
            unknowns,
            composable,
            global_reads,
        }
    }

    /// Residual return of a bounded recursive activation. This is exhaustive
    /// but not complete/exact: the caller must retain a PARTIAL case.
    pub fn recursion_residual(program: &Program) -> Result<Self, String> {
        let mut arena = ExprArena::new();
        let values = program
            .function
            .return_type_ids
            .iter()
            .enumerate()
            .map(|(index, ty)| {
                arena
                    .havoc(crate::engine::expression_type(program, ty)?, index as u64)
                    .map_err(|e| format!("{e:?}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            function_id: program.function.id.clone(),
            parameter_symbols: (0..program.function.parameter_value_ids.len())
                .map(|index| format!("param:{}:{index}", program.function.id))
                .collect(),
            arena,
            cases: vec![Exit {
                kind: "RETURN".into(),
                instruction_id: String::new(),
                constraints: vec![],
                soundness: Soundness::Overapprox,
                taken_outcomes: crate::engine::Path(None),
                values,
                partial: true,
                memory_delta: vec![],
                effects: vec![format!("UNKNOWN_RECURSION_EFFECT:{}", program.function.id)],
            }],
            precision: Precision::Partial,
            complete: false,
            covers_inputs: true,
            residual: true,
            assumptions: vec![],
            unknowns: vec!["RECURSION_BOUND".into()],
            global_reads: vec![],
            composable: program
                .blocks
                .values()
                .flat_map(|b| &b.instructions)
                .all(|i| {
                    if i.opcode == "CALL" {
                        return i
                            .operand_value_ids
                            .first()
                            .and_then(|id| program.values.get(id))
                            .is_some_and(|v| v.symbol_id == program.function.id)
                            && matches!(i.payload.as_ref().and_then(|p| p.details.as_ref()),
                            Some(ske_cir::proto::operation_payload::Details::Call(c))
                            if c.dispatch_kind == "STATIC" && c.world_closed);
                    }
                    matches!(
                        i.opcode.as_str(),
                        "RETURN"
                            | "BRANCH"
                            | "JUMP"
                            | "SWITCH"
                            | "PHI"
                            | "COPY"
                            | "ADD"
                            | "SUB"
                            | "MUL"
                            | "DIV"
                            | "REM"
                            | "EQ"
                            | "NE"
                            | "LT"
                            | "LE"
                            | "GT"
                            | "GE"
                            | "BIT_AND"
                            | "BIT_OR"
                            | "XOR"
                            | "SHL"
                            | "SHR"
                            | "NEG"
                            | "BIT_NOT"
                            | "LOGICAL_NOT"
                            | "SELECT"
                            | "SIGN_EXTEND"
                            | "ZERO_EXTEND"
                            | "TRUNCATE"
                            | "BITCAST"
                    )
                }),
        })
    }
}

/// Import a DAG into another arena, replacing parameters with caller arguments.
/// Fresh symbols/havocs are namespaced by application, not correlated across
/// independent calls. A single memo must be shared by guards and returns.
pub fn substitute(
    source: &ExprArena,
    target: &mut ExprArena,
    expression: ExprId,
    parameters: &BTreeMap<String, ExprId>,
    namespace: &str,
    memo: &mut BTreeMap<ExprId, ExprId>,
) -> Result<ExprId, String> {
    if let Some(id) = memo.get(&expression) {
        return Ok(*id);
    }
    let node = source.get(expression).ok_or("summary expression missing")?;
    if let ExprKind::Symbol(name) = &node.kind {
        if let Some(id) = parameters.get(name) {
            let ty = &target.get(*id).ok_or("summary argument missing")?.ty;
            if ty != &node.ty {
                return Err("summary parameter type mismatch".into());
            }
            memo.insert(expression, *id);
            return Ok(*id);
        }
    }
    let mut child = |id| substitute(source, target, id, parameters, namespace, memo);
    let kind = match &node.kind {
        ExprKind::Const(c) => ExprKind::Const(c.clone()),
        ExprKind::Symbol(name) => ExprKind::Symbol(format!("{namespace}:{name}")),
        ExprKind::Havoc(n) => ExprKind::Symbol(format!("{namespace}:havoc:{n}")),
        ExprKind::Unary { op, arg } => ExprKind::Unary {
            op: *op,
            arg: child(*arg)?,
        },
        ExprKind::Binary { op, left, right } => ExprKind::Binary {
            op: *op,
            left: child(*left)?,
            right: child(*right)?,
        },
        ExprKind::Ite {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ite {
            condition: child(*condition)?,
            then_expr: child(*then_expr)?,
            else_expr: child(*else_expr)?,
        },
        ExprKind::Field { base, index } => ExprKind::Field {
            base: child(*base)?,
            index: *index,
        },
        ExprKind::Uninterpreted { name, args } => ExprKind::Uninterpreted {
            name: name.clone(),
            args: args.iter().map(|id| child(*id)).collect::<Result<_, _>>()?,
        },
    };
    let result = target
        .intern(Node {
            ty: node.ty.clone(),
            kind,
        })
        .map_err(|e| format!("summary substitution: {e:?}"))?;
    memo.insert(expression, result);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BinaryOp, Constant, Sort, Type};

    #[test]
    fn summary_substitution_imports_shared_dag_without_id_aliasing() {
        let ty = Type {
            local_key: "i64".into(),
            sort: Sort::BitVec {
                width: 64,
                signed: true,
            },
        };
        let mut source = ExprArena::new();
        let x = source.symbol(ty.clone(), "x").unwrap();
        let two = source.constant(ty.clone(), Constant::Bits(2)).unwrap();
        let twice = source.binary(ty.clone(), BinaryOp::Mul, x, two).unwrap();
        let mut target = ExprArena::new();
        let _padding = target.constant(ty.clone(), Constant::Bits(99)).unwrap();
        let a = target.symbol(ty.clone(), "a").unwrap();
        let parameters = BTreeMap::from([("x".into(), a)]);
        let mut memo = BTreeMap::new();
        let result =
            substitute(&source, &mut target, twice, &parameters, "call0", &mut memo).unwrap();
        let expected_two = target.constant(ty.clone(), Constant::Bits(2)).unwrap();
        let expected = target.binary(ty, BinaryOp::Mul, a, expected_two).unwrap();
        assert_eq!(result, expected);
        let count = target.len();
        assert_eq!(
            substitute(&source, &mut target, twice, &parameters, "call0", &mut memo).unwrap(),
            result
        );
        assert_eq!(target.len(), count);
    }
}
