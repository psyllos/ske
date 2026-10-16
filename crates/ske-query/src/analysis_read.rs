//! Read-only run DTOs and conservative, ordered source activation evaluation.
use crate::{boolean, local, one, rows};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use ske_common::{Error, Result};
use std::collections::HashMap;

const STATUSES: [&str; 7] = [
    "UNANALYZED",
    "POSSIBLE",
    "WITNESSED",
    "PROVEN_UNREACHABLE",
    "UNKNOWN",
    "TIMEOUT",
    "UNSUPPORTED",
];

fn decode(value: &Value) -> Result<Value> {
    serde_json::from_str(value.as_str().unwrap_or("null"))
        .map_err(|e| Error::new("E_DB_INTEGRITY", e.to_string()))
}

/// The normative `branches analyze` response (§11.5).
pub fn summary(db: &Connection, run_id: i64) -> Result<Value> {
    let mut run = one(db, "SELECT id AS run_id,status AS run_status,exploration_closed,cut_count,metrics_json FROM analysis_runs WHERE id=?1", [run_id])?;
    boolean(&mut run, "exploration_closed");
    let mut counts = json!({});
    for status in STATUSES {
        counts[status] = json!(0);
    }
    for count in rows(db, "SELECT status,COUNT(*) AS n FROM branch_analysis_results WHERE analysis_run_id=?1 GROUP BY status", [run_id])? {
        counts[count["status"].as_str().unwrap()] = count["n"].clone();
    }
    let assumptions = rows(
        db,
        "SELECT kind FROM analysis_assumptions WHERE analysis_run_id=?1 ORDER BY id",
        [run_id],
    )?;
    run["counts"] = counts;
    run["assumptions"] = json!(assumptions
        .iter()
        .map(|a| a["kind"].clone())
        .collect::<Vec<_>>());
    let mut metrics = json!({
        "states_created":0,"instructions":0,
        "solver_queries":{"total":0,"sat":0,"unsat":0,"unknown":0,"timeout":0},
        "wall_ms":0
    });
    let persisted = decode(&run["metrics_json"])?;
    if let Some(object) = persisted.as_object() {
        for (key, value) in object {
            if key == "solver_queries" {
                if let Some(queries) = value.as_object() {
                    for (kind, count) in queries {
                        metrics[key][kind] = count.clone();
                    }
                }
            } else {
                metrics[key] = value.clone();
            }
        }
    }
    run["metrics"] = metrics;
    run.as_object_mut().unwrap().remove("metrics_json");
    Ok(run)
}

/// Match the whole activation in order, requiring its last term at this arrival.
/// Matching different terms against different arrival rows is never sufficient.
fn activates(terms: &[i64], path: &[i64]) -> bool {
    let Some((&last, prefix)) = terms.split_last() else {
        return false;
    };
    if path.last() != Some(&last) {
        return false;
    }
    let mut next = 0;
    for outcome in &path[..path.len() - 1] {
        if prefix.get(next) == Some(outcome) {
            next += 1;
        }
    }
    next == prefix.len()
}

