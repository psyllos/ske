//! Storage-independent, already validated CIR input to symbolic execution.
use ske_cir::proto;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct Program {
    pub function: proto::FunctionDef,
    pub types: BTreeMap<String, proto::TypeDef>,
    pub values: BTreeMap<String, proto::ValueDef>,
    pub blocks: BTreeMap<String, Block>,
    /// Symbol ID to qualified name, including globals and callees.
    pub symbols: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub ordinal: u32,
    pub instructions: Vec<proto::InstructionDef>,
    pub edges: Vec<Edge>,
}

#[derive(Clone, Debug)]
pub struct Edge {
    pub definition: proto::ControlEdgeDef,
    /// None for unconditional control flow; otherwise the persistence identity.
    pub outcome_id: Option<String>,
}

impl Program {
    pub fn outcomes(&self) -> BTreeSet<String> {
        self.blocks
            .values()
            .flat_map(|b| &b.edges)
            .filter_map(|e| e.outcome_id.clone())
            .collect()
    }

    /// DFS retreating edges, not dominance back edges (also handles irreducible CFGs).
    pub fn retreating_edges(&self) -> BTreeSet<String> {
        fn visit(
            p: &Program,
            id: &str,
            seen: &mut BTreeSet<String>,
            active: &mut BTreeSet<String>,
            result: &mut BTreeSet<String>,
        ) {
            if !seen.insert(id.to_owned()) {
                return;
            }
            active.insert(id.to_owned());
            if let Some(block) = p.blocks.get(id) {
                let mut edges: Vec<_> = block.edges.iter().collect();
                edges.sort_by_key(|e| e.definition.outcome_ordinal);
                for edge in edges {
                    let target = &edge.definition.target_block_id;
                    if active.contains(target) {
                        result.insert(edge.definition.id.clone());
                    } else {
                        visit(p, target, seen, active, result);
                    }
                }
            }
            active.remove(id);
        }
        let mut result = BTreeSet::new();
        visit(
            self,
            &self.function.entry_block_id,
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
            &mut result,
        );
        result
    }
}
