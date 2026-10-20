//! Capability layer (v0.8, §7.5): an explicit grouping of scenarios with an
//! optional human nomination.
//!
//! The deterministic grouping is created by `behavior::reconstruct`:
//! canonical_key `function:<unit>|<name>`, origin DETERMINISTIC,
//! validation_status UNVALIDATED. This module adds the *nomination* workflow:
//! a human may name a capability (`title` + validation_status →
//! `HUMAN_VALIDATED`) or reject it (`REJECTED`), and query the top-down view
//! capability → scenarios → cases.
//!
//! Discipline (AC-0.5-08, §7.5): nothing here invents a business name
//! automatically. A NULL title means "not yet nominated". LLM nomination
//! (`LLM_INFERRED`) remains a declared future step (no token spent).
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

/// Canonical capability key for an entry (`function:<unit>|<name>`), mirroring
/// `behavior::reconstruct`.
fn capability_key(db: &Connection, snapshot: i64, qualified: &str) -> Result<String> {
    let unit: String = db
        .query_row(
            "SELECT u.unit_key FROM functions f JOIN symbols s ON s.id=f.symbol_id
             JOIN compilation_units u ON u.id=s.unit_id
             WHERE s.snapshot_id=?1 AND s.qualified_name=?2 AND f.is_external=0 LIMIT 1",
            params![snapshot, qualified],
            |r| r.get(0),
        )
        .map_err(sql)?;
    Ok(format!("function:{unit}|{qualified}"))
}

fn capability(db: &Connection, snapshot: i64, key: &str) -> Result<Value> {
    one(
        db,
        "SELECT id, canonical_key, title, origin, validation_status
         FROM functional_capabilities WHERE snapshot_id=?1 AND canonical_key=?2",
        params![snapshot, key],
    )
}

/// `ske capabilities list`: all capabilities of a snapshot with the count of
/// linked scenarios.
pub fn list(db: &Connection, snapshot: i64) -> Result<Value> {
    let caps = rows(
        db,
        "SELECT fc.id, fc.canonical_key, fc.title, fc.origin, fc.validation_status,
                (SELECT COUNT(*) FROM capability_scenarios csc WHERE csc.capability_id=fc.id) AS scenario_count
         FROM functional_capabilities fc
         WHERE fc.snapshot_id=?1
         ORDER BY fc.canonical_key",
        [snapshot],
    )?;
    Ok(json!({
        "snapshot_id": snapshot,
        "capabilities": caps,
        "page": {"limit": 0, "offset": 0, "total": caps.len(), "truncated": false}
    }))
}

/// `ske capabilities nominate --entry E --title T`: a human names the
/// capability of entry E and marks it `HUMAN_VALIDATED`. This is the only way
/// a title becomes non-NULL (besides a future LLM path).
pub fn nominate(db: &Connection, snapshot: i64, qualified: &str, title: &str) -> Result<Value> {
    let key = capability_key(db, snapshot, qualified)?;
    let cap = capability(db, snapshot, &key).map_err(|_| {
        Error::new(
            "E_NOT_FOUND",
            format!(
                "{qualified}: no capability; run `ske behaviors reconstruct --entry {qualified}` first"
            ),
        )
    })?;
    let id = cap["id"].as_i64().unwrap();
    db.execute(
        "UPDATE functional_capabilities SET title=?1, validation_status='HUMAN_VALIDATED' WHERE id=?2",
        params![title, id],
    )
    .map_err(sql)?;
    Ok(json!({
        "canonical_key": key,
        "title": title,
        "origin": "DETERMINISTIC",
        "validation_status": "HUMAN_VALIDATED",
    }))
}

/// `ske capabilities reject --entry E`: mark the capability `REJECTED`
/// (the nomination is refused; the title is kept for audit).
pub fn reject(db: &Connection, snapshot: i64, qualified: &str) -> Result<Value> {
    let key = capability_key(db, snapshot, qualified)?;
    let cap = capability(db, snapshot, &key).map_err(|_| {
        Error::new(
            "E_NOT_FOUND",
            format!(
                "{qualified}: no capability; run `ske behaviors reconstruct --entry {qualified}` first"
            ),
        )
    })?;
    let id = cap["id"].as_i64().unwrap();
    db.execute(
        "UPDATE functional_capabilities SET validation_status='REJECTED' WHERE id=?1",
        [id],
    )
    .map_err(sql)?;
    Ok(json!({
        "canonical_key": key,
        "validation_status": "REJECTED",
    }))
}

