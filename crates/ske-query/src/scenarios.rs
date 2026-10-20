//! Causal mono-service scenarios and their persistence (v0.8, §7.5).
//!
//! A scenario is, as in v0.5, a technical grouping per entry function. This
//! module adds the *causal* layer that v0.5 deliberately left empty:
//! `scenario_links` with `relation_kind='CALL'` between the scenario of an
//! entry function and the scenarios of the callees actually composed into its
//! summary (same static closed-world filter as `summaries::compute`).
//!
//! Discipline (AC-0.5-08, §7.5): nothing here invents business meaning. The
//! `confidence_kind` reflects the resolution only (CONFIRMED for a unique
//! static closed-world target). A callee without its own persisted scenario is
//! reported as `not_reconstructed`, never invented as a closed step.
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use ske_common::{Error, Result};

fn sql(e: rusqlite::Error) -> Error {
    Error::new("E_DB_INTEGRITY", e.to_string())
}

fn rows(db: &Connection, q: &str, p: impl rusqlite::Params) -> Result<Vec<Value>> {
    let mut stmt = db.prepare(q).map_err(sql)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut cursor = stmt.query(p).map_err(sql)?;
    let mut out = Vec::new();
    while let Some(row) = cursor.next().map_err(sql)? {
        use rusqlite::types::ValueRef;
        let mut object = serde_json::Map::new();
        for (n, name) in names.iter().enumerate() {
            let value = match row.get_ref(n).map_err(sql)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Real(n) => json!(n),
                ValueRef::Text(s) => json!(String::from_utf8_lossy(s)),
                ValueRef::Blob(_) => return Err(Error::new("E_INTERNAL", "blob in public DTO")),
            };
            object.insert(name.clone(), value);
        }
        out.push(Value::Object(object));
    }
    Ok(out)
}

fn one(db: &Connection, q: &str, p: impl rusqlite::Params) -> Result<Value> {
    rows(db, q, p)?
        .into_iter()
        .next()
        .ok_or_else(|| Error::new("E_NOT_FOUND", "object not found"))
}

#[derive(Clone, Debug)]
struct Entry {
    id: i64,
    name: String,
}

fn entry(db: &Connection, snapshot: i64, qualified: &str) -> Result<Entry> {
    let row = one(
        db,
        "SELECT f.id, s.qualified_name AS name FROM functions f
         JOIN symbols s ON s.id=f.symbol_id
         WHERE s.snapshot_id=?1 AND s.qualified_name=?2 AND f.is_external=0
         LIMIT 1",
        params![snapshot, qualified],
    )?;
    Ok(Entry {
        id: row["id"].as_i64().unwrap(),
        name: row["name"].as_str().unwrap().to_owned(),
    })
}

/// Resolve a function by qualified_name across any snapshot of the database
/// (used for declared interservice targets, which may live in another
/// repository/snapshot). If the name occurs in several units, returns the
/// first deterministically (ordered by snapshot id then unit).
fn entry_any_snapshot(db: &Connection, qualified: &str) -> Result<Entry> {
    let row = one(
        db,
        "SELECT f.id, s.qualified_name AS name FROM functions f
         JOIN symbols s ON s.id=f.symbol_id
         WHERE s.qualified_name=?1 AND f.is_external=0
         ORDER BY s.snapshot_id, s.unit_id LIMIT 1",
        [qualified],
    )?;
    Ok(Entry {
        id: row["id"].as_i64().unwrap(),
        name: row["name"].as_str().unwrap().to_owned(),
    })
}

/// Scenario row of an entry function (its most recent reconstructed one).
fn scenario_of(db: &Connection, function: i64) -> Result<Option<Value>> {
    let s = rows(
        db,
        "SELECT id, name, status, precision FROM behavior_scenarios
         WHERE entry_function_id=?1 ORDER BY id DESC LIMIT 1",
        [function],
    )?;
    Ok(s.into_iter().next())
}

