//! Transactional, bounded-record CIR importer.
pub mod adapter;
mod insert;
mod language;
pub mod profile;
mod validate;
pub use language::{
    ingest_go, ingest_java, ingest_language, ingest_rust, GoOptions, IngestOptions, Language,
};

use prost::Message;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use ske_cir::proto::{record::Content, *};
use ske_common::{sha256_hex, Error, Result};
use ske_storage::Storage;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn err(code: &str, message: impl Into<String>) -> Error {
    Error::new(code, message.into())
}
pub(crate) fn sql(e: rusqlite::Error) -> Error {
    let message = e.to_string();
    let code = if message.contains("instructions.block_id") && !message.contains("ordinal") {
        "E_MULTI_TERMINATOR"
    } else if message.contains("ordinal") {
        "E_ORDINAL_DUPLICATE"
    } else if message.contains("FOREIGN KEY") {
        "E_DANGLING_REF"
    } else if message.contains("param_index") {
        "E_PARAM_ORDER"
    } else {
        "E_DB_INTEGRITY"
    };
    err(code, message)
}

/// Reads exactly one u32-BE framed protobuf record, never the entire input.
pub fn read_record(reader: &mut impl Read, max: usize) -> Result<Option<Record>> {
    let mut length = [0; 4];
    match reader.read(&mut length[..1]) {
        Ok(0) => return Ok(None),
        Ok(_) => (),
        Err(e) => return Err(err("E_PROTO_DECODE", e.to_string())),
    }
    reader
        .read_exact(&mut length[1..])
        .map_err(|e| err("E_PROTO_DECODE", e.to_string()))?;
    let length = u32::from_be_bytes(length) as usize;
    if length > max {
        return Err(err("E_MSG_TOO_LARGE", "CIR record exceeds message limit"));
    }
    if length == 0 {
        return Err(err("E_PROTO_DECODE", "Empty CIR record"));
    }
    let mut bytes = vec![0; length];
    reader
        .read_exact(&mut bytes)
        .map_err(|e| err("E_PROTO_DECODE", e.to_string()))?;
    Record::decode(bytes.as_slice())
        .map(Some)
        .map_err(|e| err("E_PROTO_DECODE", e.to_string()))
}

