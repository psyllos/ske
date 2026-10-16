//! SQLite-independent JSON DTO façade. Only this layer, never the CLI, queries SQL.
use rusqlite::{params, types::ValueRef, Connection, Params};
use serde_json::{json, Value};
use ske_common::{Error, Result};
pub use ske_ingest::{adapter::RunLimits, profile, GoOptions};
use ske_storage::Storage;
use std::path::{Path, PathBuf};

mod analysis;
pub mod analysis_config;
mod analysis_load;
mod analysis_read;
mod behavior;
mod callgraph;
mod summaries;
#[cfg(test)]
mod summary_tests;

pub struct Query {
    storage: Storage,
}
fn sql(e: rusqlite::Error) -> Error {
    Error::new("E_DB_INTEGRITY", e.to_string())
}
fn rows(db: &Connection, q: &str, p: impl Params) -> Result<Vec<Value>> {
    let mut stmt = db.prepare(q).map_err(sql)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut cursor = stmt.query(p).map_err(sql)?;
    let mut out = Vec::new();
    while let Some(row) = cursor.next().map_err(sql)? {
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
fn one(db: &Connection, q: &str, p: impl Params) -> Result<Value> {
    rows(db, q, p)?
        .into_iter()
        .next()
        .ok_or_else(|| Error::new("E_NOT_FOUND", "object not found"))
}
fn local(value: &Value) -> Value {
    value
        .as_str()
        .map(|s| json!(s.split_once('|').map(|(_, k)| k).unwrap_or(s)))
        .unwrap_or(Value::Null)
}
fn boolean(object: &mut Value, key: &str) {
    object[key] = json!(object[key].as_i64().unwrap_or(0) != 0);
}
fn page(limit: usize, offset: usize, total: usize) -> Value {
    json!({"limit":limit,"offset":offset,"total":total,"truncated":offset.saturating_add(limit)<total})
}
const SPAN_SQL:&str="SELECT sf.path||':'||ss.start_line||':'||ss.start_column||'-'||ss.end_line||':'||ss.end_column AS span FROM source_spans ss JOIN source_files sf ON sf.id=ss.source_file_id WHERE ss.id=?1";

/// Module-level span text for behavior/evidence rendering.
pub(crate) fn span_text(db: &Connection, span_id: i64) -> Result<Value> {
    one(db, SPAN_SQL, [span_id]).map(|v| v["span"].clone())
}

impl Query {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            storage: Storage::open(path)?,
        })
    }
    pub fn init(&self) -> Result<Value> {
        Ok(json!({"status":"ok","message":"already up to date"}))
    }
    pub fn ingest(&self, input: impl AsRef<Path>, source_root: Option<&Path>) -> Result<Value> {
        self.ingest_many(&[input.as_ref().to_path_buf()], source_root, None)
    }
    pub fn ingest_many(
        &self,
        inputs: &[PathBuf],
        source_root: Option<&Path>,
        repository: Option<&str>,
    ) -> Result<Value> {
        ske_ingest::ingest_many(&self.storage, inputs, source_root, repository)
    }
    pub fn db_check(&self) -> Result<Value> {
        self.storage.db_check()
    }
    /// Compute and persist scalar/global summaries in the default context.
    /// Residual recursion is PARTIAL; unknown effects remain explicit. A
    /// worklist cut fails rather than persisting incomplete coverage as exact.
    pub fn summarize_scalar(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        entry: &str,
        budget: &ske_symbolic::engine::Budget,
    ) -> Result<Value> {
        let sid = self.snapshot(snapshot, repository)?;
        let (function, _) = self.function(snapshot, repository, entry)?;
        summaries::run(self.db(), sid, function, budget)
    }
    pub fn ingest_go(&self, options: &ske_ingest::GoOptions<'_>) -> Result<Value> {
        ske_ingest::ingest_go(&self.storage, options)
    }
    pub fn analyze(
        &mut self,
        snapshot: &str,
        repository: Option<&str>,
        entry: &str,
        budget: &analysis_config::Budget,
        context: &analysis_config::Context,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Value> {
        let sid = self.snapshot(snapshot, repository)?;
        let (function, _) = self.function(snapshot, repository, entry)?;
        analysis::run(
            &mut self.storage.conn,
            sid,
            function,
            budget,
            context,
            cancel,
        )
    }
    pub fn branches_run(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        name: &str,
        selector: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Value> {
        let sid = self.snapshot(snapshot, repository)?;
        let (function, _) = self.function(snapshot, repository, name)?;
        let run = if selector == "latest" {
            one(self.db(),"SELECT id FROM analysis_runs WHERE snapshot_id=?1 AND entry_function_id=?2 AND analysis_kind='BRANCH_ANALYSIS' ORDER BY id DESC LIMIT 1",params![sid,function])?["id"].as_i64().unwrap()
        } else {
            selector
                .parse::<i64>()
                .map_err(|_| Error::new("E_USAGE", "run must be latest or an integer"))?
        };
        let mut dto = self.branches_page(snapshot, repository, name, limit, offset)?;
        analysis_read::apply(self.db(), run, function, &mut dto)?;
        Ok(dto)
    }
    pub fn runs(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Value> {
        let snapshot = self.snapshot(snapshot, repository)?;
        let runs = rows(self.db(), "SELECT id AS run_id,snapshot_id,analysis_kind,entry_function_id,status AS run_status,exploration_closed,cut_count,engine_version,solver_name,solver_version,context_name,started_at,finished_at,error_message FROM analysis_runs WHERE snapshot_id=?1 ORDER BY id DESC", [snapshot])?;
        let total = runs.len();
        let runs: Vec<_> = runs
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|mut value| {
                boolean(&mut value, "exploration_closed");
                value
            })
            .collect();
        Ok(json!({"snapshot_id":snapshot,"runs":runs,"page":page(limit,offset,total)}))
    }
    pub fn mark_run_failed(&self, run: i64) -> Result<Value> {
        let changed = self.db().execute("UPDATE analysis_runs SET status='FAILED',error_message='Manually marked failed',finished_at=CURRENT_TIMESTAMP WHERE id=?1 AND status IN ('PENDING','RUNNING')", [run]).map_err(sql)?;
        if changed == 0 {
            let _ = one(self.db(), "SELECT id FROM analysis_runs WHERE id=?1", [run])?;
            return Err(Error::new(
                "E_USAGE",
                "only PENDING or RUNNING runs can be marked failed",
            ));
        }
        Ok(json!({"run_id":run,"run_status":"FAILED"}))
    }
    pub fn callgraph(&self, snapshot: &str, repository: Option<&str>) -> Result<Value> {
        let sid = self.snapshot(snapshot, repository)?;
        callgraph::callgraph(self.db(), sid)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn paths_explore(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        source: &str,
        target: &str,
        max_depth: Option<usize>,
        max_paths: Option<usize>,
        max_nodes: Option<usize>,
        limit: usize,
        offset: usize,
    ) -> Result<Value> {
        let sid = self.snapshot(snapshot, repository)?;
        let (source_id, _) = self.function(snapshot, repository, source)?;
        callgraph::paths_explore(
            self.db(),
            sid,
            source,
            source_id,
            target,
            max_depth,
            max_paths,
            max_nodes,
            limit,
            offset,
        )
    }
    fn db(&self) -> &Connection {
        &self.storage.conn
    }
    fn snapshot(&self, selector: &str, repository: Option<&str>) -> Result<i64> {
        if selector != "latest" {
            let id = selector
                .parse::<i64>()
                .map_err(|_| Error::new("E_USAGE", "snapshot must be latest or an integer"))?;
            let value=one(self.db(),"SELECT s.id FROM snapshots s JOIN repositories r ON r.id=s.repository_id WHERE s.id=?1 AND (?2 IS NULL OR r.name=?2 OR CAST(r.id AS TEXT)=?2)",params![id,repository])?;
            return Ok(value["id"].as_i64().unwrap());
        }
        let repositories=rows(self.db(),"SELECT id,name FROM repositories WHERE (?1 IS NULL OR name=?1 OR CAST(id AS TEXT)=?1) ORDER BY name",[repository])?;
        if repositories.len() > 1 {
            let mut e = Error::new(
                "E_AMBIGUOUS",
                "latest requires --repository when multiple repositories exist",
            );
            e.candidates = repositories
                .iter()
                .map(|r| r["name"].as_str().unwrap().to_owned())
                .collect();
            return Err(e);
        }
        let repo = repositories
            .first()
            .ok_or_else(|| Error::new("E_NOT_FOUND", "repository not found"))?["id"]
            .as_i64()
            .unwrap();
        let value=one(self.db(),"SELECT id FROM snapshots WHERE repository_id=?1 AND status='CLOSED' ORDER BY id DESC LIMIT 1",[repo])?;
        Ok(value["id"].as_i64().unwrap())
    }
    fn function(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        name: &str,
    ) -> Result<(i64, Value)> {
        let snapshot = self.snapshot(snapshot, repository)?;
        let matches=rows(self.db(),"SELECT f.id,s.qualified_name AS function,u.unit_key,b.local_key AS entry FROM functions f JOIN symbols s ON s.id=f.symbol_id JOIN compilation_units u ON u.id=s.unit_id LEFT JOIN basic_blocks b ON b.id=f.entry_block_id WHERE s.snapshot_id=?1 AND s.qualified_name=?2 AND f.is_external=0 ORDER BY u.unit_key",params![snapshot,name])?;
        if matches.is_empty() {
            return Err(Error::new(
                "E_NOT_FOUND",
                format!("function {name} not found"),
            ));
        }
        if matches.len() > 1 {
            let mut e = Error::new("E_AMBIGUOUS", "function occurs in multiple units");
            e.candidates = matches
                .iter()
                .map(|r| format!("{}::{}", r["unit_key"].as_str().unwrap(), name))
                .collect();
            return Err(e);
        }
        let mut function = matches.into_iter().next().unwrap();
        let id = function["id"].as_i64().unwrap();
        function["entry"] = local(&function["entry"]);
        Ok((id, function))
    }
    pub fn snapshots(&self, repository: Option<&str>) -> Result<Value> {
        self.snapshots_page(repository, 100, 0)
    }
    pub fn snapshots_page(
        &self,
        repository: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Value> {
        let snapshots=rows(self.db(),"SELECT s.id,r.name AS repository,s.status,s.inventory_status,s.source_fingerprint,s.config_fingerprint,s.created_at,s.closed_at FROM snapshots s JOIN repositories r ON r.id=s.repository_id WHERE (?1 IS NULL OR r.name=?1 OR CAST(r.id AS TEXT)=?1) ORDER BY s.id DESC",[repository])?;
        let total = snapshots.len();
        let snapshots: Vec<_> = snapshots.into_iter().skip(offset).take(limit).collect();
        Ok(json!({"snapshots":snapshots,"page":page(limit,offset,total)}))
    }
    pub fn inspect(&self, snapshot: &str, repository: Option<&str>) -> Result<Value> {
        let id = self.snapshot(snapshot, repository)?;
        let snapshot=one(self.db(),"SELECT s.id,r.name AS repository,s.status,s.inventory_status,s.source_fingerprint,s.config_fingerprint,s.created_at,s.closed_at FROM snapshots s JOIN repositories r ON r.id=s.repository_id WHERE s.id=?1",[id])?;
        let mut units=rows(self.db(),"SELECT id,unit_key,language,semantic_profile,semantic_profile_hash,build_fingerprint,toolchain_version,adapter_name||'@'||adapter_version AS adapter,capability_flags,inventory_status FROM compilation_units WHERE snapshot_id=?1 ORDER BY unit_key",[id])?;
        for unit in &mut units {
            let uid = unit["id"].as_i64().unwrap();
            let counts=one(self.db(),"SELECT (SELECT COUNT(*) FROM functions f JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=?1 AND f.is_external=0) AS functions,(SELECT COUNT(*) FROM basic_blocks b JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=?1) AS blocks,(SELECT COUNT(*) FROM instructions i JOIN basic_blocks b ON b.id=i.block_id JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=?1) AS instructions,(SELECT COUNT(*) FROM source_decisions WHERE unit_id=?1) AS source_decisions,(SELECT COUNT(*) FROM source_outcomes o JOIN source_decisions d ON d.id=o.decision_id WHERE d.unit_id=?1) AS source_outcomes,(SELECT COUNT(*) FROM ir_decisions d JOIN instructions i ON i.id=d.terminator_instruction_id JOIN basic_blocks b ON b.id=i.block_id JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=?1) AS ir_decisions,(SELECT COUNT(*) FROM ir_outcomes o JOIN ir_decisions d ON d.id=o.decision_id JOIN instructions i ON i.id=d.terminator_instruction_id JOIN basic_blocks b ON b.id=i.block_id JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=?1) AS ir_outcomes,(SELECT COUNT(*) FROM diagnostics WHERE unit_id=?1) AS diagnostics",[uid])?;
            unit["counts"] = counts;
            unit["capability_flags"] =
                serde_json::from_str(unit["capability_flags"].as_str().unwrap())
                    .map_err(|e| Error::new("E_DB_INTEGRITY", e.to_string()))?;
            unit.as_object_mut().unwrap().remove("id");
        }
        Ok(
            json!({"snapshot":snapshot,"units":units,"analysis_defaults":{"max_wall_time_ms":60000,"max_states":10000,"max_instructions":1000000,"max_call_depth":8,"loop_unroll":3,"loop_mode":"unroll","max_recursion_unfold":2,"max_solver_queries":5000,"solver_rlimit":5000000,"solver_wall_timeout_ms":10000,"solver_seed":0,"max_memory_mb":1024},"diagnostics_summary":ske_ingest::diagnostics(self.db(),id)?}),
        )
    }
    pub fn functions(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        include_external: bool,
        limit: usize,
        offset: usize,
    ) -> Result<Value> {
        let id = self.snapshot(snapshot, repository)?;
        let mut functions=rows(self.db(),"SELECT s.qualified_name,u.unit_key,f.is_external,f.inventory_status,(SELECT COUNT(*) FROM basic_blocks WHERE function_id=f.id) AS blocks,(SELECT COUNT(*) FROM source_decisions WHERE function_id=f.id) AS source_decisions,(SELECT COUNT(*) FROM ir_outcomes o JOIN ir_decisions d ON d.id=o.decision_id JOIN instructions i ON i.id=d.terminator_instruction_id JOIN basic_blocks b ON b.id=i.block_id WHERE b.function_id=f.id) AS ir_outcomes FROM functions f JOIN symbols s ON s.id=f.symbol_id JOIN compilation_units u ON u.id=s.unit_id WHERE s.snapshot_id=?1 AND (?2 OR f.is_external=0) ORDER BY s.qualified_name,u.unit_key",params![id,include_external])?;
        let total = functions.len();
        functions = functions.into_iter().skip(offset).take(limit).collect();
        for f in &mut functions {
            boolean(f, "is_external");
        }
        Ok(json!({"snapshot_id":id,"functions":functions,"page":page(limit,offset,total)}))
    }
    fn span(&self, id: &Value) -> Result<Value> {
        if id.is_null() {
            Ok(Value::Null)
        } else {
            Ok(one(self.db(), SPAN_SQL, [id.as_i64().unwrap()])?["span"].clone())
        }
    }
    pub fn cfg(&self, snapshot: &str, repository: Option<&str>, name: &str) -> Result<Value> {
        let (id, mut function) = self.function(snapshot, repository, name)?;
        let mut blocks = rows(
            self.db(),
            "SELECT id,local_key,ordinal FROM basic_blocks WHERE function_id=?1 ORDER BY ordinal",
            [id],
        )?;
        for b in &mut blocks {
            let bid = b["id"].as_i64().unwrap();
            let mut instructions=rows(self.db(),"SELECT id,local_key,ordinal,opcode,(SELECT local_key FROM values_ir WHERE id=i.result_value_id) AS result,primary_span_id AS span FROM instructions i WHERE block_id=?1 ORDER BY ordinal",[bid])?;
            for i in &mut instructions {
                let iid = i["id"].as_i64().unwrap();
                let operands=rows(self.db(),"SELECT v.local_key FROM instruction_operands o JOIN values_ir v ON v.id=o.value_id WHERE instruction_id=?1 ORDER BY operand_index",[iid])?;
                i["operands"] = json!(operands
                    .iter()
                    .map(|o| local(&o["local_key"]))
                    .collect::<Vec<_>>());
                i["result"] = local(&i["result"]);
                i["span"] = self.span(&i["span"])?;
                i["id"] = local(&i["local_key"]);
                i.as_object_mut().unwrap().remove("local_key");
            }
            let mut successors=rows(self.db(),"SELECT e.local_key AS edge,t.local_key AS target,e.edge_kind AS kind,e.label,v.local_key AS guard_value,e.guard_negated FROM control_edges e JOIN basic_blocks t ON t.id=e.target_block_id LEFT JOIN values_ir v ON v.id=e.guard_value_id WHERE e.source_block_id=?1 ORDER BY e.ordinal",[bid])?;
            for s in &mut successors {
                s["edge"] = local(&s["edge"]);
                s["target"] = local(&s["target"]);
                s["guard"] = if s["guard_value"].is_null() {
                    Value::Null
                } else {
                    json!({"value":local(&s["guard_value"]),"negated":s["guard_negated"].as_i64()==Some(1)})
                };
                s.as_object_mut().unwrap().remove("guard_value");
                s.as_object_mut().unwrap().remove("guard_negated");
            }
            b["id"] = local(&b["local_key"]);
            b.as_object_mut().unwrap().remove("local_key");
            b["instructions"] = json!(instructions);
            b["successors"] = json!(successors);
        }
        function.as_object_mut().unwrap().remove("id");
        function["blocks"] = json!(blocks);
        Ok(function)
    }
    pub fn branches(&self, snapshot: &str, repository: Option<&str>, name: &str) -> Result<Value> {
        self.branches_page(snapshot, repository, name, 100, 0)
    }
    pub fn branches_page(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        name: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Value> {
        let sid = self.snapshot(snapshot, repository)?;
        let (id, function) = self.function(snapshot, repository, name)?;
        let mut decisions=rows(self.db(),"SELECT id,local_key,kind,source_span_id AS span FROM source_decisions WHERE function_id=?1 ORDER BY local_key",[id])?;
        for d in &mut decisions {
            let did = d["id"].as_i64().unwrap();
            let mut outcomes=rows(self.db(),"SELECT id,local_key,label,elided FROM source_outcomes WHERE decision_id=?1 ORDER BY ordinal",[did])?;
            for o in &mut outcomes {
                let oid = o["id"].as_i64().unwrap();
                let mut links=rows(self.db(),"SELECT e.local_key AS ir_outcome,l.mapping_kind,l.quality,l.inverted FROM source_ir_links l JOIN ir_outcomes o ON o.id=l.ir_outcome_id JOIN control_edges e ON e.id=o.control_edge_id WHERE l.source_outcome_id=?1 ORDER BY e.local_key",[oid])?;
                for link in &mut links {
                    link["ir_outcome"] = local(&link["ir_outcome"]);
                    boolean(link, "inverted");
                }
                o["id"] = local(&o["local_key"]);
                o.as_object_mut().unwrap().remove("local_key");
                boolean(o, "elided");
                o["status"] = json!("UNANALYZED");
                o["links"] = json!(links);
            }
            d["id"] = local(&d["local_key"]);
            d.as_object_mut().unwrap().remove("local_key");
            d["span"] = self.span(&d["span"])?;
            d["outcomes"] = json!(outcomes);
        }
        let ir=rows(self.db(),"SELECT e.local_key AS id,o.label,e.edge_kind,'UNANALYZED' AS status,NULL AS explanation FROM ir_outcomes o JOIN control_edges e ON e.id=o.control_edge_id JOIN basic_blocks b ON b.id=e.source_block_id JOIN instructions i ON i.id=e.terminator_instruction_id WHERE b.function_id=?1 ORDER BY b.ordinal,i.ordinal,o.ordinal",[id])?;
        let mut ir = ir;
        for o in &mut ir {
            o["id"] = local(&o["id"]);
            o["evidence"] = json!([]);
        }
        let inventory=one(self.db(),"SELECT f.inventory_status AS source_status,f.inventory_status AS ir_status,(SELECT COUNT(*) FROM source_decisions WHERE function_id=f.id) AS source_decisions,(SELECT COUNT(*) FROM source_outcomes o JOIN source_decisions d ON d.id=o.decision_id WHERE d.function_id=f.id) AS source_outcomes,(SELECT COUNT(*) FROM ir_decisions d JOIN instructions i ON i.id=d.terminator_instruction_id JOIN basic_blocks b ON b.id=i.block_id WHERE b.function_id=f.id) AS ir_decisions,(SELECT COUNT(*) FROM ir_outcomes o JOIN ir_decisions d ON d.id=o.decision_id JOIN instructions i ON i.id=d.terminator_instruction_id JOIN basic_blocks b ON b.id=i.block_id WHERE b.function_id=f.id) AS ir_outcomes FROM functions f WHERE f.id=?1",[id])?;
        let total = ir.len();
        let ir: Vec<_> = ir.into_iter().skip(offset).take(limit).collect();
        Ok(
            json!({"snapshot_id":sid,"function":function["function"],"unit_key":function["unit_key"],"inventory":inventory,"analysis":null,"source_decisions":decisions,"ir_outcomes":ir,"page":page(limit,offset,total)}),
        )
    }
    /// Reconstruct behaviors + rules + evidence for an entry function into the
    /// summary's snapshot (v0.5). Returns the JSON DTO.
    pub fn reconstruct_behaviors(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        entry: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Value> {
        let sid = self.snapshot(snapshot, repository)?;
        let (function, _) = self.function(snapshot, repository, entry)?;
        behavior::reconstruct(self.db(), sid, function, limit, offset)
    }
    /// List functional rules for a snapshot, optionally with evidence (v0.5).
    pub fn list_rules(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        with_evidence: bool,
        limit: usize,
        offset: usize,
    ) -> Result<Value> {
        let sid = self.snapshot(snapshot, repository)?;
        behavior::list_rules(self.db(), sid, with_evidence, limit, offset)
    }
    /// Mark rules from snapshot A stale when their function changed or vanished
    /// in snapshot B of the same repository (v0.5, §13).
    pub fn rules_stale_check(&self, repository: &str, from: &str, to: &str) -> Result<Value> {
        behavior::stale_check(self.db(), repository, from, to)
    }
}
