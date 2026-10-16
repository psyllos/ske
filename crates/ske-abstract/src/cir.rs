//! Flow-insensitive SSA interval equations. Ignoring control guards can lose
//! precision, but never excludes a concrete execution. PHIs join every input.
use crate::{add, full_range, mul, sub, IntType, Interval};
use ske_cir::proto;
use std::collections::BTreeMap;

/// Compute integer invariants with Kleene widening and one simultaneous
/// narrowing pass. Unsupported transfers yield the full machine-type range.
pub fn infer(
    types: &BTreeMap<String, proto::TypeDef>,
    values: &BTreeMap<String, proto::ValueDef>,
    instructions: &[proto::InstructionDef],
) -> Result<BTreeMap<String, Interval>, String> {
    let mut sorts = BTreeMap::new();
    let mut state = BTreeMap::new();
    for (id, value) in values {
        let ty = types
            .get(&value.type_id)
            .ok_or("missing interval value type")?;
        if ty.kind != "BITVECTOR" {
            continue;
        }
        let sort = IntType::bitvec(ty.bit_width, ty.is_signed)?;
        let initial = if value.role == "CONST" {
            match value
                .literal
                .as_ref()
                .and_then(|l| l.representation.as_ref())
            {
                Some(proto::literal_value::Representation::MachineBitsBe(bytes))
                    if bytes.len() <= 16 =>
                {
                    Interval::constant(
                        sort,
                        bytes.iter().fold(0_u128, |n, b| (n << 8) | u128::from(*b)) as i128,
                    )
                }
                _ => full_range(sort),
            }
        } else if value.role == "SSA" {
            Interval::Bottom
        } else {
            full_range(sort)
        };
        sorts.insert(id.clone(), sort);
        state.insert(id.clone(), initial);
    }
    let transfer = |ins: &proto::InstructionDef, env: &BTreeMap<String, Interval>, ty| {
        let operand = |id: &str| env.get(id).copied().unwrap_or_else(|| full_range(ty));
        match ins.opcode.as_str() {
            "PHI" => ins
                .payload
                .as_ref()
                .map(|p| {
                    p.phi_inputs
                        .iter()
                        .fold(Interval::Bottom, |acc, i| acc.join(&operand(&i.value_id)))
                })
                .unwrap_or_else(|| full_range(ty)),
            "COPY" => ins
                .operand_value_ids
                .first()
                .map(|id| operand(id))
                .unwrap_or_else(|| full_range(ty)),
            "ADD" | "SUB" | "MUL" if ins.operand_value_ids.len() == 2 => {
                let a = operand(&ins.operand_value_ids[0]);
                let b = operand(&ins.operand_value_ids[1]);
                match ins.opcode.as_str() {
                    "ADD" => add(ty, a, b),
                    "SUB" => sub(ty, a, b),
                    _ => mul(ty, a, b),
                }
            }
            _ => full_range(ty),
        }
    };
    // Each widening can move each bound outward only once to its type bound.
    // A generous finite limit defends malformed cyclic equations as well.
    let limit = state.len().saturating_mul(4).saturating_add(1);
    for _ in 0..limit {
        let mut next = state.clone();
        for ins in instructions {
            if let Some(ty) = sorts.get(&ins.result_value_id) {
                let old = state[&ins.result_value_id];
                let image = transfer(ins, &state, *ty);
                next.insert(
                    ins.result_value_id.clone(),
                    old.widen(&old.join(&image), *ty),
                );
            }
        }
        if next == state {
            let mut narrowed = state.clone();
            for ins in instructions {
                if let Some(ty) = sorts.get(&ins.result_value_id) {
                    let image = transfer(ins, &state, *ty);
                    narrowed.insert(
                        ins.result_value_id.clone(),
                        state[&ins.result_value_id].narrow(&image, *ty),
                    );
                }
            }
            return Ok(narrowed);
        }
        state = next;
    }
    Ok(sorts
        .into_iter()
        .map(|(id, ty)| (id, full_range(ty)))
        .collect())
}
