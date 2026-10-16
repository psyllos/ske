//! Transactional persistence for scalar/global function summaries.
//! Unsupported composition is rejected, never serialized as an exact summary.
use super::{analysis, analysis_load, sql};
use prost::Message;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use ske_common::{sha256_hex, Error, Result};
use ske_solver::{Solver, Z3Solver};
use ske_symbolic::{
    engine::{Budget, Exit, Path, Soundness},
    memory::Loc,
    program::Program,
    summary::{FunctionSummary, Precision},
    BinaryOp, Constant, ExprArena, ExprId, ExprKind, Node, Sort, Type, UnaryOp,
};
use std::collections::BTreeMap;

fn engine_error(message: String) -> Error {
    Error::new("E_INTERNAL", message)
}

/// Legacy v0.4 serialization hash, also used by stale-check. Full §4.6
/// canonicalization independent of flow IDs remains to be audited for v0.5.
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
    computed: u32,
    reused: u32,
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
        // §7.1 fingerprint only needs callee fingerprints + structural/semantic/
        // context/config, all available before this function's engine run. This
        // lets v0.5 copy-on-reuse (§13) find an identical summary from any
        // snapshot of the same repository before spending solver time.
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
        if let Some((summary, summary_id)) = self.try_reuse(function, &context, &fingerprint)? {
            self.reused += 1;
            return Ok((summary, fingerprint, summary_id));
        }
        self.computed += 1;
        let summary =
            ske_summary::compute(&program, self.budget, &mut self.solver, &callees, &|| false)
                .map_err(engine_error)?;
        if !summary.covers_inputs {
            return Err(Error::new("E_USAGE",
                "only complete memory-free summaries can currently be persisted; memory effects and residual cases are not implemented"));
        }
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

    /// §13 v0.5 copy-on-reuse: if any snapshot of the same repository already
    /// produced a summary with this exact (context, summary) fingerprint, copy
    /// its rows into the current snapshot and reconstruct an in-memory summary.
    fn try_reuse(
        &mut self,
        function: i64,
        context: &str,
        fingerprint: &str,
    ) -> Result<Option<(FunctionSummary, i64)>> {
        // Prefer an original (reused_from_summary_id IS NULL) row in another
        // snapshot of the same repository; fall back to any other-snapshot match.
        let source: Option<(i64, i64)> = self
            .db
            .query_row(
                "SELECT fs.id, ar.snapshot_id FROM function_summaries fs
             JOIN analysis_runs ar ON ar.id=fs.produced_by_run_id
             JOIN snapshots s ON s.id=ar.snapshot_id
             JOIN repositories r ON r.id=s.repository_id
             WHERE fs.context_fingerprint=?1 AND fs.summary_fingerprint=?2 AND fs.complete=1
               AND ar.snapshot_id IN (
                 SELECT s2.id FROM snapshots s2
                 JOIN repositories r2 ON r2.id=s2.repository_id
                 WHERE r2.id=(
                   SELECT r3.id FROM repositories r3
                   JOIN snapshots s3 ON s3.repository_id=r3.id
                   JOIN analysis_runs ar3 ON ar3.snapshot_id=s3.id
                   WHERE ar3.id=?3))
               AND ar.snapshot_id<>?4
             ORDER BY CASE WHEN fs.reused_from_summary_id IS NULL THEN 0 ELSE 1 END,
                      ar.snapshot_id, fs.id LIMIT 1",
                params![context, fingerprint, self.run, self.snapshot],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(sql)?;
        let Some((source_id, source_snapshot)) = source else {
            return Ok(None);
        };
        let result = copy_summary_rows(
            self.db,
            self.snapshot,
            self.run,
            function,
            source_id,
            source_snapshot,
        )?;
        // Mirror compute()'s recursion bookkeeping before the early return.
        let recursion = self
            .active
            .get(&function)
            .copied()
            .unwrap_or(1)
            .saturating_sub(1);
        if recursion == 0 {
            self.active.remove(&function);
        } else {
            self.active.insert(function, recursion);
        }
        Ok(Some(result))
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
    let snapshot_config: String = tx
        .query_row(
            "SELECT config_fingerprint FROM snapshots WHERE id=?1",
            [snapshot],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let config = sha256_hex(format!(
        "{snapshot_config}|{budget_json}|ske-summary-scalar-0.5|{}|{}",
        solver.name(),
        solver.version()
    ));
    let context = sha256_hex("{}");
    tx.execute(
        "INSERT INTO analysis_runs(snapshot_id,entry_function_id,analysis_kind,engine_version,
         solver_name,solver_version,config_fingerprint,assumptions_fingerprint,budget_json,status)
         VALUES(?1,?2,'SUMMARY','ske-summary-scalar-0.5',?3,?4,?5,?6,?7,'RUNNING')",
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
        computed: 0,
        reused: 0,
    };
    let (summary, fingerprint, id) = calculation.compute(entry)?;
    let metrics = json!({
        "summaries_computed": calculation.computed,
        "summaries_reused": calculation.reused,
    });
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
         assumptions_fingerprint=?2,metrics_json=?5,finished_at=CURRENT_TIMESTAMP WHERE id=?1",
        params![
            run,
            sha256_hex(assumptions.join("\n")),
            if summary.complete {
                "COMPLETED"
            } else {
                "PARTIAL"
            },
            summary.complete,
            metrics.to_string()
        ],
    )
    .map_err(sql)?;
    tx.commit().map_err(sql)?;
    Ok(
        json!({"run_id":run,"summary_id":id,"summary_fingerprint":fingerprint,
        "precision":match summary.precision { Precision::Exact => "EXACT", Precision::Partial => "PARTIAL", _ => "OVERAPPROX" },
        "complete":summary.complete,"cases":summary.cases.len(),
        "metrics":metrics}),
    )
}

