//! Static call graph queries and `ske paths explore` (T-0.4-01/07).
//! SQL is confined to `ske-query`; the CLI never touches the database.
use rusqlite::Connection;
use serde_json::{json, Value};
use ske_common::{Error, Result};
use std::collections::{BTreeMap, BTreeSet};

fn sql(e: rusqlite::Error) -> Error {
    Error::new("E_DB_INTEGRITY", e.to_string())
}

/// One call edge in the snapshot: caller function id, callsite id, callee
/// function id, and the call-site classification.
#[derive(Clone, Debug)]
pub struct CallEdge {
    pub caller_id: i64,
    pub callee_id: Option<i64>,
    pub caller: String,
    pub callsite: String,
    pub callee: String,
    pub dispatch: String,
    pub resolution: String,
    pub world_closed: bool,
    /// A call that cannot be expanded because its callee is unknown/external.
    pub incomplete: bool,
}

/// Load every resolved call edge of a snapshot (non-external callees only).
/// Incomplete sites (UNRESOLVED/EXTERNAL resolution, or DYNAMIC/REFLECTIVE/
/// UNKNOWN dispatch) are returned as edges with an empty callee and
/// `incomplete=true` so a traversal can stop and mark the result incomplete.
pub fn call_edges(db: &Connection, snapshot: i64) -> Result<Vec<CallEdge>> {
    let mut edges = Vec::new();
    let mut stmt = db
        .prepare(
            "SELECT bf.qualified_name AS caller_key,
                    i.local_key AS callsite,
                    cs.dispatch_kind, cs.resolution_status, cs.world_closed,
                    cf.qualified_name AS callee_key, bfn.id, cfn.id, u.unit_key, cfn.is_external
             FROM call_sites cs
             JOIN instructions i ON i.id = cs.instruction_id
             JOIN basic_blocks b ON b.id = i.block_id
             JOIN functions bfn ON bfn.id = b.function_id
             JOIN symbols bf ON bf.id = bfn.symbol_id
             JOIN compilation_units u ON u.id=bf.unit_id
             LEFT JOIN call_targets ct ON ct.call_site_id = cs.id
             LEFT JOIN functions cfn ON cfn.id = ct.target_function_id
             LEFT JOIN symbols cf ON cf.id = cfn.symbol_id
             WHERE bf.snapshot_id = ?1
             ORDER BY bf.qualified_name,u.unit_key,b.ordinal,i.ordinal,cf.qualified_name",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map([snapshot], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, Option<bool>>(9)?,
            ))
        })
        .map_err(sql)?;
    for row in rows {
        let (
            caller_key,
            callsite,
            dispatch,
            resolution,
            world_closed,
            callee_key,
            caller_id,
            callee_id,
            unit,
            external,
        ) = row.map_err(sql)?;
        let incomplete = matches!(resolution.as_str(), "UNRESOLVED" | "EXTERNAL")
            || matches!(dispatch.as_str(), "DYNAMIC" | "REFLECTIVE" | "UNKNOWN")
            || external == Some(true);
        edges.push(CallEdge {
            caller_id,
            callee_id,
            caller: caller_key,
            callsite: callsite
                .strip_prefix(&format!("{unit}|"))
                .ok_or_else(|| Error::new("E_DB_INTEGRITY", "callsite outside unit"))?
                .into(),
            callee: callee_key.unwrap_or_default(),
            dispatch,
            resolution,
            world_closed,
            incomplete,
        });
    }
    Ok(edges)
}

