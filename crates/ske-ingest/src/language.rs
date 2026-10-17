//! Adapter ingestion uses independently committed units; CIR fixtures remain atomic.
use super::*;
use adapter::RunLimits;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Go,
    Rust,
    Java,
}

impl Language {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Go => "go",
            Self::Rust => "rust",
            Self::Java => "java",
        }
    }
}

pub struct GoOptions<'a> {
    pub repo: &'a Path,
    pub adapter: &'a Path,
    pub profile_json: &'a str,
    pub limits: &'a RunLimits,
    pub repository: Option<&'a str>,
    pub redact: bool,
}

/// Common adapter options, retained as an alias for Go API compatibility.
pub type IngestOptions<'a> = GoOptions<'a>;

pub fn ingest_go(storage: &Storage, options: &GoOptions<'_>) -> Result<Value> {
    ingest_language(storage, Language::Go, options)
}

pub fn ingest_rust(storage: &Storage, options: &IngestOptions<'_>) -> Result<Value> {
    ingest_language(storage, Language::Rust, options)
}

pub fn ingest_java(storage: &Storage, options: &IngestOptions<'_>) -> Result<Value> {
    ingest_language(storage, Language::Java, options)
}

pub fn ingest_language(
    storage: &Storage,
    language: Language,
    options: &IngestOptions<'_>,
) -> Result<Value> {
    options.limits.validate()?;
    let profile = crate::profile::normalize_for(language, Some(options.profile_json))?;
    let root = options
        .repo
        .canonicalize()
        .map_err(|e| err("E_USAGE", e.to_string()))?;
    if !root.is_dir() {
        return Err(err("E_USAGE", "--repo must be a directory"));
    }
    // Profile paths must stay inside the repository before consent or any
    // adapter subprocess is granted, closing absolute/.. escapes.
    crate::profile::validate_contained(language, &profile, &root)?;
    let db = &storage.conn;
    // The discovery result is deliberately committed independently of extraction.
    let snapshot = transaction(db, || {
        let uri = root.to_string_lossy();
        let name = options.repository.unwrap_or(&uri);
        let conflict: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM repositories WHERE name=?1 AND root_uri<>?2)",
                params![name, uri],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if conflict {
            return Err(err("E_USAGE", "repository identity conflicts"));
        }
        db.execute("INSERT INTO repositories(name,root_uri) VALUES(?1,?2) ON CONFLICT(root_uri) DO NOTHING",
            params![name, uri]).map_err(sql)?;
        let repository: i64 = db
            .query_row(
                "SELECT id FROM repositories WHERE root_uri=?1",
                [uri.as_ref()],
                |r| r.get(0),
            )
            .map_err(sql)?;
        db.execute(
            "INSERT INTO snapshots(repository_id) VALUES(?1)",
            [repository],
        )
        .map_err(sql)?;
        Ok(db.last_insert_rowid())
    })?;
    match adapter::list_units_for(language, options.adapter, &root, &profile, options.limits) {
        Ok(units) if !units.is_empty() => {
            for unit in units {
                let mut header = None;
                let result = transaction(db, || {
                    adapter::extract_unit(
                        options.adapter,
                        &root,
                        &profile,
                        &unit.unit_key,
                        options.limits,
                        |next| {
                            let mut first = true;
                            let mut observed_next = || {
                                let mut record = next()?;
                                if std::mem::take(&mut first) {
                                    if let Some(Record {
                                        content: Some(Content::Header(h)),
                                    }) = &record
                                    {
                                        if h.language != language.as_str() {
                                            return Err(err(
                                                "E_TYPE_MISMATCH",
                                                "adapter header language differs from selected ingest language",
                                            ));
                                        }
                                        if h.cir_major == 1
                                            && matches!(h.language.as_str(), "go" | "rust" | "java")
                                        {
                                            header = Some(h.clone());
                                        }
                                    }
                                }
                                // Adapter diagnostics can use UNIT_FAILED too. Redact
                                // at the input boundary, not by assuming that code is
                                // only produced by our own failure handler.
                                if options.redact {
                                    if let Some(Record {
                                        content: Some(Content::Diagnostic(d)),
                                    }) = &mut record
                                    {
                                        d.message = "adapter diagnostic (details redacted)".into();
                                    }
                                }
                                Ok(record)
                            };
                            import_records(
                                db,
                                snapshot,
                                &mut observed_next,
                                Some(&root),
                                Some(&unit.unit_key),
                            )
                        },
                    )
                });
                if let Err(error) = result {
                    failed(
                        db,
                        snapshot,
                        &unit.unit_key,
                        &error,
                        options.redact,
                        header.as_ref(),
                    )?;
                }
            }
        }
        Ok(_) => failed(
            db,
            snapshot,
            "failed:0",
            &err("E_ADAPTER_LIMIT", "no compilation units discovered"),
            options.redact,
            None,
        )?,
        Err(error) => failed(db, snapshot, "failed:0", &error, options.redact, None)?,
    }
    transaction(db, || {
        if options.redact {
            db.execute(
                "UPDATE diagnostics SET message=code || ': details redacted' WHERE snapshot_id=?1 AND code<>'UNIT_FAILED'",
                [snapshot],
            )
            .map_err(sql)?;
        }
        close_snapshot(db, snapshot, true)
    })
}

fn transaction<T>(db: &Connection, f: impl FnOnce() -> Result<T>) -> Result<T> {
    db.execute_batch("BEGIN IMMEDIATE; PRAGMA defer_foreign_keys=ON;")
        .map_err(sql)?;
    match f() {
        Ok(value) => match db.execute_batch("COMMIT") {
            Ok(()) => Ok(value),
            Err(error) => {
                let _ = db.execute_batch("ROLLBACK");
                Err(sql(error))
            }
        },
        Err(error) => {
            let _ = db.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn failed(
    db: &Connection,
    snapshot: i64,
    key: &str,
    error: &Error,
    redact: bool,
    header: Option<&Header>,
) -> Result<()> {
    transaction(db, || {
        let empty = sha256_hex(b"");
        db.execute("INSERT INTO compilation_units(snapshot_id,unit_key,language,adapter_name,adapter_version,toolchain_version,build_fingerprint,semantic_profile,semantic_profile_hash,capability_flags,inventory_status) VALUES(?1,?2,'unknown','unknown','unknown','unknown',?3,'unknown',?3,'[]','FAILED')",
            params![snapshot,key,empty]).map_err(sql)?;
        let unit = db.last_insert_rowid();
        if let Some(h) = header {
            db.execute("UPDATE compilation_units SET language=?1,adapter_name=?2,adapter_version=?3,toolchain_version=?4,build_fingerprint=?5,semantic_profile=?6,semantic_profile_hash=?7,capability_flags=?8 WHERE id=?9",
                params![h.language,h.adapter_name,h.adapter_version,h.toolchain_version,h.build_fingerprint,h.semantic_profile,sha256_hex(h.semantic_profile.as_bytes()),serde_json::to_string(&h.capability_flags).unwrap(),unit]).map_err(sql)?;
        }
        let message = if redact {
            format!("{}: unit extraction failed (details redacted)", error.code)
        } else {
            error.to_string()
        };
        db.execute("INSERT INTO diagnostics(snapshot_id,unit_id,severity,code,impact,message) VALUES(?1,?2,'ERROR','UNIT_FAILED','INVENTORY',?3)",
            params![snapshot,unit,message]).map_err(sql)?;
        Ok(())
    })
}
