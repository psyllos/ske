//! Behavior reconstruction and rule persistence (§7.2, §3.4 — v0.5).
//!
//! Reads a function's persisted summary cases, reconstructs expression DAGs
//! into `ske_symbolic` arenas, renders guards/results with `ske-behavior`
//! (rules R1–R6), groups cases conservatively (§7.2 steps 3–4), and persists
//! behaviors / rules / evidence inside one transaction.
use super::{local, rows, span_text, sql};
use prost::Message;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use ske_behavior::{dedup_cases, render_guard, render_value, BehaviorCaseRowInput, Names};
use ske_common::{Error, Result};
use ske_symbolic::{BinaryOp, Constant, ExprArena, ExprId, ExprKind, Node, Sort, Type, UnaryOp};
use std::collections::BTreeMap;

/// Rebuild a `ske_symbolic::Type` from a persisted `type_id` (NULL -> engine
/// bool) or from the `cir_types` TypeDef protobuf.
fn expr_type(db: &Connection, type_id: Option<i64>) -> Result<Type> {
    let Some(type_id) = type_id else {
        return Ok(Type {
            local_key: "engine:bool".into(),
            sort: Sort::Bool,
        });
    };
    let payload: Vec<u8> = db
        .query_row(
            "SELECT payload FROM cir_types WHERE id=?1",
            [type_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let definition = ske_cir::proto::TypeDef::decode(payload.as_slice())
        .map_err(|e| Error::new("E_DB_INTEGRITY", e.to_string()))?;
    let stored: String = db
        .query_row(
            "SELECT local_key FROM cir_types WHERE id=?1",
            [type_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let local_key = local(&json!(stored)).as_str().unwrap_or("type").to_owned();
    let sort = match definition.kind.as_str() {
        "BOOL" => Sort::Bool,
        "BITVECTOR" => Sort::BitVec {
            width: definition.bit_width,
            signed: definition.is_signed,
        },
        _ => Sort::Uninterpreted,
    };
    Ok(Type { local_key, sort })
}

/// `init:{Global("example.ErrUnderage")}@0:havoc#0` -> `ErrUnderage`.
fn extract_global(symbol: &str) -> Option<String> {
    let start = symbol.find("Global(\"")? + "Global(\"".len();
    let end = symbol[start..].find('"')? + start;
    let qualified = &symbol[start..end];
    qualified
        .rsplit_once('.')
        .map(|(_, s)| s.to_owned())
        .or_else(|| Some(qualified.to_owned()))
}

/// Reachable closure of an expression (itself + operands) in ascending
/// persisted-id order (children before parents per CHK-18).
fn expression_closure(db: &Connection, snapshot: i64, root: i64) -> Result<Vec<i64>> {
    let mut seen: Vec<i64> = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        for row in rows(
            db,
            "SELECT operand_expression_id FROM expression_operands WHERE expression_id=?1 AND expression_id IN (SELECT id FROM symbolic_expressions WHERE snapshot_id=?2)",
            params![id, snapshot],
        )? {
            stack.push(row["operand_expression_id"].as_i64().unwrap());
        }
    }
    seen.sort_unstable();
    Ok(seen)
}

/// Build one node in the arena from its persisted kind/payload/type/operands.
fn build_expr(
    arena: &mut ExprArena,
    kind: &str,
    payload_text: &str,
    ty: &Type,
    operands: &[ExprId],
) -> Result<ExprId> {
    let mut intern = |n: Node| -> Result<ExprId> {
        arena
            .intern(n)
            .map_err(|e| Error::new("E_INTERNAL", format!("render arena: {e:?}")))
    };
    match kind {
        "CONST" => {
            let constant = match ty.sort {
                Sort::Bool => Constant::Bool(payload_text == "true"),
                _ if payload_text == "null" => Constant::Null,
                _ => Constant::Bits(payload_text.parse::<u128>().map_err(|e| {
                    Error::new("E_DB_INTEGRITY", format!("bad const payload: {e}"))
                })?),
            };
            intern(Node {
                ty: ty.clone(),
                kind: ExprKind::Const(constant),
            })
        }
        "SYMBOL" => intern(Node {
            ty: ty.clone(),
            kind: ExprKind::Symbol(payload_text.to_owned()),
        }),
        "HAVOC" => intern(Node {
            ty: ty.clone(),
            kind: ExprKind::Havoc(payload_text.parse::<u64>().unwrap_or(0)),
        }),
        "UNARY" => {
            let op = match payload_text {
                "Not" => UnaryOp::Not,
                "Neg" => UnaryOp::Neg,
                "BitNot" => UnaryOp::BitNot,
                "Cast" => UnaryOp::Cast,
                other => {
                    return Err(Error::new(
                        "E_DB_INTEGRITY",
                        format!("unknown unary op {other}"),
                    ))
                }
            };
            intern(Node {
                ty: ty.clone(),
                kind: ExprKind::Unary {
                    op,
                    arg: operands[0],
                },
            })
        }
        "BINARY" => {
            let op = match payload_text {
                "Add" => BinaryOp::Add,
                "Sub" => BinaryOp::Sub,
                "Mul" => BinaryOp::Mul,
                "Div" => BinaryOp::Div,
                "Rem" => BinaryOp::Rem,
                "BitAnd" => BinaryOp::BitAnd,
                "BitOr" => BinaryOp::BitOr,
                "Xor" => BinaryOp::Xor,
                "Shl" => BinaryOp::Shl,
                "Shr" => BinaryOp::Shr,
                "Eq" => BinaryOp::Eq,
                "Ne" => BinaryOp::Ne,
                "Lt" => BinaryOp::Lt,
                "Le" => BinaryOp::Le,
                "Gt" => BinaryOp::Gt,
                "Ge" => BinaryOp::Ge,
                "And" => BinaryOp::And,
                "Or" => BinaryOp::Or,
                other => {
                    return Err(Error::new(
                        "E_DB_INTEGRITY",
                        format!("unknown binary op {other}"),
                    ))
                }
            };
            intern(Node {
                ty: ty.clone(),
                kind: ExprKind::Binary {
                    op,
                    left: operands[0],
                    right: operands[1],
                },
            })
        }
        "ITE" => intern(Node {
            ty: ty.clone(),
            kind: ExprKind::Ite {
                condition: operands[0],
                then_expr: operands[1],
                else_expr: operands[2],
            },
        }),
        "FIELD" => intern(Node {
            ty: ty.clone(),
            kind: ExprKind::Field {
                base: operands[0],
                index: payload_text.parse::<u32>().map_err(|e| {
                    Error::new("E_DB_INTEGRITY", format!("invalid FIELD index: {e}"))
                })?,
            },
        }),
        "UNINTERPRETED" => intern(Node {
            ty: ty.clone(),
            kind: ExprKind::Uninterpreted {
                name: payload_text.to_owned(),
                args: operands.to_vec(),
            },
        }),
        other => Err(Error::new(
            "E_DB_INTEGRITY",
            format!("unknown expression kind {other}"),
        )),
    }
}

/// Reconstruct a persisted expression root into `arena`, returning its local
/// root id and registering global short names into `names` (R4).
fn reconstruct_expr(
    db: &Connection,
    snapshot: i64,
    root: i64,
    names: &mut Names,
    arena: &mut ExprArena,
) -> Result<ExprId> {
    let closure = expression_closure(db, snapshot, root)?;
    let mut built: BTreeMap<i64, ExprId> = BTreeMap::new();
    for &id in &closure {
        let (kind, payload_bytes, type_id): (String, Vec<u8>, Option<i64>) = db
            .query_row(
                "SELECT kind, payload, type_id FROM symbolic_expressions WHERE id=?1 AND snapshot_id=?2",
                params![id, snapshot],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(sql)?;
        let payload_text = {
            let raw = String::from_utf8_lossy(&payload_bytes);
            // Persistent payloads are JSON-encoded (e.g. a SYMBOL name is a JSON
            // string with escaped quotes); decode when possible.
            serde_json::from_str::<String>(raw.trim())
                .unwrap_or_else(|_| raw.trim_matches('"').to_owned())
        };
        let ty = expr_type(db, type_id)?;
        let operands: Vec<ExprId> = {
            let mut stmt = db
                .prepare(
                    "SELECT operand_expression_id FROM expression_operands WHERE expression_id=?1 ORDER BY ordinal",
                )
                .map_err(sql)?;
            let rows = stmt.query_map([id], |r| r.get::<_, i64>(0)).map_err(sql)?;
            let mut out = Vec::new();
            for row in rows {
                let pid = row.map_err(sql)?;
                out.push(built[&pid]);
            }
            out
        };
        let built_id = build_expr(arena, &kind, &payload_text, &ty, &operands)?;
        if kind == "SYMBOL" {
            if let Some(global) = extract_global(&payload_text) {
                names.insert(payload_text.clone(), global);
            }
        }
        built.insert(id, built_id);
    }
    built
        .get(&root)
        .copied()
        .ok_or_else(|| Error::new("E_INTERNAL", "root not built"))
}

/// R4 name resolver: parameters -> ValueDef.name.
fn function_names(db: &Connection, function: i64) -> Result<Names> {
    let mut names = Names::new();
    let function_local: String = db
        .query_row(
            "SELECT s.local_key FROM symbols s JOIN functions f ON f.symbol_id=s.id WHERE f.id=?1",
            [function],
            |r| r.get::<_, String>(0),
        )
        .map_err(sql)?;
    let function_local = local(&json!(function_local))
        .as_str()
        .unwrap_or("fn")
        .to_owned();
    for row in rows(
        db,
        "SELECT name, param_index FROM values_ir WHERE function_id=?1 AND value_role='PARAM' AND param_index IS NOT NULL ORDER BY param_index",
        [function],
    )? {
        let name = row["name"].as_str().unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let index = row["param_index"].as_i64().unwrap_or(0);
        names.insert(format!("param:{function_local}:{index}"), name.to_owned());
    }
    Ok(names)
}

/// Render one summary case: returns (guard_text, [return_text...], exit_span_id).
fn render_case(
    db: &Connection,
    snapshot: i64,
    case: &Value,
    names: &mut Names,
) -> Result<(String, Vec<String>, Option<i64>)> {
    let case_id = case["id"].as_i64().unwrap();
    let mut arena = ExprArena::new();
    let guard_ids: Vec<i64> = serde_json::from_str(case["guard_json"].as_str().unwrap_or("[]"))
        .map_err(|e| Error::new("E_DB_INTEGRITY", format!("guard_json: {e}")))?;
    let mut guard_terms = Vec::new();
    for id in guard_ids {
        let root = reconstruct_expr(db, snapshot, id, names, &mut arena)?;
        guard_terms.push(root);
    }
    let guard = render_guard(&arena, &guard_terms, names);
    let mut returns = Vec::new();
    for row in rows(
        db,
        "SELECT expression_id FROM summary_case_returns WHERE summary_case_id=?1 ORDER BY return_ordinal",
        [case_id],
    )? {
        let id = row["expression_id"].as_i64().unwrap();
        let root = reconstruct_expr(db, snapshot, id, names, &mut arena)?;
        returns.push(render_value(&arena, root, names));
    }
    let span = case["exit_span_id"].as_i64();
    Ok((guard, returns, span))
}

fn span_line_col(db: &Connection, span: i64) -> Result<Option<(u64, u64)>> {
    let (line, col): (i64, i64) = db
        .query_row(
            "SELECT start_line,start_column FROM source_spans WHERE id=?1",
            [span],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(sql)?;
    Ok(Some((line.max(0) as u64, col.max(0) as u64)))
}

/// Map IR-outcome edge labels (already local) to the linked source outcomes
/// (`example.Check/sd0/o0`), preserving the first-seen order.
fn source_outcomes_for(db: &Connection, snapshot: i64, ir_ids: &[i64]) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    for id in ir_ids {
        let rows = rows(
            db,
            "SELECT so.local_key AS so_key, sd.local_key AS sd_key FROM source_ir_links l
             JOIN source_outcomes so ON so.id=l.source_outcome_id
             JOIN source_decisions sd ON sd.id=so.decision_id
             JOIN ir_outcomes io ON io.id=l.ir_outcome_id
             JOIN control_edges e ON e.id=io.control_edge_id
             JOIN basic_blocks b ON b.id=e.source_block_id
             JOIN functions f ON f.id=b.function_id
             JOIN symbols s ON s.id=f.symbol_id
             WHERE io.id=?1 AND s.snapshot_id=?2 ORDER BY sd.local_key,so.ordinal",
            params![id, snapshot],
        )?;
        for row in rows {
            let so = local(&row["so_key"]).as_str().unwrap_or("").to_owned();
            let sd = local(&row["sd_key"]).as_str().unwrap_or("").to_owned();
            let source = if !so.is_empty() { so } else { sd };
            let source_value = json!(source);
            if !out.contains(&source_value) {
                out.push(source_value);
            }
        }
    }
    Ok(out)
}

/// Reconstruct and persist the behaviors/rules/evidence for `entry` from its
/// latest summary. Creates a `BEHAVIOR` analysis run and returns the JSON DTO.
pub fn reconstruct(
    db: &Connection,
    snapshot: i64,
    entry: i64,
    limit: usize,
    offset: usize,
) -> Result<Value> {
    let tx = db.unchecked_transaction().map_err(sql)?;
    let db: &Connection = &tx;
    let summary_id: i64 = db
        .query_row(
            "SELECT fs.id FROM function_summaries fs WHERE fs.function_id=?1 AND fs.complete=1 ORDER BY fs.id DESC LIMIT 1",
            [entry],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let summary_run: i64 = db
        .query_row(
            "SELECT produced_by_run_id FROM function_summaries WHERE id=?1",
            [summary_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let (qualified,): (String,) = db
        .query_row(
            "SELECT s.qualified_name FROM functions f JOIN symbols s ON s.id=f.symbol_id WHERE f.id=?1",
            [entry],
            |r| Ok((r.get(0)?,)),
        )
        .map_err(sql)?;
    let (precision,): (String,) = db
        .query_row(
            "SELECT precision FROM function_summaries WHERE id=?1",
            [summary_id],
            |r| Ok((r.get(0)?,)),
        )
        .map_err(sql)?;
    db.execute(
        "INSERT INTO analysis_runs(snapshot_id,entry_function_id,analysis_kind,engine_version,
         config_fingerprint,assumptions_fingerprint,status)
         SELECT ?1,?2,'BEHAVIOR','ske-behavior-0.5',config_fingerprint,assumptions_fingerprint,'RUNNING'
         FROM analysis_runs WHERE id=?3",
        params![snapshot, entry, summary_run],
    )
    .map_err(sql)?;
    let run = db.last_insert_rowid();
    db.execute(
        "INSERT INTO analysis_assumptions(analysis_run_id,kind,description,payload)
         SELECT ?1,kind,description,payload FROM analysis_assumptions WHERE analysis_run_id=?2",
        params![run, summary_run],
    )
    .map_err(sql)?;
    db.execute(
        "INSERT INTO behavior_scenarios(analysis_run_id,entry_function_id,local_key,name,status,precision)
         VALUES(?1,?2,?3,?4,'COMPLETED',?5)",
        params![run, entry, qualified, qualified, precision],
    )
    .map_err(sql)?;
    let scenario = db.last_insert_rowid();

    let cases = rows(
        db,
        "SELECT id,case_ordinal,guard_json,exit_kind,precision,exit_span_id,memory_delta_json
         FROM summary_cases WHERE summary_id=?1 AND exit_kind IN ('RETURN','PANIC','THROW') ORDER BY case_ordinal",
        [summary_id],
    )?;
    let mut names = function_names(db, entry)?;
    let mut inputs = Vec::new();
    for case in &cases {
        let (guard, returns, span) = render_case(db, snapshot, case, &mut names)?;
        let exit_kind = case["exit_kind"].as_str().unwrap_or("RETURN").to_owned();
        let result = returns.join(", ");
        let return_keys = rows(
            db,
            "SELECT e.expression_hash FROM summary_case_returns r
             JOIN symbolic_expressions e ON e.id=r.expression_id
             WHERE r.summary_case_id=?1 ORDER BY r.return_ordinal",
            [case["id"].as_i64().unwrap()],
        )?
        .into_iter()
        .map(|r| r["expression_hash"].as_str().unwrap().to_owned())
        .collect();
        let effects = rows(
            db,
            "SELECT effect_kind,target_descriptor,payload,semantic_model_fingerprint,precision
             FROM observable_effects WHERE summary_case_id=?1 ORDER BY effect_ordinal",
            [case["id"].as_i64().unwrap()],
        )?;
        let outcomes = rows(
            db,
            "WITH decisions AS (
               SELECT d.id, ROW_NUMBER() OVER (PARTITION BY b.function_id ORDER BY b.ordinal,i.ordinal)-1 AS ordinal
               FROM ir_decisions d JOIN instructions i ON i.id=d.terminator_instruction_id
               JOIN basic_blocks b ON b.id=i.block_id)
             SELECT io.id AS ir_outcome_id, io.ordinal AS outcome_ordinal, d.ordinal AS decision_ordinal
             FROM summary_case_outcomes sco
             JOIN ir_outcomes io ON io.id=sco.ir_outcome_id
             JOIN decisions d ON d.id=io.decision_id WHERE sco.summary_case_id=?1 ORDER BY sco.path_ordinal",
            [case["id"].as_i64().unwrap()],
        )?;
        let ir_path: Vec<(u64, u64)> = outcomes
            .iter()
            .map(|o| {
                (
                    o["decision_ordinal"].as_u64().unwrap(),
                    o["outcome_ordinal"].as_u64().unwrap(),
                )
            })
            .collect();
        let outcome_ids = outcomes
            .iter()
            .map(|o| o["ir_outcome_id"].as_i64().unwrap())
            .collect();
        let mut memory: Value =
            serde_json::from_str(case["memory_delta_json"].as_str().unwrap_or("[]"))
                .map_err(|e| Error::new("E_DB_INTEGRITY", e.to_string()))?;
        for delta in memory
            .as_array_mut()
            .ok_or_else(|| Error::new("E_DB_INTEGRITY", "memory delta must be an array"))?
        {
            let expression_id = delta["expression_id"]
                .as_i64()
                .ok_or_else(|| Error::new("E_DB_INTEGRITY", "missing delta expression"))?;
            let hash: String = db.query_row(
                "SELECT expression_hash FROM symbolic_expressions WHERE id=?1 AND snapshot_id=?2",
                params![expression_id,snapshot], |r| r.get(0),
            ).map_err(sql)?;
            delta["expression_id"] = json!(hash);
        }
        let memory_delta = ske_common::canonical_json(&memory)?;
        let guard_hashes: Vec<String> =
            serde_json::from_str::<Vec<i64>>(case["guard_json"].as_str().unwrap_or("[]"))
                .map_err(|e| Error::new("E_DB_INTEGRITY", e.to_string()))?
                .into_iter()
                .map(|id| {
                    db.query_row(
                "SELECT expression_hash FROM symbolic_expressions WHERE id=?1 AND snapshot_id=?2",
                params![id,snapshot], |r| r.get(0),
            ).map_err(sql)
                })
                .collect::<Result<_>>()?;
        let fingerprint = ske_common::sha256_hex(ske_common::canonical_json(&json!([
            exit_kind,
            guard_hashes,
            return_keys,
            memory_delta,
            effects,
            ir_path
        ]))?);
        let span = match span {
            Some(span) => span_line_col(db, span)?,
            None => None,
        };
        inputs.push(BehaviorCaseRowInput {
            guard,
            exit_kind: exit_kind.clone(),
            result: result.clone(),
            return_keys,
            span,
            ir_path,
            outcome_ids,
            memory_delta,
            effects: serde_json::to_string(&effects)
                .map_err(|e| Error::new("E_INTERNAL", e.to_string()))?,
            precision: case["precision"].as_str().unwrap_or("UNKNOWN").to_owned(),
            fingerprint,
            summary_case_id: Some(case["id"].as_i64().unwrap()),
        });
    }
    let grouped = dedup_cases(inputs);
    let total = grouped.len();
    let mut out_cases = Vec::new();
    for (ordinal, behavior) in grouped.into_iter().enumerate() {
        let rule_key = format!("{qualified}@run{run}#case{ordinal}");
        let result_text = if behavior.exit_kind == "RETURN" {
            format!("return {}", behavior.result)
        } else {
            "panic".to_owned()
        };
        let summary_case_id = behavior.summary_case_id;
        let source_outcomes = source_outcomes_for(db, snapshot, &behavior.referenced_outcomes)?;
        let case_id = persist_case(
            db,
            snapshot,
            scenario,
            ordinal as i64,
            &rule_key,
            &behavior,
            &result_text,
        )?;
        let evidence = persist_evidence(db, snapshot, case_id, summary_case_id, &behavior)?;
        out_cases.push(json!({
            "ordinal": ordinal,
            "rule_key": rule_key,
            "when": behavior.guard,
            "then": result_text,
            "exit_kind": behavior.exit_kind,
            "precision": behavior.precision,
            "source_outcomes": source_outcomes,
            "evidence": evidence,
        }));
    }
    db.execute(
        "UPDATE analysis_runs SET status='COMPLETED',exploration_closed=1,finished_at=CURRENT_TIMESTAMP WHERE id=?1",
        [run],
    )
    .map_err(sql)?;
    let assumptions = rows(
        db,
        "SELECT kind FROM analysis_assumptions WHERE analysis_run_id=?1 ORDER BY id",
        [summary_run],
    )?;
    // §7.5: a mono-service scenario is an explicit technical grouping, never
    // an automatically invented or human-validated business denomination.
    let unit_key: String = db
        .query_row(
            "SELECT u.unit_key FROM functions f JOIN symbols s ON s.id=f.symbol_id
         JOIN compilation_units u ON u.id=s.unit_id WHERE f.id=?1",
            [entry],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let capability_key = format!("function:{unit_key}|{qualified}");
    db.execute(
        "INSERT INTO functional_capabilities(snapshot_id,canonical_key,origin,validation_status)
         VALUES(?1,?2,'DETERMINISTIC','UNVALIDATED') ON CONFLICT DO NOTHING",
        params![snapshot, capability_key],
    )
    .map_err(sql)?;
    let capability: i64 = db
        .query_row(
            "SELECT id FROM functional_capabilities WHERE snapshot_id=?1 AND canonical_key=?2",
            params![snapshot, capability_key],
            |r| r.get(0),
        )
        .map_err(sql)?;
    db.execute(
        "INSERT INTO capability_scenarios(capability_id,scenario_id) VALUES(?1,?2)",
        params![capability, scenario],
    )
    .map_err(sql)?;
    let unknown_effects = rows(db,
        "SELECT e.effect_kind,e.target_descriptor,e.payload FROM observable_effects e
         JOIN summary_cases c ON c.id=e.summary_case_id WHERE c.summary_id=?1 AND e.effect_kind='UNKNOWN_EFFECT'
         ORDER BY c.case_ordinal,e.effect_ordinal",[summary_id])?;
    if !unknown_effects.is_empty() {
        db.execute(
            "INSERT INTO scenario_links(from_scenario_id,relation_kind,confidence_kind,evidence_json,assumptions_json)
             VALUES(?1,'EXTERNAL_UNKNOWN','UNKNOWN',?2,?3)",
            params![scenario,serde_json::to_string(&unknown_effects).unwrap(),
                serde_json::to_string(&assumptions).unwrap()],
        ).map_err(sql)?;
    }
    let response = json!({
        "function": qualified,
        "run_id": run,
        "summary_run_id": summary_run,
        "precision": precision,
        "scenario": {"id":scenario,"complete":unknown_effects.is_empty(),"external_boundary":
            if unknown_effects.is_empty() { Value::Null } else { json!("EXTERNAL_UNKNOWN") }},
        "capability": {"canonical_key":capability_key,"title":null,"origin":"DETERMINISTIC",
            "validation_status":"UNVALIDATED"},
        "assumptions": json!(assumptions.iter().map(|a| a["kind"].clone()).collect::<Vec<_>>()),
        "cases": out_cases.into_iter().skip(offset).take(limit).collect::<Vec<_>>(),
        "page": super::page(limit, offset, total),
    });
    tx.commit().map_err(sql)?;
    Ok(response)
}

/// Persist one behavior case (+ its IR outcomes) and its functional rule.
#[allow(clippy::too_many_arguments)]
fn persist_case(
    db: &Connection,
    snapshot: i64,
    scenario: i64,
    ordinal: i64,
    rule_key: &str,
    behavior: &ske_behavior::BehaviorCaseRow,
    result_text: &str,
) -> Result<i64> {
    db.execute(
        "INSERT INTO behavior_cases(behavior_id,summary_case_id,case_ordinal,guard_text,result_text,effects_text,precision)
         VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            scenario,
            behavior.summary_case_id,
            ordinal,
            behavior.guard,
            result_text,
            behavior.effects,
            behavior.precision
        ],
    )
    .map_err(sql)?;
    let case_id = db.last_insert_rowid();
    for id in &behavior.referenced_outcomes {
        outcome_identity(db, snapshot, *id)?;
        db.execute(
                "INSERT OR IGNORE INTO behavior_case_outcomes(behavior_case_id,ir_outcome_id) VALUES(?1,?2)",
                params![case_id, id],
            )
            .map_err(sql)?;
    }
    db.execute(
        "INSERT INTO functional_rules(snapshot_id,behavior_case_id,rule_key,short_name,description,origin,validation_status)
         VALUES(?1,?2,?3,?4,?5,
           CASE WHEN EXISTS(
             SELECT 1 FROM behavior_scenarios bs JOIN analysis_assumptions a ON a.analysis_run_id=bs.analysis_run_id
             WHERE bs.id=?6 AND a.kind='LOOP_INTERVAL_ABSTRACTION')
           THEN 'ABSTRACT_DERIVED' ELSE 'DETERMINISTIC' END,'UNVALIDATED')",
        params![
            snapshot,
            case_id,
            rule_key,
            behavior.guard,
            format!("WHEN {} THEN {result_text}", behavior.guard),
            scenario
        ],
    )
    .map_err(sql)?;
    Ok(case_id)
}

/// Persist evidence anchors: DERIVED_FROM (summary case), GUARD_OUTCOME (one per
/// borrowed IR outcome), RESULT_SITE (exit span). Returns the JSON evidence list.
fn persist_evidence(
    db: &Connection,
    snapshot: i64,
    case_id: i64,
    _summary_case_id: Option<i64>,
    behavior: &ske_behavior::BehaviorCaseRow,
) -> Result<Vec<Value>> {
    let rule_id: i64 = db
        .query_row(
            "SELECT id FROM functional_rules WHERE behavior_case_id=?1",
            [case_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let mut evidence = Vec::new();
    for sid in &behavior.summary_case_ids {
        db.execute(
            "INSERT INTO knowledge_evidence(functional_rule_id,summary_case_id,relation) VALUES(?1,?2,'DERIVED_FROM')",
            params![rule_id, sid],
        )
        .map_err(sql)?;
        evidence.push(summary_evidence(
            db,
            snapshot,
            *sid,
            &json!("DERIVED_FROM"),
        )?);
    }
    for ir_id in &behavior.referenced_outcomes {
        let (label, unit_key) = outcome_identity(db, snapshot, *ir_id)?;
        db.execute(
            "INSERT INTO knowledge_evidence(functional_rule_id,ir_outcome_id,relation) VALUES(?1,?2,'GUARD_OUTCOME')",
            params![rule_id, ir_id],
        )
        .map_err(sql)?;
        evidence.push(
            json!({"kind":"ir_outcome","id":label,"unit_key":unit_key,"relation":"GUARD_OUTCOME"}),
        );
    }
    let mut seen_spans = std::collections::BTreeSet::new();
    for sid in &behavior.summary_case_ids {
        let span: Option<i64> = db
            .query_row(
                "SELECT exit_span_id FROM summary_cases WHERE id=?1",
                [sid],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        if let Some(span_id) = span {
            if !seen_spans.insert(span_id) {
                continue;
            }
            db.execute(
                "INSERT INTO knowledge_evidence(functional_rule_id,source_span_id,relation) VALUES(?1,?2,'RESULT_SITE')",
                params![rule_id, span_id],
            )
            .map_err(sql)?;
            let span_text = span_text(db, span_id)?;
            evidence.push(json!({"kind":"source_span","span":span_text,"relation":"RESULT_SITE"}));
        }
    }
    Ok(evidence)
}

/// Name the persisted summary producer, not the separate BEHAVIOR run.
fn summary_evidence(db: &Connection, snapshot: i64, case: i64, relation: &Value) -> Result<Value> {
    let source = super::one(
        db,
        "SELECT s.qualified_name,fs.produced_by_run_id,c.case_ordinal
        FROM summary_cases c JOIN function_summaries fs ON fs.id=c.summary_id
        JOIN functions f ON f.id=fs.function_id JOIN symbols s ON s.id=f.symbol_id
        JOIN analysis_runs r ON r.id=fs.produced_by_run_id
        WHERE c.id=?1 AND s.snapshot_id=?2 AND r.snapshot_id=?2",
        params![case, snapshot],
    )?;
    let producer = &source["produced_by_run_id"];
    Ok(json!({"kind":"summary_case","id":format!("{}@run{}#case{}",
        source["qualified_name"].as_str().unwrap(),producer,source["case_ordinal"]),
        "summary_case_id":case,"producer_run_id":producer,"relation":relation}))
}

fn outcome_identity(db: &Connection, snapshot: i64, id: i64) -> Result<(String, String)> {
    let (stored,unit): (String,String) = db.query_row(
        "SELECT e.local_key,u.unit_key FROM ir_outcomes io JOIN control_edges e ON e.id=io.control_edge_id
         JOIN basic_blocks b ON b.id=e.source_block_id JOIN functions f ON f.id=b.function_id
         JOIN symbols s ON s.id=f.symbol_id JOIN compilation_units u ON u.id=s.unit_id
         WHERE io.id=?1 AND s.snapshot_id=?2", params![id,snapshot],
        |r| Ok((r.get(0)?,r.get(1)?)),
    ).map_err(sql)?;
    let label = stored
        .strip_prefix(&format!("{unit}|"))
        .ok_or_else(|| Error::new("E_DB_INTEGRITY", "outcome outside unit"))?
        .to_owned();
    Ok((label, unit))
}

/// Build one public rule item from its raw `functional_rules` row, optionally
/// attaching evidence anchors (§11.2). The input row must carry the columns
/// selected by `rule_columns`.
fn enrich_rule(db: &Connection, snapshot: i64, with_evidence: bool, rule: &Value) -> Result<Value> {
    let rule_id = rule["id"].as_i64().unwrap();
    let mut item = rule.clone();
    item.as_object_mut().unwrap().remove("id");
    item.as_object_mut().unwrap().remove("behavior_case_id");
    if item.get("prompt_version").and_then(Value::as_str).is_none() {
        item.as_object_mut().unwrap().remove("prompt_version");
        item.as_object_mut().unwrap().remove("llm_model");
        item.as_object_mut().unwrap().remove("model_parameters");
    }
    if with_evidence {
        let evidence = rows(
            db,
            "SELECT ke.relation,
                    COALESCE(sp_span.span,'') AS span,
                    ke.ir_outcome_id, ke.summary_case_id, ke.source_span_id
             FROM knowledge_evidence ke
             LEFT JOIN (SELECT ss.id, sf.path||':'||ss.start_line||':'||ss.start_column||'-'||ss.end_line||':'||ss.end_column AS span
                        FROM source_spans ss JOIN source_files sf ON sf.id=ss.source_file_id) sp_span
               ON sp_span.id=ke.source_span_id
             WHERE ke.functional_rule_id=?1 ORDER BY ke.id",
            [rule_id],
        )?;
        let mut anchors = Vec::new();
        for e in evidence {
            let relation = e["relation"].clone();
            if let Some(id) = e["ir_outcome_id"].as_i64() {
                let (label, unit) = outcome_identity(db, snapshot, id)?;
                anchors.push(
                    json!({"kind":"ir_outcome","id":label,"unit_key":unit,"relation":relation}),
                );
            } else if let Some(id) = e["summary_case_id"].as_i64() {
                anchors.push(summary_evidence(db, snapshot, id, &relation)?);
            } else if e["source_span_id"].is_i64() {
                anchors.push(json!({"kind":"source_span","span":e["span"],"relation":relation}));
            } else {
                return Err(Error::new("E_DB_INTEGRITY", "unsupported evidence anchor"));
            }
        }
        item["evidence"] = json!(anchors);
    }
    Ok(item)
}

/// Row columns shared by `list_rules`, `rule_by_key` and
/// `list_rules_for_function` so all produce the same enriched item (§11.2).
/// `alias` qualifies every column so a JOIN never hits an ambiguous id
/// (`fr` when joining against behavior_cases/scenarios, empty otherwise).
fn rule_columns(alias: &str) -> String {
    let q = if alias.is_empty() {
        String::new()
    } else {
        format!("{alias}.")
    };
    format!(
        "{q}id,{q}rule_key,COALESCE({q}short_name,'') AS short_name,{q}description,\
         {q}origin,{q}validation_status,{q}behavior_case_id,{q}prompt_version,{q}llm_model,{q}model_parameters"
    )
}

/// Fetch one rule by its `rule_key` (snapshot-scoped) and enrich it. The raw
/// row is loaded directly by key (E_NOT_FOUND if absent), so single-rule
/// lookups avoid the rank-count and full relist that `list_rules(1, offset)`
/// used to require.
pub(crate) fn rule_by_key(
    db: &Connection,
    snapshot: i64,
    key: &str,
    with_evidence: bool,
) -> Result<Value> {
    let rule = super::one(
        db,
        &format!(
            "SELECT {} FROM functional_rules WHERE snapshot_id=?1 AND rule_key=?2",
            rule_columns("")
        ),
        params![snapshot, key],
    )?;
    enrich_rule(db, snapshot, with_evidence, &rule)
}

/// List functional rules for a snapshot, optionally with evidence (§11.2).
/// Rules are loaded once (ordered by `rule_key`) and each page row is enriched
/// in memory; pagination is by row position, identical to the previous contract.
pub fn list_rules(
    db: &Connection,
    snapshot: i64,
    with_evidence: bool,
    limit: usize,
    offset: usize,
) -> Result<Value> {
    let rules = rows(
        db,
        &format!(
            "SELECT {} FROM functional_rules WHERE snapshot_id=?1 ORDER BY rule_key",
            rule_columns("")
        ),
        [snapshot],
    )?;
    let total = rules.len();
    let page_rows = rules
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    for rule in &page_rows {
        out.push(enrich_rule(db, snapshot, with_evidence, rule)?);
    }
    Ok(json!({"snapshot_id":snapshot,"rules":out,"page":super::page(limit, offset, total)}))
}

/// List rules whose scenario is anchored on `function` (v0.7, AC-0.7-03/05).
/// One query loads the function's rule rows ordered by `rule_key`; each page
/// row is enriched in memory. This replaces the former per-rule
/// `get_rule_evidence` loop (a rank count + full relist per rule).
pub(crate) fn list_rules_for_function(
    db: &Connection,
    snapshot: i64,
    function: i64,
    with_evidence: bool,
    limit: usize,
    offset: usize,
) -> Result<Value> {
    let rules = rows(
        db,
        &format!(
            "SELECT {} FROM functional_rules fr
             JOIN behavior_cases bc ON bc.id=fr.behavior_case_id
             JOIN behavior_scenarios bs ON bs.id=bc.behavior_id
             WHERE fr.snapshot_id=?1 AND bs.entry_function_id=?2 ORDER BY fr.rule_key",
            rule_columns("fr")
        ),
        params![snapshot, function],
    )?;
    let total = rules.len();
    let page_rows = rules
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    for rule in &page_rows {
        out.push(enrich_rule(db, snapshot, with_evidence, rule)?);
    }
    Ok(json!({"snapshot_id":snapshot,"rules":out,"page":super::page(limit, offset, total)}))
}

/// `ske rules stale-check --repository NAME --from A --to B`: rules of A whose
/// function vanished from B or whose `structural_fingerprint` changed become
/// `STALE` (never re-attributed to B). `from`/`to` are snapshot ids or `latest`.
pub fn stale_check(db: &Connection, repository: &str, from: &str, to: &str) -> Result<Value> {
    let tx = db.unchecked_transaction().map_err(sql)?;
    let db: &Connection = &tx;
    let repo: i64 = db
        .query_row(
            "SELECT id FROM repositories WHERE name=?1",
            [repository],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let from_snapshot = resolve_snapshot(db, repo, from)?;
    let to_snapshot = resolve_snapshot(db, repo, to)?;
    // Rules are always attributed to the snapshot they were produced for (A);
    // STALE is set on A's rows, never copied to B.
    let mut stale = 0usize;
    let mut unchanged = 0usize;
    let rules = rows(
        db,
        "SELECT fr.id, f.symbol_id, f.id AS function_id
         FROM functional_rules fr
         JOIN behavior_cases b ON b.id=fr.behavior_case_id
         JOIN behavior_scenarios bs ON bs.id=b.behavior_id
         JOIN functions f ON f.id=bs.entry_function_id
         WHERE fr.snapshot_id=?1",
        [from_snapshot],
    )?;
    for rule in rules {
        let rule_id = rule["id"].as_i64().unwrap();
        let symbol_id = rule["symbol_id"].as_i64().unwrap();
        let a_function = rule["function_id"].as_i64().unwrap();
        // Stale if the function vanished from B, or its structural fingerprint
        // changed. Match the function in B by qualified_name within its unit,
        // then compare §4.6 structural fingerprints (canonical body hash).
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
            let a = dependency_structure(db, a_function).ok();
            let b = dependency_structure(db, b_function).ok();
            match (a, b) {
                (Some(a), Some(b)) => changed = a != b,
                _ => changed = true,
            }
        }
        if b_function.is_none() || changed {
            db.execute(
                "UPDATE functional_rules SET validation_status='STALE' WHERE id=?1 AND validation_status<>'STALE'",
                [rule_id],
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
        "rules_marked_stale": stale,
        "rules_unchanged": unchanged,
    });
    tx.commit().map_err(sql)?;
    Ok(response)
}

/// Finite closure of known callees and profiles, including recursive SCCs.
/// A changed callee invalidates composed rules without rerunning the solver.
fn dependency_structure(
    db: &Connection,
    entry: i64,
) -> Result<std::collections::BTreeMap<(String, String), String>> {
    let mut result = std::collections::BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut pending = vec![entry];
    while let Some(function) = pending.pop() {
        if !seen.insert(function) {
            continue;
        }
        let identity=super::one(db,"SELECT s.qualified_name,u.unit_key,u.semantic_profile_hash,u.build_fingerprint
            FROM functions f JOIN symbols s ON s.id=f.symbol_id JOIN compilation_units u ON u.id=s.unit_id WHERE f.id=?1",[function])?;
        let structure = super::summaries::structural(&super::analysis_load::load(db, function)?);
        result.insert(
            (
                identity["unit_key"].as_str().unwrap().to_owned(),
                identity["qualified_name"].as_str().unwrap().to_owned(),
            ),
            ske_common::sha256_hex(ske_common::canonical_json(&json!([
                structure,
                identity["semantic_profile_hash"],
                identity["build_fingerprint"]
            ]))?),
        );
        for callee in rows(db,"SELECT DISTINCT target.id FROM call_sites cs JOIN instructions i ON i.id=cs.instruction_id
            JOIN basic_blocks b ON b.id=i.block_id JOIN call_targets ct ON ct.call_site_id=cs.id
            JOIN functions stub ON stub.id=ct.target_function_id
            JOIN functions target ON target.id=COALESCE(stub.resolved_function_id,stub.id)
            WHERE b.function_id=?1 AND target.is_external=0",[function])? {
            pending.push(callee["id"].as_i64().unwrap());
        }
    }
    Ok(result)
}

/// Public re-exports for the capabilities module (same repository-scoped
/// semantics as the rules stale-check; kept private to query internals).
pub(crate) fn resolve_snapshot_public(db: &Connection, repo: i64, selector: &str) -> Result<i64> {
    resolve_snapshot(db, repo, selector)
}
pub(crate) fn dependency_structure_public(
    db: &Connection,
    entry: i64,
) -> Result<std::collections::BTreeMap<(String, String), String>> {
    dependency_structure(db, entry)
}

/// Resolve a snapshot selector (integer id or `latest`) within a repository.
fn resolve_snapshot(db: &Connection, repo: i64, selector: &str) -> Result<i64> {
    if selector != "latest" {
        if let Ok(id) = selector.parse::<i64>() {
            let found = db
                .query_row(
                    "SELECT id FROM snapshots WHERE id=?1 AND repository_id=?2 AND status='CLOSED'",
                    params![id, repo],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql)?;
            if let Some(found) = found {
                return Ok(found);
            }
        }
        return Err(Error::new("E_NOT_FOUND", format!("snapshot {selector}")));
    }
    db.query_row(
        "SELECT id FROM snapshots WHERE repository_id=?1 AND status='CLOSED' ORDER BY id DESC LIMIT 1",
        [repo],
        |r| r.get(0),
    )
    .map_err(sql)
}

/// One LLM-inferred rule to persist (AC-0.7-08). Provenance fields travel with
/// the row; the deterministic rules are never modified.
pub struct LlmRuleInsert {
    pub rule_key: String,
    pub short_name: String,
    pub description: String,
    pub evidence_refs: Vec<String>,
    pub llm_model: String,
    pub model_parameters: String,
}

/// Persist LLM-inferred rules (§7.4, AC-0.7-08) with explicit provenance:
/// `origin='LLM_INFERRED'`, `validation_status='UNVALIDATED'`, `prompt_version`,
/// `llm_model`, `model_parameters`. Rules are inserted without a deterministic
/// behavior case (they are not derivations), and a `LLM_INFERRED` row is never
/// referenceable as a proof (`prop_no_llm_proof`). The caller's `prompt_version`
/// is recorded verbatim so re-runs are auditable. Deterministic rows are
/// untouched (the INSERT targets only fresh rule keys; a duplicate key is an
/// error, never an overwrite).
pub fn insert_llm_rules(
    db: &Connection,
    snapshot: i64,
    rules: &[LlmRuleInsert],
    prompt_version: &str,
) -> Result<Value> {
    let tx = db.unchecked_transaction().map_err(sql)?;
    let db: &Connection = &tx;
    let mut inserted = Vec::new();
    for rule in rules {
        // References are deterministic rule keys from this exact snapshot,
        // never model-created proof ids or keys from another snapshot.
        if rule.evidence_refs.is_empty() {
            return Err(Error::new(
                "E_USAGE",
                "LLM rule requires deterministic evidence",
            ));
        }
        let mut evidence_rules = Vec::new();
        for key in &rule.evidence_refs {
            let id: Option<i64> = db.query_row(
                "SELECT id FROM functional_rules WHERE snapshot_id=?1 AND rule_key=?2
                 AND origin != 'LLM_INFERRED'
                 AND EXISTS(SELECT 1 FROM knowledge_evidence WHERE functional_rule_id=functional_rules.id)",
                params![snapshot, key], |r| r.get(0),
            ).optional().map_err(sql)?;
            evidence_rules.push(id.ok_or_else(|| {
                Error::new("E_USAGE", "invalid deterministic evidence reference")
            })?);
        }
        db.execute(
            "INSERT INTO functional_rules(
                snapshot_id,behavior_case_id,rule_key,short_name,description,
                origin,validation_status,prompt_version,llm_model,model_parameters)
             VALUES(?1,NULL,?2,?3,?4,'LLM_INFERRED','UNVALIDATED',?5,?6,?7)",
            params![
                snapshot,
                rule.rule_key,
                rule.short_name,
                rule.description,
                prompt_version,
                rule.llm_model,
                rule.model_parameters
            ],
        )
        .map_err(sql)?;
        let rule_id = db.last_insert_rowid();
        for source_rule in evidence_rules {
            db.execute(
                "INSERT INTO knowledge_evidence(functional_rule_id,source_span_id,instruction_id,ir_outcome_id,summary_case_id,relation)
                 SELECT ?1,source_span_id,instruction_id,ir_outcome_id,summary_case_id,relation
                 FROM knowledge_evidence WHERE functional_rule_id=?2",
                params![rule_id, source_rule],
            ).map_err(sql)?;
        }
        inserted.push(json!({
            "rule_key": rule.rule_key,
            "origin": "LLM_INFERRED",
            "validation_status": "UNVALIDATED",
            "prompt_version": prompt_version,
            "llm_model": rule.llm_model,
            "model_parameters": rule.model_parameters,
            "evidence_refs": rule.evidence_refs,
        }));
    }
    tx.commit().map_err(sql)?;
    Ok(json!({"snapshot_id": snapshot, "inserted": inserted, "count": inserted.len()}))
}
