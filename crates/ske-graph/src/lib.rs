//! Deterministic indexed graphs and Cooper–Harvey–Kennedy dominance.
//!
//! The caller maps its domain IDs to dense indices. Sorted adjacency makes all
//! traversals independent of insertion order. Unreachable vertices have no
//! immediate dominator; only reachable edges can be classified as back edges.
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct Graph {
    successors: Vec<Vec<usize>>,
    predecessors: Vec<Vec<usize>>,
}

#[derive(Clone, Debug)]
pub struct Dominators {
    pub entry: usize,
    /// The entry dominates itself; unreachable nodes have `None`.
    pub immediate: Vec<Option<usize>>,
}

impl Graph {
    /// Invalid indices are rejected rather than silently omitted.
    pub fn new(
        node_count: usize,
        edges: impl IntoIterator<Item = (usize, usize)>,
    ) -> Result<Self, String> {
        let mut successors = vec![BTreeSet::new(); node_count];
        let mut predecessors = vec![BTreeSet::new(); node_count];
        for (source, target) in edges {
            if source >= node_count || target >= node_count {
                return Err(format!(
                    "edge {source}->{target} outside graph of {node_count} nodes"
                ));
            }
            successors[source].insert(target);
            predecessors[target].insert(source);
        }
        Ok(Self {
            successors: successors
                .into_iter()
                .map(|s| s.into_iter().collect())
                .collect(),
            predecessors: predecessors
                .into_iter()
                .map(|s| s.into_iter().collect())
                .collect(),
        })
    }

    pub fn node_count(&self) -> usize {
        self.successors.len()
    }

    pub fn successors(&self, node: usize) -> &[usize] {
        &self.successors[node]
    }

    pub fn predecessors(&self, node: usize) -> &[usize] {
        &self.predecessors[node]
    }

    /// Iterative DFS, avoiding stack overflow on large compilation units.
    pub fn dfs(&self, entry: usize) -> Vec<usize> {
        if entry >= self.node_count() {
            return Vec::new();
        }
        let mut visited = vec![false; self.node_count()];
        let mut stack = vec![entry];
        let mut order = Vec::new();
        while let Some(node) = stack.pop() {
            if std::mem::replace(&mut visited[node], true) {
                continue;
            }
            order.push(node);
            stack.extend(self.successors[node].iter().rev().copied());
        }
        order
    }

    fn postorder(&self, roots: impl IntoIterator<Item = usize>) -> Vec<usize> {
        let mut seen = vec![false; self.node_count()];
        let mut order = Vec::new();
        for root in roots {
            if root >= self.node_count() || seen[root] {
                continue;
            }
            seen[root] = true;
            let mut stack = vec![(root, 0)];
            while let Some((node, next)) = stack.last_mut() {
                if *next < self.successors[*node].len() {
                    let target = self.successors[*node][*next];
                    *next += 1;
                    if !seen[target] {
                        seen[target] = true;
                        stack.push((target, 0));
                    }
                } else {
                    order.push(*node);
                    stack.pop();
                }
            }
        }
        order
    }

    pub fn dominators(&self, entry: usize) -> Dominators {
        let mut immediate = vec![None; self.node_count()];
        if entry >= self.node_count() {
            return Dominators { entry, immediate };
        }
        let mut rpo = self.postorder([entry]);
        rpo.reverse();
        let mut rank = vec![usize::MAX; self.node_count()];
        for (index, &node) in rpo.iter().enumerate() {
            rank[node] = index;
        }
        immediate[entry] = Some(entry);
        // Cooper, Harvey, Kennedy: intersect already-known idom chains in RPO.
        let mut changed = true;
        while changed {
            changed = false;
            for &node in rpo.iter().skip(1) {
                let mut known = self.predecessors[node]
                    .iter()
                    .copied()
                    .filter(|&p| immediate[p].is_some());
                let Some(mut candidate) = known.next() else {
                    continue;
                };
                for mut other in known {
                    while candidate != other {
                        while rank[candidate] > rank[other] {
                            candidate = immediate[candidate].expect("known dominator chain");
                        }
                        while rank[other] > rank[candidate] {
                            other = immediate[other].expect("known dominator chain");
                        }
                    }
                }
                if immediate[node] != Some(candidate) {
                    immediate[node] = Some(candidate);
                    changed = true;
                }
            }
        }
        Dominators { entry, immediate }
    }

    /// An edge (u,v) is a back edge exactly when v dominates reachable u.
    pub fn back_edges(&self, entry: usize) -> Vec<(usize, usize)> {
        let dominators = self.dominators(entry);
        self.successors
            .iter()
            .enumerate()
            .flat_map(|(source, targets)| {
                let dominators = &dominators;
                targets.iter().filter_map(move |&target| {
                    (dominators.immediate[source].is_some() && dominators.dominates(target, source))
                        .then_some((source, target))
                })
            })
            .collect()
    }

