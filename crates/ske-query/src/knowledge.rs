//! Knowledge-level query services for v0.7: `explain` (an outcome with its
//! SMT-LIB arrivals and verifier verdict) and `impact` (conservative reverse
//! callers with `incomplete` marking). These are the data behind the MCP
//! `explain` and `impact` tools and the `ske-api` knowledge DTOs.
use crate::{local, one, page, rows, sql};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use ske_common::{Error, Result};

/// `explain` for a **source outcome** label like `example.Decide/sd1/o0`.
///
/// Resolves the containing function, the latest `BRANCH_ANALYSIS` run, the IR
/// outcomes linked to the requested source outcome, their arrivals with the
/// persisted SMT-LIB text, and any `PROVEN_UNREACHABLE` verifier verdict. It
/// never re-reads the source file (AC-0.7-04).
pub fn explain(
    db: &Connection,
    snapshot: i64,
    outcome: &str,
    limit: usize,
    offset: usize,
) -> Result<Value> {
    // Outcome label: `<qualified>.<decision>/<outcome>` e.g. example.Decide/sd1/o0
    let (head, decision_local, outcome_local) = parse_outcome_label(outcome)?;
    // Resolve the function by qualified name.
    let function_id = resolve_qualified(db, snapshot, &head)?;
    // Locate the latest branch analysis run for this entry.
    let run: i64 = db
        .query_row(
            "SELECT r.id FROM analysis_runs r
             WHERE r.snapshot_id=?1 AND r.entry_function_id=?2 AND r.analysis_kind='BRANCH_ANALYSIS'
             ORDER BY r.id DESC LIMIT 1",
            params![snapshot, function_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let run_info = one(
        db,
        "SELECT id AS run_id,status AS run_status,exploration_closed,cut_count FROM analysis_runs WHERE id=?1",
        [run],
    )?;
    let assumptions = rows(
        db,
        "SELECT kind FROM analysis_assumptions WHERE analysis_run_id=?1 ORDER BY id",
        [run],
    )?;
    // The requested source decision/outcome. Local keys are stored with the
    // `unit|` prefix; reconstruct the full key from the function's unit.
    let unit_key: String = db
        .query_row(
            "SELECT u.unit_key FROM functions f JOIN symbols s ON s.id=f.symbol_id
             JOIN compilation_units u ON u.id=s.unit_id WHERE f.id=?1",
            [function_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let full_decision = format!("{unit_key}|{head}/{decision_local}");
    let full_outcome = format!("{unit_key}|{head}/{decision_local}/{outcome_local}");
    let source_outcome = one(
        db,
        "SELECT o.id,o.local_key,d.local_key AS decision_key,o.elided,o.activation_mapping_complete
         FROM source_outcomes o JOIN source_decisions d ON d.id=o.decision_id
         WHERE d.function_id=?1 AND d.local_key=?2 AND o.local_key=?3",
        params![function_id, full_decision, full_outcome],
    )?;
    let source_outcome_id = source_outcome["id"].as_i64().unwrap();
    let cases = rows(
        db,
        "SELECT id,quality FROM source_outcome_activation_cases WHERE source_outcome_id=?1 ORDER BY case_ordinal",
        [source_outcome_id],
    )?;
    let mut ir_ids: Vec<i64> = Vec::new();
    for case in &cases {
        for term in rows(
            db,
            "SELECT ir_outcome_id FROM source_outcome_activation_terms WHERE case_id=?1 ORDER BY term_ordinal",
            [case["id"].as_i64().unwrap()],
        )? {
            let id = term["ir_outcome_id"].as_i64().unwrap();
            if !ir_ids.contains(&id) {
                ir_ids.push(id);
            }
        }
    }
    // Always include any IR outcome directly linked by source_ir_links.
    for link in rows(
        db,
        "SELECT l.ir_outcome_id FROM source_ir_links l WHERE l.source_outcome_id=?1 ORDER BY l.ir_outcome_id",
        [source_outcome_id],
    )? {
        let id = link["ir_outcome_id"].as_i64().unwrap();
        if !ir_ids.contains(&id) {
            ir_ids.push(id);
        }
    }
    // Status + arrivals + proof per IR outcome.
    let mut enriched = Vec::new();
    for id in &ir_ids {
        let result = rows(
            db,
            "SELECT status,soundness,explanation,proof_id FROM branch_analysis_results
             WHERE analysis_run_id=?1 AND ir_outcome_id=?2",
            params![run, id],
        )?
        .into_iter()
        .next()
        .unwrap_or_else(|| {
            json!({"status":"UNANALYZED","soundness":"EXACT","explanation":null,"proof_id":null})
        });
        let mut item = json!({"ir_outcome": id});
        item["status"] = result["status"].clone();
        item["soundness"] = result["soundness"].clone();
        item["explanation"] = result["explanation"].clone();
        let arrivals = rows(
            db,
            "SELECT a.id,a.result,a.soundness,a.arrival_seq,a.query_id
             FROM outcome_arrivals a WHERE a.analysis_run_id=?1 AND a.ir_outcome_id=?2
             ORDER BY a.arrival_seq",
            params![run, id],
        )?;
        let mut arrivals_dto = Vec::new();
        for a in arrivals {
            let query = if let Some(qid) = a["query_id"].as_i64() {
                one(
                    db,
                    "SELECT smtlib,result FROM solver_queries WHERE id=?1",
                    [qid],
                )
                .ok()
            } else {
                None
            };
            arrivals_dto.push(json!({
                "arrival_seq": a["arrival_seq"],
                "result": a["result"],
                "soundness": a["soundness"],
                "smtlib": query.as_ref().and_then(|q| q["smtlib"].as_str().map(|s| s.to_owned())).unwrap_or_default(),
                "verifier_result": query.as_ref().and_then(|q| q["result"].as_str().map(|s| s.to_owned())).unwrap_or_default(),
            }));
        }
        item["arrivals"] = json!(arrivals_dto);
        if let Some(proof_id) = result["proof_id"].as_i64() {
            let mut proof = one(
                db,
                "SELECT id,verifier_name,verifier_version,verdict,verification_level,model_soundness,
                        eliminated_arrivals,residual_frontier,reason FROM proof_records WHERE id=?1",
                [proof_id],
            )?;
            proof["assumptions"] = json!(assumptions
                .iter()
                .map(|a| a["kind"].clone())
                .collect::<Vec<_>>());
            item["proof"] = proof;
        }
        enriched.push(item);
    }
    // Pagination is over IR outcomes (or an explicit slice of arrivals).
    let total = enriched.len();
    let page_ir: Vec<_> = enriched.into_iter().skip(offset).take(limit).collect();
    // Derive the source outcome's post-analysis status from the canonical
    // `analysis_read::apply` (ordered activation + proofs + closure).
    let status = source_outcome_status(
        db,
        run,
        function_id,
        &full_outcome,
        &full_decision,
        snapshot,
    )?;
    Ok(json!({
        "snapshot_id": snapshot,
        "source_outcome": outcome,
        "decision": local(&json!(decision_local)),
        "outcome": local(&json!(outcome_local)),
        "status": status,
        "activation_mapping_complete": source_outcome["activation_mapping_complete"].as_i64() == Some(1),
        "run": run_info,
        "assumptions": json!(assumptions.iter().map(|a| a["kind"].clone()).collect::<Vec<_>>()),
        "ir_outcomes": json!(page_ir),
        "page": page(limit, offset, total),
    }))
}

/// Build the canonical branches DTO for the run, apply `analysis_read::apply`
/// (the same derivation as `ske branches list --run`), and return the source
/// outcome's status (`PROVEN_UNREACHABLE`, `POSSIBLE`, …). Unknowns are never
/// coerced; the derivation is identical to the public navigation contract.
fn source_outcome_status(
    db: &Connection,
    run: i64,
    function_id: i64,
    full_outcome: &str,
    full_decision: &str,
    snapshot: i64,
) -> Result<String> {
    let mut decisions = rows(
        db,
        "SELECT id,local_key,kind FROM source_decisions WHERE function_id=?1 ORDER BY local_key",
        [function_id],
    )?;
    for decision in &mut decisions {
        let did = decision["id"].as_i64().unwrap();
        let mut outcomes = rows(
            db,
            "SELECT id,local_key,label,elided FROM source_outcomes WHERE decision_id=?1 ORDER BY ordinal",
            [did],
        )?;
        for outcome in &mut outcomes {
            let oid = outcome["id"].as_i64().unwrap();
            outcome["id"] = local(&outcome["local_key"]);
            outcome["status"] = json!("UNANALYZED");
            outcome["links"] = json!(rows(
                db,
                "SELECT e.local_key AS ir_outcome,l.mapping_kind,l.quality,l.inverted
                 FROM source_ir_links l JOIN ir_outcomes o ON o.id=l.ir_outcome_id
                 JOIN control_edges e ON e.id=o.control_edge_id WHERE l.source_outcome_id=?1
                 ORDER BY e.local_key",
                [oid],
            )?
            .iter()
            .map(|link| {
                let mut link = link.clone();
                link["ir_outcome"] = local(&link["ir_outcome"]);
                link
            })
            .collect::<Vec<_>>());
        }
        decision["id"] = local(&decision["local_key"]);
        decision["outcomes"] = json!(outcomes);
    }
    let ir_outcomes = rows(
        db,
        "SELECT e.local_key AS id FROM ir_outcomes o JOIN control_edges e ON e.id=o.control_edge_id
         JOIN basic_blocks b ON b.id=e.source_block_id WHERE b.function_id=?1 ORDER BY o.id",
        [function_id],
    )?
    .iter()
    .map(|o| {
        let mut o = o.clone();
        o["id"] = local(&o["id"]);
        o
    })
    .collect::<Vec<_>>();
    let mut dto = json!({
        "snapshot_id": snapshot,
        "source_decisions": decisions,
        "ir_outcomes": ir_outcomes,
    });
    crate::analysis_read::apply(db, run, function_id, &mut dto)?;
    let decisions = dto["source_decisions"]
        .as_array()
        .ok_or_else(|| Error::new("E_DB_INTEGRITY", "branches DTO lacks source_decisions"))?;
    for decision in decisions {
        if decision["id"] != local(&json!(full_decision)) {
            continue;
        }
        for outcome in decision["outcomes"].as_array().unwrap_or(&Vec::new()) {
            if outcome["id"] == local(&json!(full_outcome)) {
                return Ok(outcome["status"]
                    .as_str()
                    .unwrap_or("UNANALYZED")
                    .to_string());
            }
        }
    }
    Err(Error::new(
        "E_NOT_FOUND",
        format!("source outcome {full_outcome}"),
    ))
}

/// Split `example.Decide/sd1/o0` into (`example.Decide`, `sd1`, `o0`).
fn parse_outcome_label(label: &str) -> Result<(String, String, String)> {
    let (head, rest) = label
        .split_once('/')
        .ok_or_else(|| Error::new("E_USAGE", "outcome must be <function>/<decision>/<outcome>"))?;
    let (decision, outcome) = rest
        .split_once('/')
        .ok_or_else(|| Error::new("E_USAGE", "outcome must be <function>/<decision>/<outcome>"))?;
    Ok((head.to_owned(), decision.to_owned(), outcome.to_owned()))
}

fn resolve_qualified(db: &Connection, snapshot: i64, name: &str) -> Result<i64> {
    let (unit, qualified) = name
        .split_once('|')
        .map_or((None, name), |(u, f)| (Some(u), f));
    let matches = rows(
        db,
        "SELECT f.id,u.unit_key FROM functions f JOIN symbols s ON s.id=f.symbol_id
         JOIN compilation_units u ON u.id=s.unit_id
         WHERE s.snapshot_id=?1 AND s.qualified_name=?2 AND f.is_external=0
         AND (?3 IS NULL OR u.unit_key=?3) ORDER BY u.unit_key,f.id",
        params![snapshot, qualified, unit],
    )?;
    match matches.as_slice() {
        [] => Err(Error::new(
            "E_NOT_FOUND",
            format!("function {name} not found"),
        )),
        [value] => Ok(value["id"].as_i64().unwrap()),
        _ => {
            let mut error = Error::new("E_AMBIGUOUS", "function occurs in multiple units");
            error.candidates = matches
                .iter()
                .map(|v| format!("{}|{qualified}", v["unit_key"].as_str().unwrap()))
                .collect();
            Err(error)
        }
    }
}

/// `impact` for a modified function: reverse callers via the snapshot call
/// graph. Any unresolved/external/dynamic edge on a found path marks the result
/// `incomplete:true` (AC-0.7-05); a fully closed graph never claims "no impact"
/// if a path exists, and always reports incomplete when it could not close.
pub fn impact(db: &Connection, snapshot: i64, function: &str) -> Result<Value> {
    let target = resolve_qualified(db, snapshot, function)?;
    let edges = crate::callgraph::call_edges(db, snapshot)?;
    // Known targets remain possible even at open or partially resolved sites.
    let mut reverse: std::collections::BTreeMap<i64, Vec<i64>> = Default::default();
    for edge in &edges {
        if let Some(callee) = edge.callee_id {
            reverse.entry(callee).or_default().push(edge.caller_id);
        }
    }
    let mut callers: Vec<Value> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut frontier = vec![target];
    while let Some(function_id) = frontier.pop() {
        if !seen.insert(function_id) {
            continue;
        }
        if let Some(callers_here) = reverse.get(&function_id) {
            frontier.extend(callers_here);
        }
    }
    for id in seen.iter().filter(|id| **id != target) {
        callers.push(one(
            db,
            "SELECT f.id AS function_id,s.qualified_name,u.unit_key FROM functions f
             JOIN symbols s ON s.id=f.symbol_id JOIN compilation_units u ON u.id=s.unit_id
             WHERE f.id=?1",
            [id],
        )?);
    }
    callers.sort_by(|a, b| {
        (
            a["qualified_name"].as_str(),
            a["unit_key"].as_str(),
            a["function_id"].as_i64(),
        )
            .cmp(&(
                b["qualified_name"].as_str(),
                b["unit_key"].as_str(),
                b["function_id"].as_i64(),
            ))
    });
    // Even sites outside the known reverse closure can hide additional callers.
    // Report them explicitly rather than implying that an empty caller list is
    // proof of no impact. Relevance identifies sites on the known affected path.
    let mut uncertain = std::collections::BTreeMap::new();
    for edge in &edges {
        if edge.incomplete || !edge.world_closed || edge.callee_id.is_none() {
            let key = (edge.caller_id, edge.callsite.clone());
            if let std::collections::btree_map::Entry::Vacant(entry) = uncertain.entry(key) {
                let identity = one(
                    db,
                    "SELECT u.unit_key FROM functions f JOIN symbols s ON s.id=f.symbol_id
                     JOIN compilation_units u ON u.id=s.unit_id WHERE f.id=?1",
                    [edge.caller_id],
                )?;
                entry.insert(json!({
                    "caller_id":edge.caller_id, "caller":edge.caller,
                    "unit_key":identity["unit_key"], "callsite":edge.callsite,
                    "dispatch_kind":edge.dispatch, "resolution_status":edge.resolution,
                    "world_closed":edge.world_closed,
                    "on_affected_path":seen.contains(&edge.caller_id)
                }));
            }
        }
    }
    let incomplete = !uncertain.is_empty();
    Ok(
        json!({"snapshot_id":snapshot,"function":function,"function_id":target,
            "callers":callers,"incomplete":incomplete,
            "incomplete_sites":uncertain.into_values().collect::<Vec<_>>()}),
    )
}

#[cfg(test)]
#[path = "impact_tests.rs"]
mod impact_tests;

#[cfg(test)]
mod tests {
    use super::parse_outcome_label;

    #[test]
    fn parse_accepts_qualified_labels() {
        let (head, d, o) = parse_outcome_label("example.Decide/sd1/o0").unwrap();
        assert_eq!(head, "example.Decide");
        assert_eq!(d, "sd1");
        assert_eq!(o, "o0");
        assert!(parse_outcome_label("no-slash").is_err());
        assert!(parse_outcome_label("a/b").is_err());
    }
}