/// Statically-resolved, closed-world, non-external callees of `function`
/// (same filter as `summaries::compute` builds summaries for).
fn composed_callees(db: &Connection, function: i64) -> Result<Vec<(i64, String)>> {
    let rows = rows(
        db,
        "SELECT ct.target_function_id, t_s.qualified_name
         FROM call_sites cs
         JOIN instructions i ON i.id=cs.instruction_id
         JOIN basic_blocks b ON b.id=i.block_id
         JOIN call_targets ct ON ct.call_site_id=cs.id
         JOIN functions t ON t.id=ct.target_function_id
         JOIN symbols t_s ON t_s.id=t.symbol_id
         WHERE b.function_id=?1 AND cs.dispatch_kind='STATIC'
           AND cs.world_closed=1 AND cs.resolution_status='RESOLVED_UNIQUE'
           AND t.is_external=0
         ORDER BY t_s.qualified_name",
        [function],
    )?;
    let mut out = Vec::new();
    for row in rows {
        out.push((
            row["target_function_id"].as_i64().unwrap(),
            row["qualified_name"].as_str().unwrap_or("").to_owned(),
        ));
    }
    Ok(out)
}

/// `ske scenarios list`: every reconstructed scenario of the snapshot with its
/// capability and the count of its outgoing scenario links.
pub fn list(db: &Connection, snapshot: i64) -> Result<Value> {
    let scenarios = rows(
        db,
        "SELECT bs.id, bs.name, bs.status, bs.precision,
                fc.title AS capability_title, fc.origin AS capability_origin,
                fc.validation_status AS capability_validation,
                (SELECT COUNT(*) FROM scenario_links sl WHERE sl.from_scenario_id=bs.id) AS link_count
         FROM behavior_scenarios bs
         JOIN analysis_runs r ON r.id=bs.analysis_run_id
         LEFT JOIN capability_scenarios csc ON csc.scenario_id=bs.id
         LEFT JOIN functional_capabilities fc ON fc.id=csc.capability_id
         WHERE r.snapshot_id=?1
         ORDER BY bs.name, bs.id",
        [snapshot],
    )?;
    Ok(json!({
        "snapshot_id": snapshot,
        "scenarios": scenarios,
        "page": {"limit": 0, "offset": 0, "total": scenarios.len(), "truncated": false}
    }))
}

/// `ske scenarios list` also returns link_count per scenario. This exposes
/// the raw materialized scenario links for the snapshot (v0.8, read view).
pub fn list_links(db: &Connection, snapshot: i64) -> Result<Value> {
    let links = rows(
        db,
        "SELECT sl.id, from_s.name AS from_scenario, to_s.name AS to_scenario,
                sl.relation_kind, sl.confidence_kind
         FROM scenario_links sl
         JOIN behavior_scenarios from_s ON from_s.id=sl.from_scenario_id
         LEFT JOIN behavior_scenarios to_s ON to_s.id=sl.to_scenario_id
         JOIN analysis_runs r ON r.id=from_s.analysis_run_id
         WHERE r.snapshot_id=?1
         ORDER BY sl.id",
        [snapshot],
    )?;
    Ok(json!({
        "snapshot_id": snapshot,
        "links": links,
        "page": {"limit": 0, "offset": 0, "total": links.len(), "truncated": false}
    }))
}

/// `ske scenarios link --entry E`: materialize idempotently the CALL edges
/// from E's scenario to the scenarios of the callees composed into its summary
/// (those that were themselves reconstructed). Callees without their own
/// scenario are returned as `not_reconstructed`, never invented as a step.
pub fn link(db: &Connection, snapshot: i64, qualified: &str) -> Result<Value> {
    let e = entry(db, snapshot, qualified)?;
    let from = scenario_of(db, e.id)?.ok_or_else(|| {
        Error::new(
            "E_NOT_FOUND",
            format!(
                "{qualified}: no reconstructed scenario; run `ske behaviors reconstruct --entry {qualified}` first"
            ),
        )
    })?;
    let from_id: i64 = from["id"].as_i64().unwrap();
    let callees = composed_callees(db, e.id)?;
    let mut linked = Vec::new();
    let mut not_reconstructed = Vec::new();
    for (callee_id, callee_name) in &callees {
        let callee_name = callee_name.clone();
        match scenario_of(db, *callee_id)? {
            Some(to) => {
                let to_id: i64 = to["id"].as_i64().unwrap();
                let exists: bool = db
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM scenario_links
                          WHERE from_scenario_id=?1 AND to_scenario_id=?2 AND relation_kind='CALL')",
                        params![from_id, to_id],
                        |r| r.get(0),
                    )
                    .map_err(sql)?;
                if !exists {
                    db.execute(
                        "INSERT INTO scenario_links(from_scenario_id,to_scenario_id,relation_kind,
                           confidence_kind,evidence_json,assumptions_json)
                         VALUES(?1,?2,'CALL','CONFIRMED',?3,?4)",
                        params![
                            from_id,
                            to_id,
                            serde_json::to_string(
                                &json!([{"kind": "call_target", "callee": callee_name}])
                            )
                            .unwrap(),
                            "[]"
                        ],
                    )
                    .map_err(sql)?;
                }
                linked.push(
                    json!({"callee": callee_name, "scenario_id": to_id, "confidence": "CONFIRMED"}),
                );
            }
            None => not_reconstructed.push(json!(callee_name)),
        }
    }
    Ok(json!({
        "entry": qualified,
        "scenario_id": from_id,
        "links": linked,
        "not_reconstructed": not_reconstructed,
        "total_callees": callees.len(),
    }))
}