/// IDs are reserved on first reference, and marked declared before insertion.
/// Structural rows are inserted as records arrive under deferred foreign keys.
/// TEMP protobuf bodies support later per-function validation, not insertion.
struct Identity {
    id: i64,
    declared: bool,
}
struct Space {
    entries: HashMap<String, Identity>,
    next: i64,
}
pub(crate) struct Unit<'a> {
    db: &'a Connection,
    snapshot: i64,
    id: i64,
    key: String,
    maps: RefCell<HashMap<&'static str, Space>>,
    sealed: Cell<bool>,
}
const SPACES: &[(&str, &str)] = &[
    ("type", "cir_types"),
    ("symbol", "symbols"),
    ("value", "values_ir"),
    ("function", "functions"),
    ("block", "basic_blocks"),
    ("instruction", "instructions"),
    ("edge", "control_edges"),
    ("decision", "source_decisions"),
    ("outcome", "source_outcomes"),
    ("exit", "implicit_exit_sites"),
    ("file", "source_files"),
];
impl<'a> Unit<'a> {
    fn new(db: &'a Connection, snapshot: i64, id: i64, key: String) -> Result<Self> {
        let mut maps = HashMap::new();
        for &(space, table) in SPACES {
            let next = db
                .query_row(
                    &format!("SELECT COALESCE(MAX(id),0)+1 FROM {table}"),
                    [],
                    |r| r.get(0),
                )
                .map_err(sql)?;
            maps.insert(
                space,
                Space {
                    entries: HashMap::new(),
                    next,
                },
            );
        }
        Ok(Self {
            db,
            snapshot,
            id,
            key,
            maps: RefCell::new(maps),
            sealed: Cell::new(false),
        })
    }
    fn declare(&self, space: &'static str, key: &str) -> Result<i64> {
        if key.is_empty() {
            return Err(err("E_EMPTY_ID", format!("empty {space} id")));
        }
        let id = self.get(space, key)?;
        let mut maps = self.maps.borrow_mut();
        let identity = maps.get_mut(space).unwrap().entries.get_mut(key).unwrap();
        if identity.declared {
            return Err(err("E_DUPLICATE_ID", format!("{space}: {key}")));
        }
        identity.declared = true;
        Ok(id)
    }
    pub(crate) fn get(&self, space: &str, key: &str) -> Result<i64> {
        if key.is_empty() {
            return Err(err("E_DANGLING_REF", format!("empty {space} reference")));
        }
        let mut maps = self.maps.borrow_mut();
        let map = maps.get_mut(space).expect("known ID space");
        if let Some(identity) = map.entries.get(key) {
            if !self.sealed.get() || identity.declared {
                return Ok(identity.id);
            }
        }
        if self.sealed.get() {
            return Err(err("E_DANGLING_REF", format!("{space}: {key}")));
        }
        // Source files are shared between units. Reserve their existing identity
        // before a forward span references them, rather than allocating a new row.
        let existing = if space == "file" {
            self.db
                .query_row(
                    "SELECT id FROM source_files WHERE snapshot_id=?1 AND path=?2",
                    params![self.snapshot, key],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql)?
        } else {
            None
        };
        let id = existing.unwrap_or_else(|| {
            let id = map.next;
            map.next += 1;
            id
        });
        map.entries.insert(
            key.to_owned(),
            Identity {
                id,
                declared: false,
            },
        );
        Ok(id)
    }
    fn optional(&self, space: &str, key: &str) -> Result<Option<i64>> {
        if key.is_empty() {
            Ok(None)
        } else {
            self.get(space, key).map(Some)
        }
    }
    fn local(&self, key: &str) -> String {
        format!("{}|{key}", self.key)
    }
    fn records(&self, mut f: impl FnMut(Content) -> Result<()>) -> Result<()> {
        let mut stmt = self
            .db
            .prepare("SELECT body FROM temp.ske_records ORDER BY ordinal")
            .map_err(sql)?;
        let mut rows = stmt.query([]).map_err(sql)?;
        while let Some(row) = rows.next().map_err(sql)? {
            let bytes: Vec<u8> = row.get(0).map_err(sql)?;
            let record = Record::decode(bytes.as_slice())
                .map_err(|e| err("E_PROTO_DECODE", e.to_string()))?;
            f(record
                .content
                .ok_or_else(|| err("E_PROTO_DECODE", "record without content"))?)?;
        }
        Ok(())
    }
}