/// Enrich an existing branches inventory. SQL IDs never replace its local IDs.
pub fn apply(db: &Connection, run_id: i64, function_id: i64, dto: &mut Value) -> Result<()> {
    let run = one(
        db,
        "SELECT snapshot_id,entry_function_id,analysis_kind FROM analysis_runs WHERE id=?1",
        [run_id],
    )?;
    let function = one(
        db,
        "SELECT s.snapshot_id FROM functions f JOIN symbols s ON s.id=f.symbol_id WHERE f.id=?1",
        [function_id],
    )?;
    if run["snapshot_id"] != function["snapshot_id"]
        || dto["snapshot_id"] != function["snapshot_id"]
        || run["entry_function_id"] != json!(function_id)
        || run["analysis_kind"] != "BRANCH_ANALYSIS"
    {
        return Err(Error::new(
            "E_USAGE",
            "run must belong to the requested snapshot and entry function",
        ));
    }
    let report = summary(db, run_id)?;
    let closed = report["run_status"] == "COMPLETED"
        && report["exploration_closed"] == true
        && report["cut_count"] == 0;
    let assumptions = report["assumptions"].clone();
    let solver = one(
        db,
        "SELECT solver_name,solver_version FROM analysis_runs WHERE id=?1",
        [run_id],
    )?;
    let arrivals = rows(db, "SELECT id,ir_outcome_id,query_id,result,soundness,path_json FROM outcome_arrivals WHERE analysis_run_id=?1 ORDER BY ir_outcome_id,arrival_seq,id", [run_id])?;
    let mut paths = Vec::new();
    for arrival in &arrivals {
        let path = decode(&arrival["path_json"])?;
        let ids: Option<Vec<i64>> = path
            .as_array()
            .and_then(|a| a.iter().map(Value::as_i64).collect());
        let ids = ids.ok_or_else(|| {
            Error::new(
                "E_DB_INTEGRITY",
                "arrival path must contain SQL outcome IDs",
            )
        })?;
        // An old/default empty path cannot provide joint activation evidence.
        if !ids.is_empty() && ids.last().copied() != arrival["ir_outcome_id"].as_i64() {
            return Err(Error::new(
                "E_DB_INTEGRITY",
                "arrival path must end with its candidate",
            ));
        }
        paths.push(ids);
    }
    let outcomes = rows(db, "SELECT o.id,e.local_key FROM ir_outcomes o JOIN control_edges e ON e.id=o.control_edge_id JOIN basic_blocks b ON b.id=e.source_block_id WHERE b.function_id=?1 ORDER BY o.id", [function_id])?;
    let mut enriched = HashMap::new();
    let mut proofs = HashMap::new();
    for outcome in outcomes {
        let id = outcome["id"].as_i64().unwrap();
        let result = rows(db, "SELECT status,soundness,explanation,proof_id FROM branch_analysis_results WHERE analysis_run_id=?1 AND ir_outcome_id=?2", params![run_id,id])?.into_iter().next();
        let mut value = result.unwrap_or_else(|| json!({"status":"UNANALYZED","soundness":"EXACT","explanation":null,"proof_id":null}));
        let mut evidence: Vec<Value> = arrivals
            .iter()
            .filter(|a| a["ir_outcome_id"] == id)
            .map(|a| json!({"kind":"arrival","id":a["id"],"query_id":a["query_id"]}))
            .collect();
        if let Some(proof_id) = value["proof_id"].as_i64() {
            let mut proof = one(db, "SELECT id,analysis_run_id,ir_outcome_id,verifier_name,verifier_version,verdict,verification_level,model_soundness,eliminated_arrivals,residual_frontier,reason,created_at FROM proof_records WHERE id=?1", [proof_id])?;
            let accepted = closed
                && proof["analysis_run_id"] == run_id
                && proof["ir_outcome_id"] == id
                && proof["verdict"] == "ACCEPTED"
                && proof["verification_level"] == "MODEL_UNREACHABLE"
                && proof["residual_frontier"] == 0
                && proof["model_soundness"] != "UNSUPPORTED";
            proof["assumptions"] = assumptions.clone();
            proof["solver_name"] = solver["solver_name"].clone();
            proof["solver_version"] = solver["solver_version"].clone();
            let item = json!({"kind":"proof","id":proof_id,"proof":proof});
            if accepted && value["status"] == "PROVEN_UNREACHABLE" {
                proofs.insert(id, item.clone());
            }
            evidence.push(item);
        }
        value.as_object_mut().unwrap().remove("proof_id");
        value["evidence"] = json!(evidence);
        enriched.insert(
            local(&outcome["local_key"]).as_str().unwrap().to_owned(),
            value,
        );
    }
    if let Some(ir) = dto["ir_outcomes"].as_array_mut() {
        for outcome in ir {
            if let Some(value) = outcome["id"].as_str().and_then(|id| enriched.get(id)) {
                for (key, value) in value.as_object().unwrap() {
                    outcome[key] = value.clone();
                }
            }
        }
    }
    let sources = rows(db, "SELECT o.id,o.local_key,d.local_key AS decision_key,o.elided,o.activation_mapping_complete FROM source_outcomes o JOIN source_decisions d ON d.id=o.decision_id WHERE d.function_id=?1 ORDER BY d.local_key,o.ordinal", [function_id])?;
    for source in sources {
        let cases = rows(db, "SELECT id,quality FROM source_outcome_activation_cases WHERE source_outcome_id=?1 ORDER BY case_ordinal", [source["id"].as_i64().unwrap()])?;
        let mut exact = source["activation_mapping_complete"] == 1 && !cases.is_empty();
        let mut possible = false;
        let mut impossible = true;
        let mut evidence = Vec::new();
        for case in &cases {
            let terms = rows(db, "SELECT ir_outcome_id FROM source_outcome_activation_terms WHERE case_id=?1 ORDER BY term_ordinal", [case["id"].as_i64().unwrap()])?;
            let terms: Vec<i64> = terms
                .iter()
                .map(|t| t["ir_outcome_id"].as_i64().unwrap())
                .collect();
            exact &= case["quality"] == "EXACT" && !terms.is_empty();
            let mut case_possible = false;
            for (arrival, path) in arrivals.iter().zip(&paths) {
                if arrival["result"] == "SAT"
                    && arrival["soundness"] != "UNSUPPORTED"
                    && activates(&terms, path)
                {
                    case_possible = true;
                    evidence.push(
                        json!({"kind":"arrival","id":arrival["id"],"query_id":arrival["query_id"]}),
                    );
                }
            }
            possible |= case_possible;
            // A verified globally eliminated term eliminates every ordered prefix
            // containing it, including cases whose later terms have zero arrivals.
            // Merely finding UNSAT rows or individual POSSIBLE terms does not.
            let elimination = terms.iter().find_map(|id| proofs.get(id));
            impossible &= elimination.is_some() && !case_possible;
            if let Some(proof) = elimination {
                evidence.push(proof.clone());
            }
        }
        let elided = source["elided"] == 1;
        let status = if elided {
            "UNANALYZED"
        } else if !exact {
            "UNKNOWN"
        } else if possible {
            "POSSIBLE"
        } else if closed && impossible {
            "PROVEN_UNREACHABLE"
        } else {
            "UNKNOWN"
        };
        if let Some(decisions) = dto["source_decisions"].as_array_mut() {
            for decision in decisions {
                if decision["id"] != local(&source["decision_key"]) {
                    continue;
                }
                if let Some(outcomes) = decision["outcomes"].as_array_mut() {
                    for outcome in outcomes {
                        if outcome["id"] != local(&source["local_key"]) {
                            continue;
                        }
                        outcome["activation_mapping_complete"] =
                            json!(source["activation_mapping_complete"] == 1);
                        outcome["activation_case_count"] = json!(cases.len());
                        outcome["status"] = json!(status);
                        outcome["explanation"] = if elided {
                            json!("ELIDED")
                        } else if !exact {
                            json!("SOURCE_ACTIVATION_INCOMPLETE")
                        } else {
                            json!("ORDERED_ACTIVATION")
                        };
                        outcome["evidence"] = if exact && !elided {
                            json!(evidence)
                        } else {
                            json!([])
                        };
                    }
                }
            }
        }
    }
    dto["analysis"] = json!({"run_id":run_id,"run_status":report["run_status"],"exploration_closed":report["exploration_closed"],"counts":report["counts"]});
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{activates, apply, summary};
    use rusqlite::Connection;
    use serde_json::json;

    #[test]
    fn summary_reads_sql_and_zero_fills_statuses() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE analysis_runs(id INTEGER,status TEXT,exploration_closed INTEGER,cut_count INTEGER,metrics_json TEXT);
             CREATE TABLE branch_analysis_results(analysis_run_id INTEGER,status TEXT);
             CREATE TABLE analysis_assumptions(id INTEGER,analysis_run_id INTEGER,kind TEXT);
             INSERT INTO analysis_runs VALUES(7,'COMPLETED',1,0,'{\"states_created\":9}');
             INSERT INTO branch_analysis_results VALUES(7,'POSSIBLE'),(7,'POSSIBLE'),(8,'UNKNOWN');
             INSERT INTO analysis_assumptions VALUES(1,7,'SEQUENTIAL_EXECUTION');"
        ).unwrap();
        let value = summary(&db, 7).unwrap();
        assert_eq!(value["exploration_closed"], true);
        assert_eq!(value["counts"].as_object().unwrap().len(), 7);
        assert_eq!(value["counts"]["POSSIBLE"], 2);
        assert_eq!(value["counts"]["UNKNOWN"], 0);
        assert_eq!(value["metrics"]["states_created"], 9);
        assert_eq!(value["metrics"]["solver_queries"]["total"], 0);
        assert_eq!(value["assumptions"], json!(["SEQUENTIAL_EXECUTION"]));
        assert_eq!(summary(&db, 999).unwrap_err().code, "E_NOT_FOUND");
    }

    #[test]
    fn activation_requires_one_ordered_joint_path() {
        assert!(activates(&[1, 3], &[8, 1, 2, 3]));
        assert!(!activates(&[1, 3], &[3, 1]));
        assert!(!activates(&[1, 3], &[1, 3, 4]));
        assert!(!activates(&[1, 1], &[1]));
        assert!(activates(&[1, 1], &[1, 1]));
        assert!(!activates(&[], &[]));
        assert!(!activates(&[1], &[]));
    }

    #[test]
    fn source_cases_use_joint_paths_not_independent_sat_edges() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE analysis_runs(id INTEGER,snapshot_id INTEGER,entry_function_id INTEGER,analysis_kind TEXT,status TEXT,exploration_closed INTEGER,cut_count INTEGER,metrics_json TEXT,solver_name TEXT,solver_version TEXT);
             CREATE TABLE analysis_assumptions(id INTEGER,analysis_run_id INTEGER,kind TEXT);
             CREATE TABLE functions(id INTEGER,symbol_id INTEGER);
             CREATE TABLE symbols(id INTEGER,snapshot_id INTEGER);
             CREATE TABLE basic_blocks(id INTEGER,function_id INTEGER);
             CREATE TABLE control_edges(id INTEGER,local_key TEXT,source_block_id INTEGER);
             CREATE TABLE ir_outcomes(id INTEGER,control_edge_id INTEGER);
             CREATE TABLE branch_analysis_results(analysis_run_id INTEGER,ir_outcome_id INTEGER,status TEXT,soundness TEXT,explanation TEXT,proof_id INTEGER);
             CREATE TABLE outcome_arrivals(id INTEGER,analysis_run_id INTEGER,ir_outcome_id INTEGER,arrival_seq INTEGER,query_id INTEGER,result TEXT,soundness TEXT,path_json TEXT);
             CREATE TABLE source_decisions(id INTEGER,function_id INTEGER,local_key TEXT);
             CREATE TABLE source_outcomes(id INTEGER,decision_id INTEGER,local_key TEXT,ordinal INTEGER,elided INTEGER,activation_mapping_complete INTEGER);
             CREATE TABLE source_outcome_activation_cases(id INTEGER,source_outcome_id INTEGER,case_ordinal INTEGER,quality TEXT);
             CREATE TABLE source_outcome_activation_terms(case_id INTEGER,term_ordinal INTEGER,ir_outcome_id INTEGER);
             INSERT INTO analysis_runs VALUES(1,1,1,'BRANCH_ANALYSIS','COMPLETED',1,0,'{}','z3','test');
             INSERT INTO functions VALUES(1,1);
             INSERT INTO symbols VALUES(1,1);
             INSERT INTO basic_blocks VALUES(1,1);
             INSERT INTO control_edges VALUES(1,'unit|e1',1),(2,'unit|e2',1);
             INSERT INTO ir_outcomes VALUES(1,1),(2,2);
             INSERT INTO branch_analysis_results VALUES(1,1,'POSSIBLE','EXACT','SAT_ARRIVAL',NULL),(1,2,'POSSIBLE','EXACT','SAT_ARRIVAL',NULL);
             INSERT INTO outcome_arrivals VALUES(1,1,1,0,10,'SAT','EXACT','[1]'),(2,1,2,0,11,'SAT','EXACT','[2]');
             INSERT INTO source_decisions VALUES(1,1,'unit|sd0');
             INSERT INTO source_outcomes VALUES(1,1,'unit|o0',0,0,1);
             INSERT INTO source_outcome_activation_cases VALUES(1,1,0,'EXACT');
             INSERT INTO source_outcome_activation_terms VALUES(1,0,1),(1,1,2);"
        ).unwrap();
        let inventory = json!({"snapshot_id":1,"source_decisions":[{"id":"sd0","outcomes":[{"id":"o0"}]}],"ir_outcomes":[{"id":"e1"},{"id":"e2"}]});
        let mut dto = inventory.clone();
        apply(&db, 1, 1, &mut dto).unwrap();
        assert_eq!(
            dto["source_decisions"][0]["outcomes"][0]["status"],
            "UNKNOWN"
        );
        assert_eq!(dto["ir_outcomes"][0]["status"], "POSSIBLE");
        db.execute(
            "UPDATE outcome_arrivals SET path_json='[1,2]' WHERE id=2",
            [],
        )
        .unwrap();
        apply(&db, 1, 1, &mut dto).unwrap();
        assert_eq!(
            dto["source_decisions"][0]["outcomes"][0]["status"],
            "POSSIBLE"
        );
        db.execute(
            "UPDATE source_outcomes SET activation_mapping_complete=0",
            [],
        )
        .unwrap();
        apply(&db, 1, 1, &mut dto).unwrap();
        assert_eq!(
            dto["source_decisions"][0]["outcomes"][0]["status"],
            "UNKNOWN"
        );
        db.execute("UPDATE source_outcomes SET elided=1", [])
            .unwrap();
        apply(&db, 1, 1, &mut dto).unwrap();
        assert_eq!(
            dto["source_decisions"][0]["outcomes"][0]["status"],
            "UNANALYZED"
        );
        let mut wrong_snapshot = json!({"snapshot_id":2});
        assert_eq!(
            apply(&db, 1, 1, &mut wrong_snapshot).unwrap_err().code,
            "E_USAGE"
        );
    }
}