/// `ske scenarios show --entry E --depth N`: display the causal mono-service
/// chain entry → callees → cases, computed on the fly (never persisted). Each
/// step carries its behavior cases; a callee without a reconstructed scenario
/// is marked `not_reconstructed` and the walk stops there. `EXTERNAL_UNKNOWN`
/// links mark an honest boundary instead of a closed scenario.
pub fn show(db: &Connection, snapshot: i64, qualified: &str, depth: usize) -> Result<Value> {
    fn step(db: &Connection, function: i64, name: &str, depth: usize) -> Result<Value> {
        let scenario = scenario_of(db, function)?;
        let scenario_id = scenario.as_ref().map(|s| s["id"].as_i64().unwrap());
        let cases = match scenario_id {
            Some(sid) => rows(
                db,
                "SELECT case_ordinal, guard_text, result_text, effects_text, precision
                 FROM behavior_cases WHERE behavior_id=?1 ORDER BY case_ordinal",
                [sid],
            )?,
            None => Vec::new(),
        };
        let boundary = match scenario_id {
            Some(sid) => rows(
                db,
                "SELECT relation_kind, confidence_kind FROM scenario_links
                 WHERE from_scenario_id=?1 AND relation_kind='EXTERNAL_UNKNOWN'",
                [sid],
            )?,
            None => Vec::new(),
        };
        let mut out = json!({
            "function": name,
            "cases": cases,
            "external_boundary": if boundary.is_empty() { Value::Null } else {
                boundary[0]["relation_kind"].clone()
            },
        });
        match &scenario {
            Some(s) => {
                out["scenario_id"] = json!(s["id"].as_i64().unwrap());
                out["scenario_status"] = s["status"].clone();
                out["scenario_precision"] = s["precision"].clone();
                out["not_reconstructed"] = json!(false);
            }
            None => {
                out["scenario_id"] = Value::Null;
                out["not_reconstructed"] = json!(true);
            }
        }
        let children = if depth == 0 {
            Vec::new()
        } else {
            composed_callees(db, function)?
                .into_iter()
                .map(|(cid, cname)| step(db, cid, &cname, depth - 1))
                .collect::<Result<Vec<_>>>()?
        };
        out["callees"] = json!(children);
        Ok(out)
    }
    let e = entry(db, snapshot, qualified)?;
    step(db, e.id, &e.name, depth)
}

/// One declared interservice contract: a human maps an UNKNOWN_EFFECT of an
/// entry to a scenario of another service (same database) with a relation kind
/// and explicit assumptions. Never auto-inferred; the link is defined by this
/// declaration and persisted with `confidence_kind='INFERRED'`.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct InterContract {
    /// Optional: matches an effect's `target_descriptor` when the adapter
    /// names the external call. When absent (`match_any`), applies to any
    /// UNKNOWN_EFFECT of the entry.
    #[serde(default)]
    pub target_descriptor: Option<String>,
    pub relation_kind: String,
    pub service_function: String,
    /// When true (and no `target_descriptor`), the contract covers every
    /// anonymous external effect of the entry (e.g. a `func` value call).
    #[serde(default)]
    pub match_any: bool,
    #[serde(default)]
    pub assumptions: Vec<String>,
}

