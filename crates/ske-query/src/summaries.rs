//! Transactional persistence for scalar/global function summaries.
//! Unsupported composition is rejected, never serialized as an exact summary.
use super::{analysis, analysis_load, sql};
use prost::Message;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use ske_common::{sha256_hex, Error, Result};
use ske_solver::{Solver, Z3Solver};
use ske_symbolic::{
    engine::Budget,
    program::Program,
    summary::{FunctionSummary, Precision},
};
use std::collections::BTreeMap;

fn engine_error(message: String) -> Error {
    Error::new("E_INTERNAL", message)
}

/// §4.6 structural fingerprint: sha256 of the canonical serialization of the
/// function's body (no flow ids, no spans). Shared with stale-check.
pub(crate) fn structural(program: &Program) -> String {
    // Length-delimited protobuf records avoid ambiguous concatenation. Map
    // iteration is ordered, independent of SQLite IDs and extraction order.
    let mut bytes = Vec::new();
    program
        .function
        .encode_length_delimited(&mut bytes)
        .expect("Vec write");
    for ty in program.types.values() {
        ty.encode_length_delimited(&mut bytes).expect("Vec write");
    }
    for value in program.values.values() {
        value
            .encode_length_delimited(&mut bytes)
            .expect("Vec write");
    }
    for block in program.blocks.values() {
        for ins in &block.instructions {
            ins.encode_length_delimited(&mut bytes).expect("Vec write");
        }
        for edge in &block.edges {
            edge.definition
                .encode_length_delimited(&mut bytes)
                .expect("Vec write");
        }
    }
    sha256_hex(bytes)
}
struct Calculation<'a> {
    db: &'a Connection,
    snapshot: i64,
    run: i64,
    budget: &'a Budget,
    config: &'a str,
    active: BTreeMap<i64, u32>,
    solver: Z3Solver,
}

