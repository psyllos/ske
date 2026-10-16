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
fn source_outcomes_for(db: &Connection, snapshot: i64, ir_labels: &[String]) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    for label in ir_labels {
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
             JOIN compilation_units u ON u.id=s.unit_id
             WHERE e.local_key=u.unit_key||'|'||?1 AND s.snapshot_id=?2",
            params![label, snapshot],
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
         config_fingerprint,assumptions_fingerprint,status) VALUES(?1,?2,'BEHAVIOR','ske-behavior-0.5',?3,?3,'RUNNING')",
        params![snapshot, entry, "none"],
    )
    .map_err(sql)?;
    let run = db.last_insert_rowid();
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
         FROM summary_cases WHERE summary_id=?1 ORDER BY case_ordinal",
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
        let outcomes: Vec<String> = rows(
            db,
            "SELECT o.local_key AS ir_outcome FROM summary_case_outcomes sco
             JOIN ir_outcomes io ON io.id=sco.ir_outcome_id
             JOIN control_edges o ON o.id=io.control_edge_id WHERE sco.summary_case_id=?1 ORDER BY sco.path_ordinal",
            [case["id"].as_i64().unwrap()],
        )?
        .iter()
        .map(|o| local(&o["ir_outcome"]).as_str().unwrap_or("").to_owned())
        .collect();
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
            ir_path: outcomes.clone(),
            memory_delta: case["memory_delta_json"]
                .as_str()
                .unwrap_or("[]")
                .to_owned(),
            effects: serde_json::to_string(&effects)
                .map_err(|e| Error::new("E_INTERNAL", e.to_string()))?,
            precision: case["precision"].as_str().unwrap_or("UNKNOWN").to_owned(),
            fingerprint: format!(
                "{}@case{}",
                qualified,
                case["case_ordinal"].as_i64().unwrap()
            ),
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
        "UPDATE analysis_runs SET status='COMPLETED',finished_at=CURRENT_TIMESTAMP WHERE id=?1",
        [run],
    )
    .map_err(sql)?;
    let assumptions = rows(
        db,
        "SELECT kind FROM analysis_assumptions WHERE analysis_run_id=?1 ORDER BY id",
        [summary_run],
    )?;
    let response = json!({
        "function": qualified,
        "run_id": run,
        "precision": precision,
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
    for label in &behavior.referenced_outcomes {
        let id: Option<i64> = db
            .query_row(
                "SELECT io.id FROM ir_outcomes io JOIN control_edges e ON e.id=io.control_edge_id
                 JOIN basic_blocks b ON b.id=e.source_block_id
                 JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id
                 JOIN compilation_units u ON u.id=s.unit_id
                 WHERE e.local_key=u.unit_key||'|'||?1 AND s.snapshot_id=?2",
                params![label, snapshot],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        if let Some(id) = id {
            db.execute(
                "INSERT OR IGNORE INTO behavior_case_outcomes(behavior_case_id,ir_outcome_id) VALUES(?1,?2)",
                params![case_id, id],
            )
            .map_err(sql)?;
        }
    }
    db.execute(
        "INSERT INTO functional_rules(snapshot_id,behavior_case_id,rule_key,short_name,description,origin,validation_status)
         VALUES(?1,?2,?3,?4,?5,'DETERMINISTIC','UNVALIDATED')",
        params![
            snapshot,
            case_id,
            rule_key,
            behavior.guard,
            format!("WHEN {} THEN {result_text}", behavior.guard)
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
    summary_case_id: Option<i64>,
    behavior: &ske_behavior::BehaviorCaseRow,
) -> Result<Vec<Value>> {
    let rule_id: i64 = db
        .query_row(
            "SELECT id FROM functional_rules WHERE behavior_case_id=?1",
            [case_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let rule_key: String = db
        .query_row(
            "SELECT rule_key FROM functional_rules WHERE id=?1",
            [rule_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let mut evidence = Vec::new();
    if let Some(sid) = summary_case_id {
        db.execute(
            "INSERT INTO knowledge_evidence(functional_rule_id,summary_case_id,relation) VALUES(?1,?2,'DERIVED_FROM')",
            params![rule_id, sid],
        )
        .map_err(sql)?;
        evidence.push(json!({"kind":"summary_case","id":rule_key,"relation":"DERIVED_FROM"}));
    }
    for label in &behavior.referenced_outcomes {
        let ir_id: Option<i64> = db
            .query_row(
                "SELECT io.id FROM ir_outcomes io JOIN control_edges e ON e.id=io.control_edge_id
                 JOIN basic_blocks b ON b.id=e.source_block_id
                 JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id
                 JOIN compilation_units u ON u.id=s.unit_id
                 WHERE e.local_key=u.unit_key||'|'||?1 AND s.snapshot_id=?2",
                params![label, snapshot],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        let Some(ir_id) = ir_id else { continue };
        db.execute(
            "INSERT INTO knowledge_evidence(functional_rule_id,ir_outcome_id,relation) VALUES(?1,?2,'GUARD_OUTCOME')",
            params![rule_id, ir_id],
        )
        .map_err(sql)?;
        let unit_key: String = db
            .query_row(
                "SELECT u.unit_key FROM ir_outcomes io JOIN ir_decisions d ON d.id=io.decision_id
                 JOIN instructions i ON i.id=d.terminator_instruction_id
                 JOIN basic_blocks b ON b.id=i.block_id
                 JOIN functions f ON f.id=b.function_id
                 JOIN symbols s ON s.id=f.symbol_id
                 JOIN compilation_units u ON u.id=s.unit_id WHERE io.id=?1",
                [ir_id],
                |r| r.get(0),
            )
            .map_err(sql)?;
        evidence.push(
            json!({"kind":"ir_outcome","id":label,"unit_key":unit_key,"relation":"GUARD_OUTCOME"}),
        );
    }
    if let Some(sid) = summary_case_id {
        let span: Option<i64> = db
            .query_row(
                "SELECT exit_span_id FROM summary_cases WHERE id=?1",
                [sid],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        if let Some(span_id) = span {
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

/// List functional rules for a snapshot, optionally with evidence (§11.2).
pub fn list_rules(
    db: &Connection,
    snapshot: i64,
    with_evidence: bool,
    limit: usize,
    offset: usize,
) -> Result<Value> {
    let rules = rows(
        db,
        "SELECT id,rule_key,COALESCE(short_name,'') AS short_name,description,
                origin,validation_status,behavior_case_id
         FROM functional_rules WHERE snapshot_id=?1 ORDER BY rule_key",
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
        let rule_id = rule["id"].as_i64().unwrap();
        let mut item = rule.clone();
        item.as_object_mut().unwrap().remove("id");
        item.as_object_mut().unwrap().remove("behavior_case_id");
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
            item["evidence"] = json!(evidence
                .iter()
                .map(|e| {
                    let mut v = e.clone();
                    v.as_object_mut().unwrap().remove("span");
                    if !e["span"].as_str().unwrap_or("").is_empty() {
                        v["kind"] = json!("source_span");
                        v["span"] = e["span"].clone();
                    } else if e["ir_outcome_id"].is_i64() {
                        v["kind"] = json!("ir_outcome");
                        v["id"] = json!(e["ir_outcome_id"]);
                        v.as_object_mut().unwrap().remove("ir_outcome_id");
                    } else if e["summary_case_id"].is_i64() {
                        v["kind"] = json!("summary_case");
                        v["id"] = json!(e["summary_case_id"]);
                        v.as_object_mut().unwrap().remove("summary_case_id");
                    }
                    v.as_object_mut().unwrap().remove("source_span_id");
                    v
                })
                .collect::<Vec<_>>());
        }
        out.push(item);
    }
    Ok(json!({"snapshot_id":snapshot,"rules":out,"page":super::page(limit, offset, total)}))
}

/// `ske rules stale-check --repository NAME --from A --to B`: rules of A whose
/// function vanished from B or whose `structural_fingerprint` changed become
/// `STALE` (never re-attributed to B). `from`/`to` are snapshot ids or `latest`.
pub fn stale_check(db: &Connection, repository: &str, from: &str, to: &str) -> Result<Value> {
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
        let b_function: Option<i64> = db
            .query_row(
                "SELECT f2.id FROM functions f2
                 JOIN symbols s2 ON s2.id=f2.symbol_id
                 JOIN symbols s1 ON s1.id=?1
                 JOIN compilation_units u2 ON u2.id=s2.unit_id
                 WHERE u2.snapshot_id=?2 AND s2.qualified_name=s1.qualified_name
                   AND s2.unit_id IN (SELECT id FROM compilation_units WHERE snapshot_id=?2)",
                params![symbol_id, to_snapshot],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        let mut changed = false;
        if let Some(b_function) = b_function {
            let a = super::analysis_load::load(db, a_function).ok();
            let b = super::analysis_load::load(db, b_function).ok();
            match (a, b) {
                (Some(a), Some(b)) => {
                    changed = super::summaries::structural(&a) != super::summaries::structural(&b)
                }
                _ => changed = true,
            }
        }
        if b_function.is_none() || changed {
            db.execute(
                "UPDATE functional_rules SET validation_status='STALE' WHERE id=?1 AND validation_status='UNVALIDATED'",
                [rule_id],
            )
            .map_err(sql)?;
            stale += 1;
        } else {
            unchanged += 1;
        }
    }
    Ok(json!({
        "repository": repository,
        "from": from_snapshot,
        "to": to_snapshot,
        "rules_marked_stale": stale,
        "rules_unchanged": unchanged,
    }))
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
