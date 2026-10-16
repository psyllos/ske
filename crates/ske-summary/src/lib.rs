//! Storage-independent summary computation and cache identity.
use ske_common::{canonical_json, sha256_hex};
use ske_solver::Solver;
use ske_symbolic::{
    engine::{self, Budget},
    program::Program,
    summary::FunctionSummary,
};
use std::collections::BTreeMap;

/// §7.1 cache inputs. Callee and model fingerprints are sorted independently
/// before hashing; serialization preserves field boundaries.
pub struct CacheKey<'a> {
    pub structural: &'a str,
    pub semantic_profile: &'a str,
    pub context: &'a str,
    pub callees: Vec<String>,
    pub models: Vec<String>,
    pub config: &'a str,
}

impl CacheKey<'_> {
    pub fn fingerprint(mut self) -> ske_common::Result<String> {
        self.callees.sort();
        self.models.sort();
        let data = serde_json::json!([
            self.structural,
            self.semantic_profile,
            self.context,
            self.callees,
            self.models,
            self.config
        ]);
        Ok(sha256_hex(canonical_json(&data)?))
    }
}

/// Compute one function after the caller has resolved and summarized callees.
/// SQL, target resolution and snapshot ownership remain in the application.
pub fn compute(
    program: &Program,
    budget: &Budget,
    solver: &mut dyn Solver,
    callees: &BTreeMap<String, FunctionSummary>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<FunctionSummary, String> {
    let report = engine::analyze_with_summaries(
        program,
        budget,
        solver,
        |_, _| Ok(vec![]),
        cancelled,
        &mut |_| {},
        Some(callees),
    )?;
    Ok(FunctionSummary::from_report(program, report))
}

/// Storage-free bounded composition for a closed set of CIR bodies. Unknown
/// or open dispatch sites are left to the engine's havoc semantics.
pub fn compute_bounded(
    entry: &str,
    programs: &BTreeMap<String, Program>,
    budget: &Budget,
    solver: &mut dyn Solver,
) -> Result<FunctionSummary, String> {
    fn visit(
        id: &str,
        programs: &BTreeMap<String, Program>,
        budget: &Budget,
        solver: &mut dyn Solver,
        active: &mut BTreeMap<String, u32>,
        depth: u32,
    ) -> Result<FunctionSummary, String> {
        let p = programs.get(id).ok_or("summary body missing")?;
        let count = active.get(id).copied().unwrap_or(0);
        if count > budget.max_recursion_unfold {
            return FunctionSummary::recursion_residual(p);
        }
        if depth >= budget.max_call_depth {
            return Err("summary call depth exceeded".into());
        }
        active.insert(id.into(), count + 1);
        let mut callees = BTreeMap::new();
        for i in p
            .blocks
            .values()
            .flat_map(|b| &b.instructions)
            .filter(|i| i.opcode == "CALL")
        {
            if !matches!(i.payload.as_ref().and_then(|p| p.details.as_ref()),
                Some(ske_cir::proto::operation_payload::Details::Call(c)) if c.dispatch_kind == "STATIC" && c.world_closed)
            {
                continue;
            }
            let callee = i
                .operand_value_ids
                .first()
                .and_then(|id| p.values.get(id))
                .map(|v| p.symbols.get(&v.symbol_id).unwrap_or(&v.symbol_id));
            if let Some(callee) = callee.filter(|id| programs.contains_key(*id)) {
                callees.insert(
                    i.id.clone(),
                    visit(callee, programs, budget, solver, active, depth + 1)?,
                );
            }
        }
        let result = compute(p, budget, solver, &callees, &|| false);
        if count == 0 {
            active.remove(id);
        } else {
            active.insert(id.into(), count);
        }
        result
    }
    visit(entry, programs, budget, solver, &mut BTreeMap::new(), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_cache_key_sorts_dependencies_and_preserves_boundaries() {
        let key = |structural, callees| {
            CacheKey {
                structural,
                semantic_profile: "go64",
                context: "default",
                callees,
                models: vec![],
                config: "config",
            }
            .fingerprint()
            .unwrap()
        };
        assert_eq!(
            key("body", vec!["b".into(), "a".into()]),
            key("body", vec!["a".into(), "b".into()])
        );
        assert_ne!(
            key("body", vec!["ab".into(), "c".into()]),
            key("body", vec!["a".into(), "bc".into()])
        );
        assert_ne!(key("body", vec![]), key("other", vec![]));
    }
}