pub fn ingest_many(
    storage: &Storage,
    inputs: &[PathBuf],
    source_root: Option<&Path>,
    repository: Option<&str>,
) -> Result<Value> {
    if inputs.is_empty() {
        return Err(err("E_USAGE", "at least one input is required"));
    }
    let db = &storage.conn;
    db.execute_batch("BEGIN IMMEDIATE; PRAGMA defer_foreign_keys=ON;")
        .map_err(sql)?;
    let result = (|| {
        let name = repository.unwrap_or("cir-fixtures");
        let uri = format!("ske:{name}");
        let conflict:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM repositories WHERE (name=?1 AND root_uri<>?2) OR (root_uri=?2 AND name<>?1))",params![name,uri],|r|r.get(0)).map_err(sql)?;
        if conflict {
            return Err(err(
                "E_USAGE",
                "repository identity conflicts; choose another --repository",
            ));
        }
        db.execute(
            "INSERT INTO repositories(name,root_uri) VALUES(?1,?2) ON CONFLICT(name) DO NOTHING",
            params![name, uri],
        )
        .map_err(sql)?;
        let repo: i64 = db
            .query_row("SELECT id FROM repositories WHERE name=?1", [name], |r| {
                r.get(0)
            })
            .map_err(sql)?;
        db.execute("INSERT INTO snapshots(repository_id) VALUES(?1)", [repo])
            .map_err(sql)?;
        let snapshot = db.last_insert_rowid();
        for (ordinal, input) in inputs.iter().enumerate() {
            db.execute_batch(&format!("SAVEPOINT unit_{ordinal}"))
                .map_err(sql)?;
            let mut file = File::open(input)
                .map_err(|e| err("E_USAGE", format!("{}: {e}", input.display())))?;
            import_unit(db, snapshot, &mut file, source_root)?;
            db.execute_batch(&format!("RELEASE SAVEPOINT unit_{ordinal}"))
                .map_err(sql)?;
        }
        close_snapshot(db, snapshot, false)
    })();
    match result {
        Ok(value) => {
            if let Err(e) = db.execute_batch("COMMIT") {
                let _ = db.execute_batch("ROLLBACK");
                Err(sql(e))
            } else {
                Ok(value)
            }
        }
        Err(e) => {
            let _ = db.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

fn close_snapshot(db: &Connection, snapshot: i64, abort_empty: bool) -> Result<Value> {
    // Link only unambiguous external stubs within this snapshot and language.
    db.execute("UPDATE functions AS stub SET resolved_function_id=(SELECT MIN(real.id) FROM functions real JOIN symbols rs ON rs.id=real.symbol_id JOIN compilation_units ru ON ru.id=rs.unit_id JOIN symbols ss ON ss.id=stub.symbol_id JOIN compilation_units su ON su.id=ss.unit_id WHERE real.is_external=0 AND rs.snapshot_id=?1 AND rs.qualified_name=ss.qualified_name AND ru.language=su.language HAVING COUNT(*)=1) WHERE stub.is_external=1 AND stub.symbol_id IN (SELECT id FROM symbols WHERE snapshot_id=?1)",[snapshot]).map_err(sql)?;
    let (source, config) = fingerprints(db, snapshot)?;
    db.execute("UPDATE snapshots SET status='CLOSED',closed_at=CURRENT_TIMESTAMP,source_fingerprint=?2,config_fingerprint=?3,inventory_status=CASE WHEN NOT EXISTS(SELECT 1 FROM compilation_units WHERE snapshot_id=?1 AND inventory_status<>'FAILED') THEN 'FAILED' WHEN EXISTS(SELECT 1 FROM compilation_units WHERE snapshot_id=?1 AND inventory_status<>'COMPLETE') THEN 'PARTIAL' ELSE 'COMPLETE' END WHERE id=?1",params![snapshot,source,config]).map_err(sql)?;
    let status: String = db
        .query_row(
            "SELECT inventory_status FROM snapshots WHERE id=?1",
            [snapshot],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let units: Vec<Value> = {
        let mut s=db.prepare("SELECT unit_key,inventory_status FROM compilation_units WHERE snapshot_id=?1 ORDER BY unit_key").map_err(sql)?;
        let units=s.query_map([snapshot],|r|Ok(json!({"unit_key":r.get::<_,String>(0)?,"inventory_status":r.get::<_,String>(1)?}))).map_err(sql)?.collect::<std::result::Result<_,_>>().map_err(sql)?;
        units
    };
    let lifecycle = if abort_empty && status == "FAILED" {
        "ABORTED"
    } else {
        "CLOSED"
    };
    if lifecycle == "ABORTED" {
        db.execute(
            "UPDATE snapshots SET status='ABORTED' WHERE id=?1",
            [snapshot],
        )
        .map_err(sql)?;
    }
    let diagnostics = diagnostics(db, snapshot)?;
    Ok(
        json!({"snapshot_id":snapshot,"status":lifecycle,"inventory_status":status,"units":units,"diagnostics":diagnostics}),
    )
}

pub fn diagnostics(db: &Connection, snapshot: i64) -> Result<Value> {
    let mut out = json!({"INFO":0,"WARNING":0,"ERROR":0});
    let mut stmt = db
        .prepare("SELECT severity,COUNT(*) FROM diagnostics WHERE snapshot_id=?1 GROUP BY severity")
        .map_err(sql)?;
    for row in stmt
        .query_map([snapshot], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(sql)?
    {
        let (severity, n) = row.map_err(sql)?;
        out[severity] = json!(n);
    }
    Ok(out)
}

fn fingerprints(db: &Connection, snapshot: i64) -> Result<(String, String)> {
    fn lines(db: &Connection, q: &str, snapshot: i64) -> Result<String> {
        let mut s = db.prepare(q).map_err(sql)?;
        let rows = s
            .query_map([snapshot], |r| r.get::<_, String>(0))
            .map_err(sql)?;
        let mut out = String::new();
        for row in rows {
            out.push_str(&row.map_err(sql)?);
        }
        Ok(out)
    }
    let source=lines(db,"SELECT path||char(9)||content_hash||char(10) FROM source_files WHERE snapshot_id=?1 ORDER BY path COLLATE BINARY",snapshot)?;
    let config=lines(db,"SELECT unit_key||char(9)||build_fingerprint||char(9)||semantic_profile_hash||char(9)||adapter_name||'@'||adapter_version||char(10) FROM compilation_units WHERE snapshot_id=?1 ORDER BY unit_key COLLATE BINARY",snapshot)?;
    Ok((sha256_hex(source.as_bytes()), sha256_hex(config.as_bytes())))
}

fn import_unit(
    db: &Connection,
    snapshot: i64,
    reader: &mut impl Read,
    root: Option<&Path>,
) -> Result<()> {
    import_records(
        db,
        snapshot,
        &mut || read_record(reader, MAX_MESSAGE_BYTES),
        root,
        None,
    )
}

fn import_records(
    db: &Connection,
    snapshot: i64,
    next: &mut dyn FnMut() -> Result<Option<Record>>,
    root: Option<&Path>,
    expected_key: Option<&str>,
) -> Result<()> {
    let header = match next()?.and_then(|r| r.content) {
        Some(Content::Header(h)) => h,
        _ => return Err(err("E_HEADER_MISSING", "first record must be Header")),
    };
    if header.cir_major != 1 {
        return Err(err("E_CIR_MAJOR_UNSUPPORTED", "expected CIR major 1"));
    }
    let def = match next()?.and_then(|r| r.content) {
        Some(Content::UnitDef(u)) => u,
        _ => return Err(err("E_RECORD_ORDER", "second record must be UnitDef")),
    };
    if def.unit_id.is_empty() || def.unit_key.is_empty() {
        return Err(err("E_EMPTY_ID", "unit identity is empty"));
    }
    if expected_key.is_some_and(|key| key != def.unit_key) {
        return Err(err(
            "E_RECORD_ORDER",
            "discovered and extracted unit keys disagree",
        ));
    }
    validate::known(&header.language, "go rust java")?;
    validate::known(&def.language, "go rust java")?;
    if header.language != def.language {
        return Err(err("E_TYPE_MISMATCH", "header and unit languages disagree"));
    }
    if db
        .query_row(
            "SELECT 1 FROM compilation_units WHERE snapshot_id=?1 AND unit_key=?2",
            params![snapshot, def.unit_key],
            |_| Ok(()),
        )
        .optional()
        .map_err(sql)?
        .is_some()
    {
        return Err(err("E_UNIT_DUPLICATE", def.unit_key));
    }
    db.execute("INSERT INTO compilation_units(snapshot_id,unit_key,language,adapter_name,adapter_version,toolchain_version,build_fingerprint,semantic_profile,semantic_profile_hash,capability_flags,inventory_status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'PARTIAL')",params![snapshot,def.unit_key,def.language,header.adapter_name,header.adapter_version,header.toolchain_version,header.build_fingerprint,header.semantic_profile,sha256_hex(header.semantic_profile.as_bytes()),serde_json::to_string(&header.capability_flags).unwrap()]).map_err(sql)?;
    let unit = Unit::new(db, snapshot, db.last_insert_rowid(), def.unit_key.clone())?;
    db.execute_batch("DROP TABLE IF EXISTS temp.ske_records; CREATE TEMP TABLE ske_records(ordinal INTEGER PRIMARY KEY,body BLOB NOT NULL,space TEXT,object_id INTEGER); CREATE INDEX temp.ske_records_object ON ske_records(space,object_id);").map_err(sql)?;
    let mut ordinal = 0;
    let end = loop {
        let record = next()?.ok_or_else(|| err("E_END_UNIT_MISSING", "missing EndUnit"))?;
        let content = record
            .content
            .as_ref()
            .ok_or_else(|| err("E_PROTO_DECODE", "record without content"))?;
        match content {
            Content::Header(_) | Content::UnitDef(_) => {
                return Err(err("E_RECORD_ORDER", "repeated Header/UnitDef"))
            }
            Content::EndUnit(end) => break end.clone(),
            _ => (),
        }
        validate::local(content)?;
        match content {
            Content::TypeDef(v) => {
                unit.declare("type", &v.id)?;
            }
            Content::SymbolDef(v) => {
                unit.declare("symbol", &v.id)?;
            }
            Content::ValueDef(v) => {
                unit.declare("value", &v.id)?;
            }
            Content::FunctionDef(v) => {
                unit.declare("function", &v.id)?;
            }
            Content::BlockDef(v) => {
                unit.declare("block", &v.id)?;
            }
            Content::InstructionDef(v) => {
                unit.declare("instruction", &v.id)?;
            }
            Content::ControlEdgeDef(v) => {
                unit.declare("edge", &v.id)?;
            }
            Content::SourceDecisionDef(v) => {
                unit.declare("decision", &v.id)?;
                for o in &v.outcomes {
                    unit.declare("outcome", &o.id)?;
                }
            }
            Content::ImplicitExitSite(v) => {
                unit.declare("exit", &v.id)?;
            }
            Content::SourceFileDef(v) => {
                unit.declare("file", &v.path)?;
            }
            _ => (),
        }
        // Reserve every forward reference before inserting this record. At EndUnit
        // the same traversal verifies that all reserved references were declared.
        validate::references(&unit, content)?;
        if let Content::SourceFileDef(file) = content {
            insert::file(&unit, file, root)?;
        }
        insert::record(&unit, content.clone())?;
        let identity = match content {
            Content::TypeDef(v) => Some(("type", unit.get("type", &v.id)?)),
            Content::ValueDef(v) => Some(("value", unit.get("value", &v.id)?)),
            Content::FunctionDef(v) => Some(("function", unit.get("function", &v.id)?)),
            Content::InstructionDef(v) => Some(("instruction", unit.get("instruction", &v.id)?)),
            Content::ControlEdgeDef(v) => Some(("edge", unit.get("edge", &v.id)?)),
            _ => None,
        };
        db.execute(
            "INSERT INTO temp.ske_records VALUES(?1,?2,?3,?4)",
            params![
                ordinal,
                record.encode_to_vec(),
                identity.map(|i| i.0),
                identity.map(|i| i.1)
            ],
        )
        .map_err(sql)?;
        ordinal += 1;
    };
    if next()?.is_some() {
        return Err(err("E_RECORD_ORDER", "record after EndUnit"));
    }
    if end.unit_id != def.unit_id {
        return Err(err("E_RECORD_ORDER", "EndUnit identity mismatch"));
    }
    validate::known(&end.inventory_status, "COMPLETE PARTIAL FAILED")?;
    unit.sealed.set(true);
    // Check declaration of references before structural validation.
    unit.records(|c| validate::references(&unit, &c))?;
    validate::unit(&unit)?;
    insert::derive(&unit)?;
    let actual: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM source_decisions WHERE unit_id=?1",
            [unit.id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let edges:i64=db.query_row("SELECT COUNT(*) FROM control_edges e JOIN basic_blocks b ON b.id=e.source_block_id JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=?1",[unit.id],|r|r.get(0)).map_err(sql)?;
    if end.emitted_source_decisions != actual as u64 || end.emitted_ir_edges != edges as u64 {
        return Err(err(
            "E_COUNTER_MISMATCH",
            "EndUnit emitted counters disagree",
        ));
    }
    db.execute("UPDATE functions SET inventory_status=CASE WHEN is_external=1 THEN 'COMPLETE' WHEN EXISTS(SELECT 1 FROM diagnostics d WHERE d.function_id=functions.id AND d.code='FUNCTION_TRANSLATION_FAILED') THEN 'FAILED' WHEN EXISTS(SELECT 1 FROM diagnostics d WHERE d.function_id=functions.id AND d.impact='INVENTORY' AND d.severity IN ('WARNING','ERROR')) THEN 'PARTIAL' ELSE 'COMPLETE' END WHERE symbol_id IN (SELECT id FROM symbols WHERE unit_id=?1)",[unit.id]).map_err(sql)?;
    let incomplete:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM functions f JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=?1 AND f.is_external=0 AND f.inventory_status<>'COMPLETE') OR EXISTS(SELECT 1 FROM diagnostics WHERE unit_id=?1 AND function_id IS NULL AND impact='INVENTORY' AND severity IN ('WARNING','ERROR'))",[unit.id],|r|r.get(0)).map_err(sql)?;
    let partial = end.expected_source_decisions != actual as u64
        || end.inventory_status != "COMPLETE"
        || incomplete;
    if partial && end.inventory_status == "COMPLETE" {
        insert::diagnostic(
            &unit,
            "WARNING",
            "INVENTORY_STATUS_DOWNGRADED",
            "INVENTORY",
            "core inventory checks disagree with adapter COMPLETE",
            None,
            None,
        )?;
    }
    let inventory = if end.inventory_status == "FAILED" {
        "FAILED"
    } else if partial {
        "PARTIAL"
    } else {
        "COMPLETE"
    };
    let expected = i64::try_from(end.expected_source_decisions).map_err(|_| {
        err(
            "E_COUNTER_MISMATCH",
            "expected counter overflows SQLite integer",
        )
    })?;
    db.execute("UPDATE compilation_units SET inventory_status=?2,expected_source_decisions=?3,source_decision_count=?4,ir_edge_count=?5,ir_decision_count=(SELECT COUNT(*) FROM ir_decisions d JOIN instructions i ON i.id=d.terminator_instruction_id JOIN basic_blocks b ON b.id=i.block_id JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=?1) WHERE id=?1",params![unit.id,inventory,expected,actual,edges]).map_err(sql)?;
    if root.is_none() {
        insert::diagnostic(
            &unit,
            "INFO",
            "SOURCE_NOT_VERIFIED",
            "OTHER",
            "source root not supplied",
            None,
            None,
        )?;
    }
    if header.cir_minor > 3 {
        insert::diagnostic(
            &unit,
            "INFO",
            "CIR_MINOR_NEWER",
            "OTHER",
            "newer CIR minor; unknown features not interpreted",
            None,
            None,
        )?;
    }
    db.execute_batch("DROP TABLE temp.ske_records")
        .map_err(sql)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_u32_be() {
        let r = Record {
            content: Some(Content::Header(Header {
                cir_major: 1,
                ..Default::default()
            })),
        };
        let body = r.encode_to_vec();
        let mut bytes = (body.len() as u32).to_be_bytes().to_vec();
        bytes.extend(body);
        assert_eq!(read_record(&mut bytes.as_slice(), 1024).unwrap(), Some(r));
    }
    #[test]
    fn framing_zero_length() {
        assert_eq!(
            read_record(&mut [0u8; 4].as_slice(), 1024)
                .unwrap_err()
                .code,
            "E_PROTO_DECODE"
        );
    }
    #[test]
    fn framing_oversize() {
        assert_eq!(
            read_record(&mut [255u8; 4].as_slice(), 1024)
                .unwrap_err()
                .code,
            "E_MSG_TOO_LARGE"
        );
    }
}