/// `ske paths explore`:
/// enumerate simple call paths from `source` to `target` in the snapshot's
/// call graph. Returns the DTO per the §19 v0.4 JSON block.
#[allow(clippy::too_many_arguments)]
pub fn paths_explore(
    db: &Connection,
    snapshot: i64,
    source: &str,
    source_id: i64,
    target: &str,
    max_depth: Option<usize>,
    max_paths: Option<usize>,
    max_nodes: Option<usize>,
    limit: usize,
    offset: usize,
) -> Result<Value> {
    let edges = call_edges(db, snapshot)?;
    // Expandable calls only (resolved callee present, not marked incomplete).
    let graph = edges
        .iter()
        .filter(|e| !e.incomplete && !e.callee.is_empty())
        .collect::<Vec<_>>();
    let mut path_results: Vec<Value> = Vec::new();
    let mut visited = BTreeSet::new();
    let mut truncated = false;
    let mut truncation_reason: Option<String> = None;
    // DFS over simple paths: (current function, call list so far). Stack
    // order is deterministic; we sort expansion so results are stable.
    let mut stack = vec![(
        source_id,
        source.to_owned(),
        Vec::<Value>::new(),
        BTreeSet::from([source_id]),
    )];
    let mut depth_frontier = 0;
    while let Some((current, name, calls, path_nodes)) = stack.pop() {
        if max_nodes.is_some_and(|n| !visited.contains(&current) && visited.len() >= n) {
            stack.push((current, name, calls, path_nodes));
            truncated = true;
            truncation_reason = Some("MAX_NODES".into());
            break;
        }
        if max_paths.is_some_and(|n| path_results.len() >= n) {
            stack.push((current, name, calls, path_nodes));
            truncated = true;
            truncation_reason = Some("MAX_PATHS".into());
            break;
        }
        visited.insert(current);
        if name == target {
            path_results.push(json!({ "calls": calls }));
            continue;
        }
        let mut nexts = graph
            .iter()
            .filter(|e| {
                e.caller_id == current && e.callee_id.is_some_and(|id| !path_nodes.contains(&id))
            })
            .collect::<Vec<_>>();
        nexts.sort_by(|a, b| (&a.callee, &a.callsite).cmp(&(&b.callee, &b.callsite)));
        if max_depth.is_some_and(|d| calls.len() >= d) && !nexts.is_empty() {
            depth_frontier += nexts.len();
            truncated = true;
            truncation_reason.get_or_insert_with(|| "MAX_DEPTH".into());
            continue;
        }
        // Push in reverse so the smallest callee pops first (deterministic).
        for e in nexts.iter().rev() {
            let mut new_calls = calls.clone();
            new_calls.push(json!({
                "caller": e.caller,
                "callsite": e.callsite,
                "callee": e.callee,
                "resolution": e.dispatch,
            }));
            let id = e.callee_id.expect("expandable callee");
            let mut nodes = path_nodes.clone();
            nodes.insert(id);
            stack.push((id, e.callee.clone(), new_calls, nodes));
        }
    }
    let incomplete = edges
        .iter()
        .any(|e| e.incomplete && visited.contains(&e.caller_id));
    let page_total = path_results.len();
    let page_truncated = offset.saturating_add(limit) < page_total;
    let status = if page_total == 0 {
        "NO_PATH_FOUND"
    } else {
        "POTENTIALLY_REACHABLE"
    };
    let page_paths = path_results
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    Ok(json!({
        "source": source, "target": target, "status": status,
        "incomplete": incomplete, "paths": page_paths, "truncated": truncated,
        "truncation_reason": truncation_reason,
        "visited_count": visited.len(), "frontier_count": stack.len()+depth_frontier,
        "page": {"limit": limit, "offset": offset, "total": page_total, "truncated": page_truncated}
    }))
}

/// `callgraph`: persistent call sites and targets DTO (AC-0.4-01 read view).
pub fn callgraph(db: &Connection, snapshot: i64) -> Result<Value> {
    let edges = call_edges(db, snapshot)?;
    let mut sites: BTreeMap<String, Value> = BTreeMap::new();
    for e in &edges {
        let entry = sites
            .entry(format!("{}:{}", e.caller_id, e.callsite))
            .or_insert_with(|| {
                json!({
                    "callsite": e.callsite,
                    "caller": e.caller,
                    "dispatch_kind": e.dispatch,
                    "resolution_status": e.resolution,
                    "world_closed": e.world_closed,
                    "targets": []
                })
            });
        if !e.callee.is_empty() {
            entry["targets"]
                .as_array_mut()
                .unwrap()
                .push(json!(e.callee));
        }
    }
    let sites = sites.into_values().collect::<Vec<_>>();
    Ok(json!({"snapshot_id": snapshot, "call_sites": sites}))
}
