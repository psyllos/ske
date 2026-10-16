use rusqlite::{params, Connection, TransactionBehavior};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayResult {
    Unsat,
    Sat,
    Unknown,
    Timeout,
}

#[derive(Debug)]
pub struct Verification {
    pub proof_id: i64,
    pub accepted: bool,
    pub reason: String,
    pub verification_level: &'static str,
    pub model_soundness: &'static str,
}

/// Reread persisted facts under an immediate transaction, then record proof BEFORE
/// result. `replay` must construct a fresh solver for EACH call, use the supplied
/// seed, and must not persist queries. No classifier state is accepted.
/// SQL/operational failures roll back; non-UNSAT replay (including adapter errors
/// represented as Unknown) produces a durable rejected proof.
pub fn verify_unreachability(
    conn: &mut Connection,
    run_id: i64,
    outcome_id: i64,
    mut replay: impl FnMut(&str, u32) -> ReplayResult,
) -> rusqlite::Result<Verification> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (status, closed, cuts, same_snapshot): (String, bool, i64, bool) = tx.query_row(
        "SELECT r.status,r.exploration_closed,r.cut_count,r.snapshot_id=u.snapshot_id
         FROM analysis_runs r JOIN ir_outcomes o ON o.id=?2
         JOIN control_edges e ON e.id=o.control_edge_id
         JOIN instructions i ON i.id=e.terminator_instruction_id
         JOIN basic_blocks b ON b.id=i.block_id JOIN functions f ON f.id=b.function_id
         JOIN symbols s ON s.id=f.symbol_id
         JOIN compilation_units u ON u.id=s.unit_id WHERE r.id=?1",
        params![run_id, outcome_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let mut reason = if status != "COMPLETED" || !closed || cuts != 0 {
        Some("EXPLORATION_NOT_CLOSED".to_owned())
    } else if !same_snapshot {
        Some("OUTCOME_SNAPSHOT_MISMATCH".to_owned())
    } else {
        None
    };
    let assumptions: i64 = tx.query_row(
        "SELECT count(DISTINCT kind) FROM analysis_assumptions WHERE analysis_run_id=?1
         AND kind IN ('SEQUENTIAL_EXECUTION','CLOSED_ENTRY_CONTEXT')",
        [run_id],
        |r| r.get(0),
    )?;
    if assumptions != 2 {
        reason.get_or_insert("MISSING_ASSUMPTIONS".into());
    }
    // A declared closed run cannot contain unresolved global arrivals.
    let unresolved: i64 = tx.query_row(
        "SELECT count(*) FROM outcome_arrivals WHERE analysis_run_id=?1
         AND result IN ('UNKNOWN','TIMEOUT','UNSUPPORTED')",
        [run_id],
        |r| r.get(0),
    )?;
    if unresolved != 0 {
        reason.get_or_insert("GLOBAL_UNRESOLVED_ARRIVAL".into());
    }
    let mut soundness = 0usize;
    let mut count = 0i64;
    {
        let mut stmt = tx.prepare(
            "SELECT a.result,a.soundness,q.result,q.analysis_run_id
             FROM outcome_arrivals a LEFT JOIN solver_queries q ON q.id=a.query_id
             WHERE a.analysis_run_id=?1 AND a.ir_outcome_id=?2 ORDER BY a.arrival_seq",
        )?;
        let rows = stmt.query_map(params![run_id, outcome_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })?;
        for row in rows {
            let (result, precision, query_result, query_run) = row?;
            count += 1;
            let rank = match precision.as_str() {
                "EXACT" => 0,
                "OVERAPPROX" => 1,
                "OPAQUE" => 2,
                _ => 3,
            };
            soundness = soundness.max(rank);
            if result != "UNSAT" {
                reason.get_or_insert("ARRIVAL_NOT_UNSAT".into());
            }
            if rank == 3 {
                reason.get_or_insert("UNSUPPORTED_SOUNDNESS".into());
            }
            if query_result.as_deref() != Some("UNSAT") || query_run != Some(run_id) {
                reason.get_or_insert("INVALID_ARRIVAL_QUERY".into());
            }
        }
    }
    // Deliberately run-wide, including upstream eliminations for zero-arrival outcomes.
    {
        let mut stmt = tx.prepare(
            "SELECT id,smtlib FROM solver_queries WHERE analysis_run_id=?1
             AND result='UNSAT' ORDER BY id",
        )?;
        for row in stmt.query_map([run_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
        })? {
            let (id, text) = row?;
            match text {
                Some(text) if replay(&text, 1) == ReplayResult::Unsat => {}
                _ => {
                    reason.get_or_insert(format!("REPLAY_NOT_UNSAT:{id}"));
                }
            }
        }
    }
    let accepted = reason.is_none();
    let reason = reason.unwrap_or_else(|| {
        if count == 0 {
            "NO_ARRIVALS_CLOSED_EXPLORATION".into()
        } else {
            "ALL_ARRIVALS_UNSAT".into()
        }
    });
    let precision = ["EXACT", "OVERAPPROX", "OPAQUE", "UNSUPPORTED"][soundness];
    tx.execute(
        "INSERT INTO proof_records(analysis_run_id,ir_outcome_id,verifier_name,
         verifier_version,verdict,verification_level,model_soundness,
         eliminated_arrivals,residual_frontier,reason)
         VALUES(?1,?2,'ske-verify',?3,?4,?5,?6,?7,?8,?9)",
        params![
            run_id,
            outcome_id,
            env!("CARGO_PKG_VERSION"),
            if accepted { "ACCEPTED" } else { "REJECTED" },
            if accepted {
                "MODEL_UNREACHABLE"
            } else {
                "SOLVER_REPLAYED"
            },
            precision,
            count,
            if accepted { 0 } else { 1 },
            reason
        ],
    )?;
    let proof_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO branch_analysis_results(analysis_run_id,ir_outcome_id,status,
         soundness,explanation,proof_id) VALUES(?1,?2,?3,?4,?5,?6)
         ON CONFLICT(analysis_run_id,ir_outcome_id) DO UPDATE SET
         status=excluded.status,soundness=excluded.soundness,
         explanation=excluded.explanation,proof_id=excluded.proof_id",
        params![
            run_id,
            outcome_id,
            if accepted {
                "PROVEN_UNREACHABLE"
            } else {
                "UNKNOWN"
            },
            precision,
            if accepted {
                reason.clone()
            } else {
                format!("PROOF_REJECTED:{reason}")
            },
            if accepted { Some(proof_id) } else { None }
        ],
    )?;
    tx.commit()?;
    Ok(Verification {
        proof_id,
        accepted,
        reason,
        verification_level: if accepted {
            "MODEL_UNREACHABLE"
        } else {
            "SOLVER_REPLAYED"
        },
        model_soundness: precision,
    })
}