    /// Kosaraju SCC, both passes iterative. Components and members are sorted.
    pub fn strongly_connected_components(&self) -> Vec<Vec<usize>> {
        let order = self.postorder(0..self.node_count());
        let mut seen = vec![false; self.node_count()];
        let mut components = Vec::new();
        for &root in order.iter().rev() {
            if seen[root] {
                continue;
            }
            let mut stack = vec![root];
            let mut component = Vec::new();
            seen[root] = true;
            while let Some(node) = stack.pop() {
                component.push(node);
                for &pred in self.predecessors[node].iter().rev() {
                    if !seen[pred] {
                        seen[pred] = true;
                        stack.push(pred);
                    }
                }
            }
            component.sort_unstable();
            components.push(component);
        }
        components.sort();
        components
    }
}

impl Dominators {
    pub fn dominates(&self, dominator: usize, node: usize) -> bool {
        if dominator >= self.immediate.len()
            || node >= self.immediate.len()
            || self.immediate[node].is_none()
        {
            return false;
        }
        let mut current = node;
        loop {
            if current == dominator {
                return true;
            }
            let Some(parent) = self.immediate[current] else {
                return false;
            };
            if current == parent {
                return false;
            }
            current = parent;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dominance_diamond() {
        let graph = Graph::new(4, [(0, 1), (0, 2), (1, 3), (2, 3)]).unwrap();
        assert_eq!(
            graph.dominators(0).immediate,
            vec![Some(0), Some(0), Some(0), Some(0)]
        );
        assert!(!graph.dominators(0).dominates(1, 3));
    }

    #[test]
    fn dominance_simple_loop() {
        let graph = Graph::new(4, [(0, 1), (1, 2), (2, 1), (1, 3)]).unwrap();
        assert_eq!(
            graph.dominators(0).immediate,
            vec![Some(0), Some(0), Some(1), Some(1)]
        );
        assert_eq!(graph.back_edges(0), [(2, 1)]);
    }

    #[test]
    fn dominance_nested_loops() {
        let graph =
            Graph::new(6, [(0, 1), (1, 2), (2, 3), (3, 2), (3, 4), (4, 1), (1, 5)]).unwrap();
        assert_eq!(
            graph.dominators(0).immediate,
            vec![Some(0), Some(0), Some(1), Some(2), Some(3), Some(1)]
        );
        assert_eq!(graph.back_edges(0), [(3, 2), (4, 1)]);
    }

    #[test]
    fn dominance_irreducible() {
        let graph =
            Graph::new(5, [(0, 1), (0, 2), (1, 3), (2, 3), (3, 1), (3, 2), (3, 4)]).unwrap();
        assert_eq!(
            graph.dominators(0).immediate,
            vec![Some(0), Some(0), Some(0), Some(0), Some(3)]
        );
        assert!(graph.back_edges(0).is_empty());
    }

    #[test]
    fn dominance_unreachable() {
        let graph = Graph::new(4, [(0, 1), (2, 1), (2, 3), (3, 2)]).unwrap();
        assert_eq!(
            graph.dominators(0).immediate,
            vec![Some(0), Some(0), None, None]
        );
        assert!(!graph.dominators(0).dominates(2, 3));
        assert!(!graph.dominators(0).dominates(2, 2));
    }

    #[test]
    fn traversals_are_deterministic() {
        let graph = Graph::new(6, [(3, 4), (0, 2), (2, 1), (0, 1), (1, 2), (4, 3)]).unwrap();
        assert_eq!(graph.dfs(0), [0, 1, 2]);
        assert_eq!(
            graph.strongly_connected_components(),
            [vec![0], vec![1, 2], vec![3, 4], vec![5]]
        );
        assert!(Graph::new(1, [(0, 1)]).is_err());
        assert!(graph.dfs(9).is_empty());
    }

    #[test]
    fn chk_matches_independent_vertex_removal_oracle() {
        // Every directed graph on four vertices (excluding self-loops).
        // Independent definition: d dominates reachable n iff deleting d makes
        // n unreachable. This does not reuse the idom algorithm as its oracle.
        let possible: Vec<_> = (0..4)
            .flat_map(|s| (0..4).filter(move |&t| t != s).map(move |t| (s, t)))
            .collect();
        for mask in 0..(1_u32 << possible.len()) {
            let edges = possible
                .iter()
                .enumerate()
                .filter_map(|(bit, &edge)| (mask & (1 << bit) != 0).then_some(edge));
            let graph = Graph::new(4, edges).unwrap();
            for entry in 0..4 {
                let dom = graph.dominators(entry);
                let reachable = graph.dfs(entry);
                for removed in 0..4 {
                    let mut remaining = [false; 4];
                    let mut stack = if entry == removed {
                        vec![]
                    } else {
                        vec![entry]
                    };
                    while let Some(node) = stack.pop() {
                        if std::mem::replace(&mut remaining[node], true) {
                            continue;
                        }
                        stack.extend(
                            graph
                                .successors(node)
                                .iter()
                                .copied()
                                .filter(|&t| t != removed),
                        );
                    }
                    for (node, &still_reachable) in remaining.iter().enumerate() {
                        assert_eq!(
                            dom.dominates(removed, node),
                            reachable.contains(&node) && !still_reachable,
                            "mask={mask}, entry={entry}, dominator={removed}, node={node}"
                        );
                    }
                }
            }
        }
    }
}