/// §13 v0.5 copy-on-reuse: copy a summary produced in `source_snapshot` into the
/// current snapshot, rewriting every reference (function, expressions by
/// structural hash, spans and IR outcomes by position). Returns the in-memory
/// (reconstructed) summary plus the new `function_summaries` id.
fn copy_summary_rows(
    db: &Connection,
    target_snapshot: i64,
    target_run: i64,
    target_function: i64,
    source_summary_id: i64,
    source_snapshot: i64,
) -> Result<(FunctionSummary, i64)> {
    // 1. Copy the function_summaries header row.
    let (function_id, context, fingerprint, precision, complete, unknowns): (
        i64,
        String,
        String,
        String,
        bool,
        String,
    ) = db
        .query_row(
            "SELECT function_id,context_fingerprint,summary_fingerprint,precision,complete,unknowns_json
             FROM function_summaries WHERE id=?1 AND function_id<>?2",
            params![source_summary_id, target_function],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .map_err(sql)?;
    if function_id == target_function {
        // Same function/snapshot: no reuse needed.
        let id = db
            .query_row(
                "SELECT id FROM function_summaries WHERE id=?1",
                [source_summary_id],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let summary = reconstruct_summary(db, source_summary_id, source_snapshot)?;
        return Ok((summary, id));
    }
    db.execute(
        "INSERT INTO function_summaries(function_id,produced_by_run_id,context_fingerprint,summary_fingerprint,precision,complete,unknowns_json,reused_from_summary_id)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(function_id,context_fingerprint,summary_fingerprint) DO NOTHING",
        params![target_function, target_run, context, fingerprint, precision, complete, unknowns, source_summary_id],
    )
    .map_err(sql)?;
    let target_summary_id = db.query_row(
        "SELECT id FROM function_summaries WHERE function_id=?1 AND context_fingerprint=?2 AND summary_fingerprint=?3",
        params![target_function, context, fingerprint],
        |r| r.get(0),
    )
    .map_err(sql)?;
    db.execute(
        "INSERT INTO analysis_assumptions(analysis_run_id,kind,description,payload)
         SELECT ?1,a.kind,a.description,a.payload FROM analysis_assumptions a
         JOIN function_summaries fs ON fs.produced_by_run_id=a.analysis_run_id
         WHERE fs.id=?2 AND NOT EXISTS (
           SELECT 1 FROM analysis_assumptions existing
           WHERE existing.analysis_run_id=?1 AND existing.kind=a.kind
             AND existing.payload IS a.payload)",
        params![target_run, source_summary_id],
    )
    .map_err(sql)?;

    // Idempotence: if the target row already carries cases, copy is not re-run
    // (e.g. when re-summarizing a snapshot that already reused). Reconstruct.
    let already: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM summary_cases WHERE summary_id=?1)",
            [target_summary_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if already {
        let summary = reconstruct_summary(db, target_summary_id, target_snapshot)?;
        return Ok((summary, target_summary_id));
    }

    // 2. Copy summary_cases + returns + outcomes + observable_effects.
    let cases = query_maps(
        db,
        "SELECT id FROM summary_cases WHERE summary_id=?1 ORDER BY case_ordinal",
        [source_summary_id],
    )?;
    for case in cases {
        let source_case = case["id"].as_i64().unwrap();
        let (guard_json, exit_kind, exec_precision, exit_span, mem_delta): (String, String, String, Option<i64>, String) =
            db.query_row(
                "SELECT guard_json,exit_kind,precision,exit_span_id,memory_delta_json FROM summary_cases WHERE id=?1",
                [source_case],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .map_err(sql)?;
        // Rewrite guard_json expression ids -> target.
        let guard_ids: Vec<i64> =
            serde_json::from_str(&guard_json).map_err(|e| engine_error(e.to_string()))?;
        let new_guard: Result<Vec<i64>> = guard_ids
            .iter()
            .map(|id| copy_expr(db, source_snapshot, target_snapshot, *id))
            .collect();
        let guard_json =
            serde_json::to_string(&new_guard?).map_err(|e| engine_error(e.to_string()))?;
        // Rewrite memory_delta_json expression ids.
        let mem_delta = rewrite_memory_delta(db, source_snapshot, target_snapshot, &mem_delta)?;
        // Rewrite exit span.
        let new_span = exit_span
            .map(|span| copy_span(db, source_snapshot, target_snapshot, span))
            .transpose()?;
        db.execute(
            "INSERT INTO summary_cases(summary_id,case_ordinal,guard_json,exit_kind,memory_delta_json,precision,exit_span_id)
             SELECT ?1,(SELECT COUNT(*) FROM summary_cases WHERE summary_id=?1),?2,?3,?4,?5,?6",
            params![target_summary_id, guard_json, exit_kind, mem_delta, exec_precision, new_span],
        )
        .map_err(sql)?;
        let target_case = db.last_insert_rowid();
        // returns.
        for source_expr in query_maps(db, "SELECT return_ordinal,expression_id FROM summary_case_returns WHERE summary_case_id=?1 ORDER BY return_ordinal", [source_case])? {
            let target_expr = copy_expr(db, source_snapshot, target_snapshot, source_expr["expression_id"].as_i64().unwrap())?;
            db.execute(
                "INSERT INTO summary_case_returns(summary_case_id,return_ordinal,expression_id) VALUES(?1,?2,?3)",
                params![target_case, source_expr["return_ordinal"].as_i64().unwrap(), target_expr],
            )
            .map_err(sql)?;
        }
        // outcomes (IR outcome id by control edge position).
        for source_ir in query_maps(db, "SELECT path_ordinal,ir_outcome_id FROM summary_case_outcomes WHERE summary_case_id=?1 ORDER BY path_ordinal", [source_case])? {
            let target_ir = copy_ir_outcome(db, source_snapshot, target_snapshot, source_ir["ir_outcome_id"].as_i64().unwrap())?;
            db.execute(
                "INSERT INTO summary_case_outcomes(summary_case_id,path_ordinal,ir_outcome_id) VALUES(?1,?2,?3)",
                params![target_case, source_ir["path_ordinal"].as_i64().unwrap(), target_ir],
            )
            .map_err(sql)?;
        }
        // observable_effects (no references to rewrite).
        for effect in query_maps(db, "SELECT effect_ordinal,effect_kind,target_descriptor,payload,semantic_model_fingerprint,precision FROM observable_effects WHERE summary_case_id=?1 ORDER BY effect_ordinal", [source_case])? {
            let to_null = |v: &Value| -> Option<String> {
                v.as_str().map(str::to_owned)
            };
            db.execute(
                "INSERT INTO observable_effects(summary_case_id,effect_ordinal,effect_kind,target_descriptor,payload,semantic_model_fingerprint,precision) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![target_case, effect["effect_ordinal"].as_i64().unwrap(), effect["effect_kind"].as_str().unwrap_or(""), to_null(&effect["target_descriptor"]), to_null(&effect["payload"]), to_null(&effect["semantic_model_fingerprint"]), effect["precision"].as_str().unwrap_or("")],
            )
            .map_err(sql)?;
        }
    }
    // 3. Reconstruct the in-memory summary from the copied (target) rows.
    let summary = reconstruct_summary(db, target_summary_id, target_snapshot)?;
    Ok((summary, target_summary_id))
}

fn query_maps(db: &Connection, query: &str, args: impl rusqlite::Params) -> Result<Vec<Value>> {
    let mut stmt = db.prepare(query).map_err(sql)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut cursor = stmt.query(args).map_err(sql)?;
    let mut out = Vec::new();
    while let Some(row) = cursor.next().map_err(sql)? {
        let mut object = serde_json::Map::new();
        for (n, name) in names.iter().enumerate() {
            let value = match row.get_ref(n).map_err(sql)? {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(n) => json!(n),
                rusqlite::types::ValueRef::Real(n) => json!(n),
                rusqlite::types::ValueRef::Text(s) => json!(String::from_utf8_lossy(s)),
                rusqlite::types::ValueRef::Blob(b) => json!(String::from_utf8_lossy(b)),
            };
            object.insert(name.clone(), value);
        }
        out.push(Value::Object(object));
    }
    Ok(out)
}

/// Copy one expression (dedup by hash) from `source` to `target` snapshot.
/// Recursively copies operands; returns the target expression id.
fn copy_expr(db: &Connection, source: i64, target: i64, source_expr: i64) -> Result<i64> {
    let (hash, kind, type_id, payload): (String, String, Option<i64>, Vec<u8>) = db
        .query_row(
            "SELECT expression_hash,kind,type_id,payload FROM symbolic_expressions WHERE id=?1 AND snapshot_id=?2",
            params![source_expr, source],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(sql)?;
    let existing: Option<i64> = db
        .query_row(
            "SELECT id FROM symbolic_expressions WHERE snapshot_id=?1 AND expression_hash=?2",
            params![target, hash],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    if let Some(id) = existing {
        return Ok(id);
    }
    // Remap the type to the target snapshot: cir_types share unit+local_key in
    // structurally identical snapshots, so the same local_key exists there.
    let target_type: Option<i64> = match type_id {
        None => None,
        Some(source_type) => {
            let local_key: String = db
                .query_row(
                    "SELECT local_key FROM cir_types WHERE id=?1",
                    [source_type],
                    |r| r.get(0),
                )
                .map_err(sql)?;
            db.query_row(
                "SELECT c2.id FROM cir_types c2 JOIN compilation_units u2 ON u2.id=c2.unit_id
                 WHERE c2.local_key=?1 AND u2.snapshot_id=?2",
                params![local_key, target],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?
        }
    };
    // Recursively copy operands.
    let operands: Vec<i64> = {
        let mut stmt = db
            .prepare("SELECT operand_expression_id FROM expression_operands WHERE expression_id=?1 AND expression_id IN (SELECT id FROM symbolic_expressions WHERE snapshot_id=?2) ORDER BY ordinal")
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![source_expr, source], |r| r.get::<_, i64>(0))
            .map_err(sql)?;
        let mut out = Vec::new();
        for row in rows {
            let pid = row.map_err(sql)?;
            out.push(copy_expr(db, source, target, pid)?);
        }
        out
    };
    db.execute(
        "INSERT INTO symbolic_expressions(snapshot_id,expression_hash,kind,type_id,payload) VALUES(?1,?2,?3,?4,?5) ON CONFLICT DO NOTHING",
        params![target, hash, kind, target_type, payload],
    )
    .map_err(sql)?;
    let new_id = db
        .query_row(
            "SELECT id FROM symbolic_expressions WHERE snapshot_id=?1 AND expression_hash=?2",
            params![target, hash],
            |r| r.get(0),
        )
        .map_err(sql)?;
    for (ordinal, child) in operands.iter().enumerate() {
        db.execute(
            "INSERT OR IGNORE INTO expression_operands(expression_id,ordinal,operand_expression_id) VALUES(?1,?2,?3)",
            params![new_id, ordinal as i64, child],
        )
        .map_err(sql)?;
    }
    Ok(new_id)
}

fn rewrite_memory_delta(
    db: &Connection,
    source: i64,
    target: i64,
    json_text: &str,
) -> Result<String> {
    let value: Value = serde_json::from_str(json_text).map_err(|e| engine_error(e.to_string()))?;
    let array = value.as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for item in array {
        let mut item = item;
        if let Some(expr) = item["expression_id"].as_i64() {
            item["expression_id"] = json!(copy_expr(db, source, target, expr)?);
        }
        out.push(item);
    }
    serde_json::to_string(&out).map_err(|e| engine_error(e.to_string()))
}

/// Copy a source span by (path, line, col) position.
fn copy_span(db: &Connection, source: i64, target: i64, span: i64) -> Result<i64> {
    let (path, sl, sc, el, ec): (String, i64, i64, i64, i64) = db
        .query_row(
            "SELECT f.path,s.start_line,s.start_column,s.end_line,s.end_column FROM source_spans s JOIN source_files f ON f.id=s.source_file_id WHERE s.id=?1 AND f.snapshot_id=?2",
            params![span, source],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .map_err(sql)?;
    let target_span: Option<i64> = db
        .query_row(
            "SELECT s.id FROM source_spans s JOIN source_files f ON f.id=s.source_file_id
             WHERE f.path=?1 AND f.snapshot_id=?2 AND s.start_line=?3 AND s.start_column=?4 AND s.end_line=?5 AND s.end_column=?6",
            params![path, target, sl, sc, el, ec],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    target_span.ok_or_else(|| Error::new("E_DB_INTEGRITY", "span not found in target snapshot"))
}

/// Copy an IR outcome reference by its control edge local key.
fn copy_ir_outcome(db: &Connection, source: i64, target: i64, ir_outcome: i64) -> Result<i64> {
    let edge_key: String = db
        .query_row(
            "SELECT e.local_key FROM ir_outcomes io
             JOIN control_edges e ON e.id=io.control_edge_id
             JOIN basic_blocks b ON b.id=e.source_block_id
             JOIN functions f ON f.id=b.function_id
             JOIN symbols s ON s.id=f.symbol_id
             WHERE io.id=?1 AND s.snapshot_id=?2",
            params![ir_outcome, source],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let target_id: Option<i64> = db
        .query_row(
            "SELECT io.id FROM ir_outcomes io
             JOIN control_edges e ON e.id=io.control_edge_id
             JOIN basic_blocks b ON b.id=e.source_block_id
             JOIN functions f ON f.id=b.function_id
             JOIN symbols s ON s.id=f.symbol_id
             WHERE e.local_key=?1 AND s.snapshot_id=?2",
            params![edge_key, target],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    target_id.ok_or_else(|| Error::new("E_DB_INTEGRITY", "IR outcome not found in target snapshot"))
}

/// Rebuild an in-memory `FunctionSummary` from persisted (copied) rows in
/// `snapshot`. This is the counterpart of the original engine computation: the
/// reconstructed cases feed composition exactly like freshly computed ones.
fn reconstruct_summary(db: &Connection, summary_id: i64, snapshot: i64) -> Result<FunctionSummary> {
    let (function, precision, complete, unknowns): (i64, String, bool, String) = db
        .query_row(
            "SELECT f.id, fs.precision, fs.complete, fs.unknowns_json
             FROM function_summaries fs JOIN functions f ON f.id=fs.function_id WHERE fs.id=?1",
            [summary_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(sql)?;
    let program = analysis_load::load(db, function)?;
    let mut arena = ExprArena::new();
    // Map persisted expression id -> arena id.
    let mut expr_map: BTreeMap<i64, ExprId> = BTreeMap::new();
    let mut cases = Vec::new();
    let source_cases = query_maps(db, "SELECT id,case_ordinal,guard_json,exit_kind,precision,exit_span_id,memory_delta_json FROM summary_cases WHERE summary_id=?1 ORDER BY case_ordinal", [summary_id])?;
    // counters for havoc numbering and per-case arena reuse.
    for case in &source_cases {
        let source_case = case["id"].as_i64().unwrap();
        let guard_ids: Vec<i64> = serde_json::from_str(case["guard_json"].as_str().unwrap_or("[]"))
            .map_err(|e| engine_error(e.to_string()))?;
        let mut constraints = Vec::new();
        for gid in &guard_ids {
            constraints.push(reconstruct_expr_in_arena(
                db,
                snapshot,
                *gid,
                &mut arena,
                &mut expr_map,
            )?);
        }
        let mut values = Vec::new();
        for row in query_maps(db, "SELECT return_ordinal,expression_id FROM summary_case_returns WHERE summary_case_id=?1 ORDER BY return_ordinal", [source_case])? {
            values.push(reconstruct_expr_in_arena(db, snapshot, row["expression_id"].as_i64().unwrap(), &mut arena, &mut expr_map)?);
        }
        let mut memory_delta = Vec::new();
        let deltas: Vec<Value> =
            serde_json::from_str(case["memory_delta_json"].as_str().unwrap_or("[]"))
                .map_err(|e| engine_error(e.to_string()))?;
        for delta in deltas {
            if let Some(expr) = delta["expression_id"].as_i64() {
                let loc = match delta["location"]["kind"].as_str() {
                    Some("GLOBAL") => Loc::Global(
                        delta["location"]["symbol"]
                            .as_str()
                            .unwrap_or("")
                            .to_owned(),
                    ),
                    _ => Loc::Unknown,
                };
                let id = reconstruct_expr_in_arena(db, snapshot, expr, &mut arena, &mut expr_map)?;
                memory_delta.push((loc, id));
            }
        }
        let exit_kind = case["exit_kind"].as_str().unwrap_or("RETURN").to_owned();
        let case_precision = case["precision"].as_str().unwrap_or("UNKNOWN");
        let soundness = match case_precision {
            "EXACT" => Soundness::Exact,
            _ => Soundness::Overapprox,
        };
        let partial = case_precision == "PARTIAL";
        let mut taken_outcomes = Path(None);
        for row in query_maps(db,
            "SELECT ir_outcome_id FROM summary_case_outcomes WHERE summary_case_id=?1 ORDER BY path_ordinal",
            [source_case])? {
            taken_outcomes = Path(Some(std::sync::Arc::new(ske_symbolic::engine::PathNode {
                previous: taken_outcomes,
                outcome: row["ir_outcome_id"].as_i64().unwrap().to_string(),
            })));
        }
        cases.push(Exit {
            kind: exit_kind,
            instruction_id: format!("summary_case_{source_case}"),
            constraints,
            soundness,
            taken_outcomes,
            values,
            partial,
            memory_delta,
        });
    }
    let unknowns: Vec<String> =
        serde_json::from_str(&unknowns).map_err(|e| engine_error(e.to_string()))?;
    let assumptions = query_maps(db, "SELECT kind FROM analysis_assumptions WHERE analysis_run_id IN (SELECT produced_by_run_id FROM function_summaries WHERE id=?1) ORDER BY kind", [summary_id])?
        .iter().map(|a| a["kind"].as_str().unwrap_or("").to_owned()).collect();
    let global_reads = arena
        .iter()
        .filter_map(|(_, node)| {
            let ExprKind::Symbol(symbol) = &node.kind else {
                return None;
            };
            let (_, rest) = symbol.split_once("Global(")?;
            let (quoted, _) = rest.split_once(')')?;
            let global = serde_json::from_str::<String>(quoted).ok()?;
            Some((symbol.clone(), global, node.ty.clone()))
        })
        .collect();
    let opaque_effects: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM observable_effects e JOIN summary_cases c ON c.id=e.summary_case_id
         WHERE c.summary_id=?1 AND e.effect_kind='UNKNOWN_EFFECT')", [summary_id], |r| r.get(0),
    ).map_err(sql)?;
    Ok(FunctionSummary {
        function_id: program.function.id.clone(),
        parameter_symbols: (0..program.function.parameter_value_ids.len())
            .map(|index| format!("param:{}:{index}", program.function.id))
            .collect(),
        arena,
        cases,
        precision: match precision.as_str() {
            "EXACT" => Precision::Exact,
            "OVERAPPROX" => Precision::Overapprox,
            "PARTIAL" => Precision::Partial,
            _ => Precision::Unknown,
        },
        complete,
        covers_inputs: complete,
        residual: false,
        assumptions,
        unknowns,
        composable: !opaque_effects,
        global_reads,
    })
}

/// Reconstruct one persisted expression (closure) into `arena`, returning the
/// arena-local root id. Operands are built in ascending persisted-id order.
fn reconstruct_expr_in_arena(
    db: &Connection,
    snapshot: i64,
    root: i64,
    arena: &mut ExprArena,
    expr_map: &mut BTreeMap<i64, ExprId>,
) -> Result<ExprId> {
    // Collect closure via BFS, then build in ascending id order.
    let mut closure: Vec<i64> = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if closure.contains(&id) || expr_map.contains_key(&id) {
            continue;
        }
        closure.push(id);
        let operands: Vec<i64> = {
            let mut stmt = db
                .prepare("SELECT operand_expression_id FROM expression_operands WHERE expression_id=?1 ORDER BY ordinal")
                .map_err(sql)?;
            let rows = stmt.query_map([id], |r| r.get::<_, i64>(0)).map_err(sql)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(sql)?);
            }
            out
        };
        for child in operands {
            stack.push(child);
        }
    }
    closure.sort_unstable();
    for &id in &closure {
        if expr_map.contains_key(&id) {
            continue;
        }
        let (kind, payload, type_id): (String, Vec<u8>, Option<i64>) = db
            .query_row(
                "SELECT kind,payload,type_id FROM symbolic_expressions WHERE id=?1 AND snapshot_id=?2",
                params![id, snapshot],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(sql)?;
        let raw = String::from_utf8_lossy(&payload);
        let payload_text = serde_json::from_str::<String>(&raw).unwrap_or_else(|_| raw.to_string());
        let ty = expression_type_from_db(db, type_id)?;
        let operands: Vec<ExprId> = {
            let mut stmt = db
                .prepare("SELECT operand_expression_id FROM expression_operands WHERE expression_id=?1 ORDER BY ordinal")
                .map_err(sql)?;
            let rows = stmt.query_map([id], |r| r.get::<_, i64>(0)).map_err(sql)?;
            let mut out = Vec::new();
            for row in rows {
                let pid = row.map_err(sql)?;
                out.push(
                    *expr_map
                        .get(&pid)
                        .ok_or_else(|| Error::new("E_DB_INTEGRITY", "operand not built"))?,
                );
            }
            out
        };
        let node = build_summary_node(&kind, &payload_text, &ty, &operands)?;
        let expr_id = arena
            .intern(node)
            .map_err(|e| Error::new("E_INTERNAL", format!("{e:?}")))?;
        expr_map.insert(id, expr_id);
    }
    expr_map
        .get(&root)
        .copied()
        .ok_or_else(|| Error::new("E_INTERNAL", "root not interned"))
}

/// Rebuild a `ske_symbolic::Type` from a persisted `type_id` (NULL -> engine
/// bool) or from the `cir_types` TypeDef protobuf.
fn expression_type_from_db(db: &Connection, type_id: Option<i64>) -> Result<Type> {
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
    let local_key = stored.rsplit('|').next().unwrap_or(&stored).to_owned();
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

/// Build one expression node from persisted kind/payload/type/operands.
fn build_summary_node(
    kind: &str,
    payload_text: &str,
    ty: &Type,
    operands: &[ExprId],
) -> Result<Node> {
    let kind = match kind {
        "CONST" => {
            let constant = match ty.sort {
                Sort::Bool => Constant::Bool(payload_text == "true"),
                _ if payload_text == "null" => Constant::Null,
                _ => Constant::Bits(payload_text.parse::<u128>().map_err(|e| {
                    Error::new("E_DB_INTEGRITY", format!("bad const payload: {e}"))
                })?),
            };
            ExprKind::Const(constant)
        }
        "SYMBOL" => ExprKind::Symbol(payload_text.to_owned()),
        "HAVOC" => ExprKind::Havoc(payload_text.parse::<u64>().unwrap_or(0)),
        "UNARY" => ExprKind::Unary {
            op: match payload_text {
                "Not" => UnaryOp::Not,
                "Neg" => UnaryOp::Neg,
                "BitNot" => UnaryOp::BitNot,
                "Cast" => UnaryOp::Cast,
                other => {
                    return Err(Error::new(
                        "E_DB_INTEGRITY",
                        format!("unknown unary {other}"),
                    ))
                }
            },
            arg: operands[0],
        },
        "BINARY" => ExprKind::Binary {
            op: match payload_text {
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
                        format!("unknown binary {other}"),
                    ))
                }
            },
            left: operands[0],
            right: operands[1],
        },
        "ITE" => ExprKind::Ite {
            condition: operands[0],
            then_expr: operands[1],
            else_expr: operands[2],
        },
        "FIELD" => ExprKind::Field {
            base: operands[0],
            index: payload_text
                .parse::<u32>()
                .map_err(|e| Error::new("E_DB_INTEGRITY", format!("invalid FIELD index: {e}")))?,
        },
        "UNINTERPRETED" => ExprKind::Uninterpreted {
            name: payload_text.to_owned(),
            args: operands.to_vec(),
        },
        other => {
            return Err(Error::new(
                "E_DB_INTEGRITY",
                format!("unknown kind {other}"),
            ))
        }
    };
    Ok(Node {
        ty: ty.clone(),
        kind,
    })
}

#[cfg(test)]
mod persisted_expression_tests {
    use super::*;

    #[test]
    fn reused_field_hydration_preserves_nonzero_index() {
        let ty = Type {
            local_key: "field-result".into(),
            sort: Sort::Uninterpreted,
        };
        let node = build_summary_node("FIELD", "3", &ty, &[ExprId(0)]).unwrap();
        assert!(matches!(node.kind, ExprKind::Field { index: 3, .. }));
        assert!(build_summary_node("FIELD", "not-an-index", &ty, &[ExprId(0)]).is_err());
    }
}