/// `ske scenarios link-inter --entry E --contracts FILE`: materialize
/// declared interservice `scenario_links` from E's scenario to the scenario
/// of `service_function` in another repository of the same database, for every
/// contract matching an UNKNOWN_EFFECT target_descriptor. Confidence is
/// always `INFERRED` (a declared hypothesis, never a proven causality, §7.5);
/// evidence and assumptions come from the contract file. A target whose
/// scenario was not reconstructed is reported `not_reconstructed`, never
/// invented. Returns the DTO.
pub fn link_inter(
    db: &Connection,
    snapshot: i64,
    qualified: &str,
    contracts: &[InterContract],
) -> Result<Value> {
    let e = entry(db, snapshot, qualified)?;
    let from = scenario_of(db, e.id)?.ok_or_else(|| {
        Error::new(
            "E_NOT_FOUND",
            format!(
                "{qualified}: no reconstructed scenario; run `ske behaviors reconstruct --entry {qualified}` first"
            ),
        )
    })?;
    let from_id: i64 = from["id"].as_i64().unwrap();
    let effects = rows(
        db,
        "SELECT e.effect_kind, e.target_descriptor, e.payload
         FROM observable_effects e JOIN summary_cases sc ON sc.id=e.summary_case_id
         JOIN function_summaries fs ON fs.id=sc.summary_id
         JOIN functions f ON f.id=fs.function_id
         WHERE f.id=?1 AND e.effect_kind IN ('UNKNOWN_EFFECT','EXTERNAL_CALL')
         ORDER BY e.effect_ordinal",
        [e.id],
    )?;
    let mut linked = Vec::new();
    let mut missing_effect = Vec::new();
    let mut not_reconstructed = Vec::new();
    for contract in contracts {
        let has_effect = !effects.is_empty();
        let matched_effect = match &contract.target_descriptor {
            Some(desc) => effects
                .iter()
                .any(|fx| fx["target_descriptor"].as_str() == Some(desc.as_str())),
            None => contract.match_any && has_effect,
        };
        if !matched_effect {
            missing_effect.push(json!(contract
                .target_descriptor
                .clone()
                .unwrap_or_else(|| "<anonymous>".to_owned())));
            continue;
        }
        let target = match entry_any_snapshot(db, &contract.service_function) {
            Ok(t) => t,
            Err(_) => {
                not_reconstructed.push(json!(contract.service_function));
                continue;
            }
        };
        let to = match scenario_of(db, target.id)? {
            Some(t) => t,
            None => {
                not_reconstructed.push(json!(contract.service_function));
                continue;
            }
        };
        let to_id: i64 = to["id"].as_i64().unwrap();
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM scenario_links
                  WHERE from_scenario_id=?1 AND to_scenario_id=?2 AND relation_kind=?3)",
                params![from_id, to_id, contract.relation_kind],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if !exists {
            db.execute(
                "INSERT INTO scenario_links(from_scenario_id,to_scenario_id,relation_kind,
                   confidence_kind,evidence_json,assumptions_json)
                 VALUES(?1,?2,?3,'INFERRED',?4,?5)",
                params![
                    from_id,
                    to_id,
                    contract.relation_kind,
                    serde_json::to_string(&json!([{
                        "kind": "declared_contract",
                        "target_descriptor": contract.target_descriptor.clone(),
                        "service_function": contract.service_function
                    }]))
                    .map_err(|e| Error::new("E_INTERNAL", e.to_string()))?,
                    serde_json::to_string(&contract.assumptions)
                        .map_err(|e| Error::new("E_INTERNAL", e.to_string()))?
                ],
            )
            .map_err(sql)?;
        }
        linked.push(json!({
            "target_descriptor": contract.target_descriptor.clone(),
            "relation_kind": contract.relation_kind,
            "service_function": contract.service_function,
            "to_scenario_id": to_id,
            "confidence": "INFERRED",
        }));
    }
    Ok(json!({
        "entry": qualified,
        "scenario_id": from_id,
        "links": linked,
        "missing_effect": missing_effect,
        "not_reconstructed": not_reconstructed,
    }))
}