/// `ske capabilities show --entry E --depth N`: the top-down view — the
/// capability (title/status) and each linked scenario's causal chain.
pub fn show(db: &Connection, snapshot: i64, qualified: &str, depth: usize) -> Result<Value> {
    let key = capability_key(db, snapshot, qualified)?;
    let cap = capability(db, snapshot, &key).map_err(|_| {
        Error::new(
            "E_NOT_FOUND",
            format!(
                "{qualified}: no capability; run `ske behaviors reconstruct --entry {qualified}` first"
            ),
        )
    })?;
    let cap_id = cap["id"].as_i64().unwrap();
    let scenarios = rows(
        db,
        "SELECT bs.entry_function_id, s.qualified_name AS entry
         FROM capability_scenarios csc
         JOIN behavior_scenarios bs ON bs.id=csc.scenario_id
         JOIN functions f ON f.id=bs.entry_function_id
         JOIN symbols s ON s.id=f.symbol_id
         WHERE csc.capability_id=?1
         ORDER BY s.qualified_name",
        [cap_id],
    )?;
    let mut chains = Vec::new();
    for sc in &scenarios {
        let entry = sc["entry"].as_str().unwrap();
        chains.push(super::scenarios::show(db, snapshot, entry, depth)?);
    }
    Ok(json!({
        "canonical_key": key,
        "title": cap["title"],
        "origin": cap["origin"],
        "validation_status": cap["validation_status"],
        "scenario_count": scenarios.len(),
        "chains": chains,
    }))
}

/// `ske capabilities stale-check --repository NAME --from A --to B`: mark a
/// capability STALE when its entry function changed or vanished between two
/// snapshots of the same repository (mirrors `rules_stale_check`). Rules are
/// attributed to A and marked there; never copied to B. Uses the same
/// §4.6 structural identity as the rules stale-check.
pub fn stale_check(
    db: &Connection,
    repository: &str,
    from_snapshot: &str,
    to_snapshot: &str,
) -> Result<Value> {
    let tx = db.unchecked_transaction().map_err(sql)?;
    let db: &Connection = &tx;
    let repo: i64 = db
        .query_row(
            "SELECT id FROM repositories WHERE name=?1",
            [repository],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let from_snapshot = crate::behavior::resolve_snapshot_public(db, repo, from_snapshot)?;
    let to_snapshot = crate::behavior::resolve_snapshot_public(db, repo, to_snapshot)?;
    let mut stale = 0usize;
    let mut unchanged = 0usize;
    let caps = rows(
        db,
        "SELECT DISTINCT fc.id AS cap_id, f.symbol_id, f.id AS function_id
         FROM functional_capabilities fc
         JOIN capability_scenarios csc ON csc.capability_id=fc.id
         JOIN behavior_scenarios bs ON bs.id=csc.scenario_id
         JOIN functions f ON f.id=bs.entry_function_id
         WHERE fc.snapshot_id=?1",
        [from_snapshot],
    )?;
    for cap in caps {
        let cap_id = cap["cap_id"].as_i64().unwrap();
        let symbol_id = cap["symbol_id"].as_i64().unwrap();
        let a_function = cap["function_id"].as_i64().unwrap();
        let matches = rows(
            db,
            "SELECT f2.id FROM functions f2
                JOIN symbols s2 ON s2.id=f2.symbol_id
                JOIN symbols s1 ON s1.id=?1
                JOIN compilation_units u1 ON u1.id=s1.unit_id
                JOIN compilation_units u2 ON u2.id=s2.unit_id
                WHERE u2.snapshot_id=?2 AND s2.qualified_name=s1.qualified_name
                  AND u2.unit_key=u1.unit_key AND f2.is_external=0",
            params![symbol_id, to_snapshot],
        )?;
        if matches.len() > 1 {
            return Err(Error::new(
                "E_AMBIGUOUS",
                "multiple target functions in corresponding unit",
            ));
        }
        let b_function = matches.first().and_then(|v| v["id"].as_i64());
        let mut changed = false;
        if let Some(b_function) = b_function {
            let a_structure = crate::behavior::dependency_structure_public(db, a_function).ok();
            let b_structure = crate::behavior::dependency_structure_public(db, b_function).ok();
            match (a_structure, b_structure) {
                (Some(a), Some(b)) => changed = a != b,
                _ => changed = true,
            }
        }
        if b_function.is_none() || changed {
            db.execute(
                "UPDATE functional_capabilities SET validation_status='STALE'
                 WHERE id=?1 AND validation_status<>'STALE'",
                [cap_id],
            )
            .map_err(sql)?;
            stale += 1;
        } else {
            unchanged += 1;
        }
    }
    let response = json!({
        "repository": repository,
        "from": from_snapshot,
        "to": to_snapshot,
        "capabilities_marked_stale": stale,
        "capabilities_unchanged": unchanged,
    });
    tx.commit().map_err(sql)?;
    Ok(response)
}