impl Calculation<'_> {
    fn compute(&mut self, function: i64) -> Result<(FunctionSummary, String, i64)> {
        let program = analysis_load::load(self.db, function)?;
        let recursion = self.active.get(&function).copied().unwrap_or(0);
        if recursion > self.budget.max_recursion_unfold {
            // A scalar residual cannot omit memory effects. Until deltas are
            // supported, reject any activation with explicit effectful ops.
            if program
                .blocks
                .values()
                .flat_map(|b| &b.instructions)
                .any(|i| {
                    matches!(
                        i.opcode.as_str(),
                        "STORE"
                            | "LOAD"
                            | "ALLOC"
                            | "NEW_OBJECT"
                            | "OPAQUE_OP"
                            | "UNKNOWN_EFFECT"
                            | "INVOKE"
                            | "SPAWN"
                            | "RUN_DEFERS"
                            | "DEFER_PUSH"
                            | "EMIT_EFFECT"
                            | "CHANNEL_SEND"
                            | "CHANNEL_RECV"
                    )
                })
            {
                return Err(Error::new(
                    "E_USAGE",
                    "recursive memory effects require summary deltas",
                ));
            }
            let residual = FunctionSummary::recursion_residual(&program).map_err(engine_error)?;
            let key = sha256_hex(format!(
                "{}|RECURSION_BOUND|{}",
                structural(&program),
                self.config
            ));
            return Ok((residual, key, 0));
        }
        if self.active.values().copied().sum::<u32>() >= self.budget.max_call_depth {
            return Err(Error::new("E_USAGE", "summary call depth budget exhausted"));
        }
        self.active.insert(function, recursion + 1);
        let prefix: String = self
            .db
            .query_row(
                "SELECT u.unit_key FROM functions f JOIN symbols s ON s.id=f.symbol_id
             JOIN compilation_units u ON u.id=s.unit_id WHERE f.id=?1",
                [function],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let prefix = format!("{prefix}|");
        let targets = {
            let mut stmt = self
                .db
                .prepare(
                    "SELECT i.local_key, ct.target_function_id FROM call_sites cs
                 JOIN instructions i ON i.id=cs.instruction_id
                 JOIN basic_blocks b ON b.id=i.block_id
                 JOIN call_targets ct ON ct.call_site_id=cs.id
                 JOIN functions target ON target.id=ct.target_function_id
                 WHERE b.function_id=?1 AND cs.dispatch_kind='STATIC'
                 AND cs.world_closed=1 AND cs.resolution_status='RESOLVED_UNIQUE'
                 AND target.is_external=0 ORDER BY i.local_key",
                )
                .map_err(sql)?;
            let rows = stmt
                .query_map([function], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                })
                .map_err(sql)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(sql)?;
            rows
        };
        let mut callees = BTreeMap::new();
        let mut fingerprints = Vec::new();
        for (site, target) in targets {
            let (summary, fingerprint, _) = self.compute(target)?;
            fingerprints.push(fingerprint);
            let site = site
                .strip_prefix(&prefix)
                .ok_or_else(|| Error::new("E_DB_INTEGRITY", "callsite outside function unit"))?;
            callees.insert(site.to_owned(), summary);
        }
        let summary =
            ske_summary::compute(&program, self.budget, &mut self.solver, &callees, &|| false)
                .map_err(engine_error)?;
        if !summary.covers_inputs {
            return Err(Error::new("E_USAGE",
                "only complete memory-free summaries can currently be persisted; memory effects and residual cases are not implemented"));
        }
        let semantic: String = self
            .db
            .query_row(
                "SELECT u.semantic_profile_hash FROM functions f JOIN symbols s ON s.id=f.symbol_id
             JOIN compilation_units u ON u.id=s.unit_id WHERE f.id=?1",
                [function],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let context = sha256_hex("{}");
        let fingerprint = ske_summary::CacheKey {
            structural: &structural(&program),
            semantic_profile: &semantic,
            context: &context,
            callees: fingerprints,
            models: vec![],
            config: self.config,
        }
        .fingerprint()?;
        let precision = match summary.precision {
            Precision::Exact => "EXACT",
            Precision::Overapprox => "OVERAPPROX",
            Precision::Partial => "PARTIAL",
            Precision::Unknown => "UNKNOWN",
        };
        self.db
            .execute(
                "INSERT INTO function_summaries(function_id,produced_by_run_id,context_fingerprint,
             summary_fingerprint,precision,complete,unknowns_json) VALUES(?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(function_id,context_fingerprint,summary_fingerprint) DO NOTHING",
                params![
                    function,
                    self.run,
                    context,
                    fingerprint,
                    precision,
                    summary.complete,
                    serde_json::to_string(&summary.unknowns)
                        .map_err(|e| engine_error(e.to_string()))?
                ],
            )
            .map_err(sql)?;
        let summary_id: i64 = self.db.query_row(
            "SELECT id FROM function_summaries WHERE function_id=?1 AND context_fingerprint=?2 AND summary_fingerprint=?3",
            params![function,context,fingerprint], |r| r.get(0)).map_err(sql)?;
        let exists: bool = self
            .db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM summary_cases WHERE summary_id=?1)",
                [summary_id],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if !exists {
            let mut expression_ids = BTreeMap::new();
            for (ordinal, case) in summary.cases.iter().enumerate() {
                let mut guards = Vec::new();
                for id in &case.constraints {
                    guards.push(
                        analysis::persist_expr(
                            self.db,
                            self.snapshot,
                            function,
                            &summary.arena,
                            *id,
                            &mut expression_ids,
                        )?
                        .0,
                    );
                }
                let mut delta = Vec::new();
                for (location, expression) in &case.memory_delta {
                    let expression_id = analysis::persist_expr(
                        self.db,
                        self.snapshot,
                        function,
                        &summary.arena,
                        *expression,
                        &mut expression_ids,
                    )?
                    .0;
                    let location = match location {
                        ske_symbolic::memory::Loc::Global(global) => {
                            json!({"kind":"GLOBAL","symbol":global})
                        }
                        _ => json!({"kind":"UNKNOWN"}),
                    };
                    delta.push(json!({"location":location,"expression_id":expression_id}));
                }
                let span: Option<i64> = self.db.query_row(
                    "SELECT i.primary_span_id FROM instructions i JOIN basic_blocks b ON b.id=i.block_id
                     WHERE b.function_id=?1 AND i.local_key=?2",
                    params![function,format!("{prefix}{}", case.instruction_id)], |r| r.get(0)).map_err(sql)?;
                self.db.execute(
                    "INSERT INTO summary_cases(summary_id,case_ordinal,guard_json,exit_kind,precision,exit_span_id,memory_delta_json)
                     VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![summary_id,ordinal as i64,serde_json::to_string(&guards).unwrap(),
                        case.kind,if case.partial { "PARTIAL" } else if summary.composable && case.soundness == ske_symbolic::engine::Soundness::Exact {
                            "EXACT"
                        } else { "OVERAPPROX" },span,serde_json::to_string(&delta).unwrap()]).map_err(sql)?;
                let case_id = self.db.last_insert_rowid();
                if !summary.composable {
                    self.db.execute(
                        "INSERT INTO observable_effects(summary_case_id,effect_ordinal,effect_kind,precision,payload)
                         VALUES(?1,0,'UNKNOWN_EFFECT','OVERAPPROX','memory effects not modeled by scalar summaries')",
                        [case_id]).map_err(sql)?;
                }
                for (index, expression) in case.values.iter().enumerate() {
                    let id = analysis::persist_expr(
                        self.db,
                        self.snapshot,
                        function,
                        &summary.arena,
                        *expression,
                        &mut expression_ids,
                    )?
                    .0;
                    self.db
                        .execute(
                            "INSERT INTO summary_case_returns VALUES(?1,?2,?3)",
                            params![case_id, index as i64, id],
                        )
                        .map_err(sql)?;
                }
                for (index, outcome) in case.taken_outcomes.ordered().iter().enumerate() {
                    let outcome = outcome
                        .parse::<i64>()
                        .map_err(|e| engine_error(e.to_string()))?;
                    self.db
                        .execute(
                            "INSERT INTO summary_case_outcomes VALUES(?1,?2,?3)",
                            params![case_id, index as i64, outcome],
                        )
                        .map_err(sql)?;
                }
            }
        }
        for assumption in &summary.assumptions {
            self.db
                .execute(
                    "INSERT INTO analysis_assumptions(analysis_run_id,kind,description)
                 SELECT ?1,?2,?2 WHERE NOT EXISTS(
                 SELECT 1 FROM analysis_assumptions WHERE analysis_run_id=?1 AND kind=?2)",
                    params![self.run, assumption],
                )
                .map_err(sql)?;
        }
        if recursion == 0 {
            self.active.remove(&function);
        } else {
            self.active.insert(function, recursion);
        }
        Ok((summary, fingerprint, summary_id))
    }
}

pub fn run(db: &Connection, snapshot: i64, entry: i64, budget: &Budget) -> Result<Value> {
    let tx = db.unchecked_transaction().map_err(sql)?;
    let solver = Z3Solver::new();
    let budget_json = ske_common::canonical_json(&json!({
        "max_wall_time_ms":budget.max_wall_time_ms,"max_states":budget.max_states,
        "max_instructions":budget.max_instructions,"max_solver_queries":budget.max_solver_queries,
        "max_memory_mb":budget.max_memory_mb,"loop_unroll":budget.loop_unroll,
        "max_call_depth":budget.max_call_depth,"loop_mode":budget.loop_mode,
        "max_recursion_unfold":budget.max_recursion_unfold,"solver_rlimit":budget.solver_rlimit,
        "solver_wall_timeout_ms":budget.solver_wall_timeout_ms,"solver_seed":budget.solver_seed
    }))?;
    let config = sha256_hex(format!(
        "{budget_json}|ske-summary-scalar-0.4|{}|{}",
        solver.name(),
        solver.version()
    ));
    let context = sha256_hex("{}");
    tx.execute(
        "INSERT INTO analysis_runs(snapshot_id,entry_function_id,analysis_kind,engine_version,
         solver_name,solver_version,config_fingerprint,assumptions_fingerprint,budget_json,status)
         VALUES(?1,?2,'SUMMARY','ske-summary-scalar-0.4',?3,?4,?5,?6,?7,'RUNNING')",
        params![
            snapshot,
            entry,
            solver.name(),
            solver.version(),
            config,
            context,
            budget_json
        ],
    )
    .map_err(sql)?;
    let run = tx.last_insert_rowid();
    let mut calculation = Calculation {
        db: &tx,
        snapshot,
        run,
        budget,
        config: &config,
        active: BTreeMap::new(),
        solver,
    };
    let (summary, fingerprint, id) = calculation.compute(entry)?;
    let assumptions = {
        let mut stmt = tx.prepare("SELECT DISTINCT kind FROM analysis_assumptions WHERE analysis_run_id=?1 ORDER BY kind").map_err(sql)?;
        let values = stmt
            .query_map([run], |r| r.get::<_, String>(0))
            .map_err(sql)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql)?;
        values
    };
    tx.execute(
        "UPDATE analysis_runs SET status=?3,exploration_closed=?4,
         assumptions_fingerprint=?2,finished_at=CURRENT_TIMESTAMP WHERE id=?1",
        params![
            run,
            sha256_hex(assumptions.join("\n")),
            if summary.complete {
                "COMPLETED"
            } else {
                "PARTIAL"
            },
            summary.complete
        ],
    )
    .map_err(sql)?;
    tx.commit().map_err(sql)?;
    Ok(
        json!({"run_id":run,"summary_id":id,"summary_fingerprint":fingerprint,
        "precision":match summary.precision { Precision::Exact => "EXACT", Precision::Partial => "PARTIAL", _ => "OVERAPPROX" },
        "complete":summary.complete,"cases":summary.cases.len()}),
    )
}
