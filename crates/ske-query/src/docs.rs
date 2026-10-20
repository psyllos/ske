//! Functional documentation fiches (v0.8, §7.5 + §11.2). Package/function
//! view: one Markdown fiche per function with a deterministic WHEN→THEN matrix,
//! status badges, proved-unreachable branches, and source anchors. The
//! authoritative run per function is its latest *complete* summary; repeated
//! behavior runs of the same summary are not duplicated. No business text is
//! ever invented (AC-0.5-08): the fiches render only rules and statuses. The
//! human-nominated capability title (when present) leads the fiche so a
//! non-technical reader immediately sees the business purpose.
//!
//! v0.8 cycle 3 (readability): INDEX.md carries a legend explaining the
//! vocabulary (scénario, cas, précision, statuts, preuve d'inaccessibilité)
//! and a deterministic "si … alors …" restatement of each case. Nothing new
//! is asserted about the analyzed code.
use rusqlite::Connection;
use serde_json::{json, Value};
use ske_common::{Error, Result};
use std::io::Write;
use std::path::Path;

/// Shared "how to read this documentation" legend (plain French, rendered in
/// INDEX.md and referenced by every fiche). It explains SKE's own vocabulary;
/// it never interprets the analyzed source.
const LEGEND: &str = r#"## Comment lire cette documentation

Cette documentation est **générée par SKE** à partir d'une analyse du code
source (Go, Rust, Java). Elle décrit ce que le moteur a *reconstruit*, pas ce
qu'une personne aurait rédigé : chaque affirmation est ancrée dans le code
analysé (principe « aucune interprétation métier inventée »).

### Vocabulaire

- **Scénario** : le comportement d'une fonction d'entrée, reconstruit par
  l'analyse interprocédurale. C'est la « fiche comportement » d'une fonction.
- **Cas (WHEN / THEN)** : une ligne de la matrice. `WHEN` = la condition,
  `THEN` = le résultat (valeur retournée ou effet observable). Rendus
  déterministes R1–R6, directement issus de l'analyse.
- **Précision** : fiabilité du résultat. `EXACT` = exact ; `OVERAPPROX` =
  sur-approximation (le moteur couvre *au moins* les vrais comportements) ;
  `PARTIAL` = partiel (certaines branches non couvertes) ; `UNKNOWN` = inconnu.
- **Règle** : un comportement extrait, rattaché à un cas (condition → résultat).
- **Statut d'une règle ou capacité** :
  - `UNVALIDATED` : reconstruit par le moteur, pas encore relu par un humain ;
  - `HUMAN_VALIDATED` : un humain a validé ou nommé explicitement ;
  - `REJECTED` : un humain a rejeté la proposition (le titre est conservé) ;
  - `STALE` : la source a changé depuis l'analyse, à rejouer.
- **Branche prouvée inatteignable** (`MODEL_UNREACHABLE`) : le solveur SMT
  (Z3) a prouvé qu'aucun chemin du *modèle* n'emprunte cette branche. C'est
  une forte indication, pas une certitude absolue du logiciel source.
- **Source** : ancre `fichier:ligne-colonne` dans le code analysé.
- **Run autoritatif** : le dernier scénario complet reconstruit pour la
  fonction ; les cas d'un même résumé ne sont jamais dupliqués.

Les titres de capacités (vocabulaire métier) proviennent **uniquement** de
nominations humaines (`ske capabilities nominate`). Rien d'autre n'est
interprété par le moteur.
"#;

fn sql(e: rusqlite::Error) -> Error {
    Error::new("E_DB_INTEGRITY", e.to_string())
}

fn rows(db: &Connection, q: &str, p: impl rusqlite::Params) -> Result<Vec<Value>> {
    let mut stmt = db.prepare(q).map_err(sql)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut cursor = stmt.query(p).map_err(sql)?;
    let mut out = Vec::new();
    while let Some(row) = cursor.next().map_err(sql)? {
        use rusqlite::types::ValueRef;
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

fn local(v: &Value) -> Value {
    v.as_str()
        .map(|s| json!(s.split_once('|').map(|(_, k)| k).unwrap_or(s)))
        .unwrap_or(Value::Null)
}

/// The authoritative summary (latest complete) for a function.
fn authoritative_summary(db: &Connection, function: i64) -> Result<Option<i64>> {
    use rusqlite::OptionalExtension;
    let id: Option<i64> = db
        .query_row(
            "SELECT id FROM function_summaries WHERE function_id=?1 AND complete=1
             ORDER BY id DESC LIMIT 1",
            [function],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    Ok(id)
}

fn latest_scenario(db: &Connection, function: i64) -> Option<i64> {
    db.query_row(
        "SELECT id FROM behavior_scenarios WHERE entry_function_id=?1 ORDER BY id DESC LIMIT 1",
        [function],
        |r| r.get(0),
    )
    .ok()
}

fn behavior_cases(db: &Connection, function: i64) -> Result<Vec<Value>> {
    let scenario = match latest_scenario(db, function) {
        Some(s) => s,
        None => return Ok(Vec::new()),
    };
    rows(
        db,
        "SELECT bc.case_ordinal, bc.guard_text, bc.result_text, bc.precision,
                sc.exit_span_id AS exit_span
         FROM behavior_cases bc
         JOIN summary_cases sc ON sc.id=bc.summary_case_id
         WHERE bc.behavior_id=?1
         ORDER BY bc.case_ordinal",
        [scenario],
    )
}

fn rules_for(db: &Connection, function: i64) -> Result<Vec<Value>> {
    let scenario = match latest_scenario(db, function) {
        Some(s) => s,
        None => return Ok(Vec::new()),
    };
    rows(
        db,
        "SELECT fr.rule_key, fr.description, fr.validation_status, fr.origin
         FROM functional_rules fr
         JOIN behavior_cases bc ON bc.id=fr.behavior_case_id
         WHERE bc.behavior_id=?1
         ORDER BY fr.id",
        [scenario],
    )
}

fn unreachable(db: &Connection, function: i64) -> Result<Vec<Value>> {
    rows(
        db,
        "SELECT o.label AS outcome, p.reason, p.verification_level,
                r.solver_name, r.solver_version
         FROM proof_records p
         JOIN analysis_runs r ON r.id=p.analysis_run_id
         JOIN ir_outcomes o ON o.id=p.ir_outcome_id
         JOIN control_edges e ON e.id=o.control_edge_id
         JOIN basic_blocks b ON b.id=e.source_block_id
         WHERE b.function_id=?1 AND p.verification_level='MODEL_UNREACHABLE'
         ORDER BY o.label",
        [function],
    )
}

fn span_label(db: &Connection, span: i64) -> Result<String> {
    let row = rows(
        db,
        "SELECT f.path, s.start_line, s.start_column, s.end_line, s.end_column
         FROM source_spans s JOIN source_files f ON f.id=s.source_file_id WHERE s.id=?1",
        [span],
    )?;
    let row = match row.first() {
        Some(r) => r,
        None => return Ok("-".to_owned()),
    };
    let path = std::path::Path::new(row["path"].as_str().unwrap_or(""))
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("?");
    Ok(format!(
        "{}:{}-{}",
        path,
        row["start_line"].as_i64().unwrap_or(0),
        row["end_line"].as_i64().unwrap_or(0)
    ))
}

fn escape_md(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

/// `ske docs render --capabilities`: write `CAPABILITIES.md`, a top-down
/// view of the snapshot's capabilities (title/status) with each linked
/// scenario's behavior matrix. Business names come from human nomination
/// (`HUMAN_VALIDATED`) only; nothing is invented.
pub fn render_capabilities(db: &Connection, snapshot: i64, out: &Path) -> Result<Value> {
    std::fs::create_dir_all(out).map_err(|e| {
        Error::new(
            "E_INTERNAL",
            format!("cannot create output directory {}: {e}", out.display()),
        )
    })?;
    let caps = rows(
        db,
        "SELECT fc.canonical_key, fc.title, fc.validation_status, fc.origin, fc.id AS cap_id
         FROM functional_capabilities fc WHERE fc.snapshot_id=?1 ORDER BY fc.canonical_key",
        [snapshot],
    )?;
    let mut body = String::new();
    body.push_str("# Capacités métier — vue top-down\n\n");
    body.push_str("> Généré par SKE. Les titres proviennent de la **nomination humaine**\n");
    body.push_str("> (`HUMAN_VALIDATED`) ; une capacité sans titre reste `UNVALIDATED`.\n");
    body.push_str("> Aucune dénomination n'est inventée par le moteur.\n\n");
    body.push_str(&format!(
        "Ce document regroupe les **{} capacité(s)** du snapshot par domaine\n",
        caps.len()
    ));
    body.push_str("métier, chaque capacité menant aux scénarios qui la réalisent puis à leur\n");
    body.push_str("matrice condition → résultat. C'est la lecture recommandée pour une\n");
    body.push_str("non-technique : commencez par les titres nommés par un humain.\n\n");
    body.push_str("> Légende et vocabulaire : voir « Comment lire cette documentation » dans\n");
    body.push_str("> `INDEX.md`.\n\n");
    let mut count = 0usize;
    for cap in &caps {
        let cap_id = cap["cap_id"].as_i64().unwrap();
        let title = cap["title"].as_str().unwrap_or("(non nommée)");
        let status = cap["validation_status"].as_str().unwrap_or("");
        let key = cap["canonical_key"].as_str().unwrap_or("");
        body.push_str(&format!("## {title}\n\n"));
        body.push_str(&format!("- clé technique : `{key}`\n"));
        let (statut_lisible, badge) = match status {
            "HUMAN_VALIDATED" => ("validée par un humain", "`HUMAN_VALIDATED`"),
            "REJECTED" => ("rejetée par un humain (titre conservé)", "`REJECTED`"),
            "STALE" => ("source modifiée depuis l'analyse — à rejouer", "`STALE`"),
            _ => ("non encore validée", "`UNVALIDATED`"),
        };
        body.push_str(&format!(
            "- statut : {badge} ({statut_lisible}) · origine : `{}`\n\n",
            cap["origin"].as_str().unwrap_or("")
        ));
        let scenarios = rows(
            db,
            "SELECT bs.entry_function_id, s.qualified_name AS entry
             FROM capability_scenarios csc
             JOIN behavior_scenarios bs ON bs.id=csc.scenario_id
             JOIN functions f ON f.id=bs.entry_function_id
             JOIN symbols s ON s.id=f.symbol_id
             WHERE csc.capability_id=?1 ORDER BY s.qualified_name",
            [cap_id],
        )?;
        if scenarios.is_empty() {
            body.push_str("_aucun scénario lié._\n\n");
            continue;
        }
        for sc in &scenarios {
            let entry = sc["entry"].as_str().unwrap();
            let fid = sc["entry_function_id"].as_i64().unwrap();
            let unit_key: String = db
                .query_row(
                    "SELECT u.unit_key FROM functions f JOIN symbols s ON s.id=f.symbol_id
                     JOIN compilation_units u ON u.id=s.unit_id WHERE f.id=?1",
                    [fid],
                    |r| r.get(0),
                )
                .map_err(sql)?;
            body.push_str(&format!("### Scénario `{entry}` ({unit_key})\n\n"));
            body.push_str("Ce scénario réalise la capacité ci-dessus : chaque ligne est un\n");
            body.push_str("comportement **si … alors …** reconstruit par le moteur.\n\n");
            let cases = behavior_cases(db, fid)?;
            body.push_str(
                "| # | Condition (quand / WHEN) | Résultat (alors / THEN) | Précision |\n",
            );
            body.push_str(
                "|---|---------------------------|--------------------------|-----------|\n",
            );
            for c in &cases {
                body.push_str(&format!(
                    "| {} | `{}` | `{}` | {} |\n",
                    c["case_ordinal"].as_i64().unwrap_or(0),
                    escape_md(c["guard_text"].as_str().unwrap_or("")),
                    escape_md(c["result_text"].as_str().unwrap_or("")),
                    c["precision"].as_str().unwrap_or("")
                ));
            }
            body.push('\n');
            count += 1;
        }
    }
    let file_path = out.join("CAPABILITIES.md");
    let mut handle = std::fs::File::create(&file_path)
        .map_err(|e| Error::new("E_INTERNAL", format!("cannot write {file_path:?}: {e}")))?;
    handle
        .write_all(body.as_bytes())
        .map_err(|e| Error::new("E_INTERNAL", format!("write {file_path:?}: {e}")))?;
    Ok(json!({
        "written": caps.len(),
        "scenarios_rendered": count,
        "file": "CAPABILITIES.md",
    }))
}

/// Generate one fiche for a function; returns (package, filename, body).
fn fiche(
    db: &Connection,
    function: i64,
    qualified: &str,
    unit_key: &str,
) -> Result<Option<(String, String, String)>> {
    if authoritative_summary(db, function)?.is_none() {
        return Ok(None);
    }
    let cases = behavior_cases(db, function)?;
    if cases.is_empty() {
        return Ok(None);
    }
    let rules = rules_for(db, function)?;
    let unreas = unreachable(db, function)?;
    let package = unit_key.split('@').next().unwrap_or(unit_key).to_owned();
    let filename = format!("{}.md", qualified.replace(['/', '.', '(', ')', '*'], "_"));
    let mut body = String::new();
    let capability_key = format!("function:{unit_key}|{qualified}");
    let cap = rows(
        db,
        "SELECT title, validation_status FROM functional_capabilities
         WHERE canonical_key=?1 AND title IS NOT NULL",
        [&capability_key],
    )?;
    let cap_title = cap
        .first()
        .and_then(|c| c["title"].as_str())
        .map(|s| s.to_owned());
    // Lead with the human-nominated business name when present; otherwise the
    // function name carries the fiche.
    match &cap_title {
        Some(title) => {
            body.push_str(&format!("# {title}\n\n"));
            body.push_str(&format!(
                "> Fonction `{qualified}` — fiche générée par SKE.\n"
            ));
        }
        None => {
            body.push_str(&format!("# Fonction `{qualified}`\n\n"));
        }
    }
    body.push_str("> Guide de lecture : voir la légende « Comment lire cette documentation »\n");
    body.push_str("> dans `INDEX.md` (vocabulaire, statuts, précision).\n\n");
    body.push_str(&format!("- unité : `{unit_key}`\n"));
    if let (Some(title), Some(c)) = (cap_title.as_deref(), cap.first()) {
        let statut_lisible = match c["validation_status"].as_str() {
            Some("HUMAN_VALIDATED") => "validée par un humain",
            Some("REJECTED") => "rejetée par un humain",
            Some("STALE") => "source modifiée depuis l'analyse",
            _ => "non encore validée",
        };
        body.push_str(&format!("- capacité : **{title}** — {statut_lisible}\n"));
    }
    let precision = cases[0]["precision"].as_str().unwrap_or("");
    body.push_str(&format!(
        "- cas identifiés : {}{} · précision dominante : `{precision}`\n",
        cases.len(),
        if rules.is_empty() {
            String::new()
        } else {
            format!(" · règles : {}", rules.len())
        }
    ));
    if !unreas.is_empty() {
        body.push_str(&format!(
            "- branches prouvées inatteignables : {}\n",
            unreas.len()
        ));
    }
    body.push('\n');
    // Deterministic plain-language restatement (no invented business meaning).
    body.push_str("## En clair — comportements reconstruits\n\n");
    body.push_str("Chaque ligne dit : **si** la condition est vraie **alors** le résultat\n");
    body.push_str("observé est celui indiqué, tel que reconstruit par l'analyse.\n\n");
    for c in &cases {
        let src = match c["exit_span"].as_i64() {
            Some(span) => span_label(db, span)?,
            None => "-".to_owned(),
        };
        body.push_str(&format!(
            "- **Cas {}** — si `{}` alors `{}` _(source {})_\n",
            c["case_ordinal"].as_i64().unwrap_or(0),
            escape_md(c["guard_text"].as_str().unwrap_or("")),
            escape_md(c["result_text"].as_str().unwrap_or("")),
            src
        ));
    }
    body.push('\n');
    body.push_str("## Matrice complète (condition → résultat)\n\n");
    body.push_str(
        "| # | Condition (quand / WHEN) | Résultat (alors / THEN) | Précision | Source |\n",
    );
    body.push_str(
        "|---|---------------------------|--------------------------|-----------|--------|\n",
    );
    for c in &cases {
        let src = match c["exit_span"].as_i64() {
            Some(span) => span_label(db, span)?,
            None => "-".to_owned(),
        };
        body.push_str(&format!(
            "| {} | `{}` | `{}` | {} | {} |\n",
            c["case_ordinal"].as_i64().unwrap_or(0),
            escape_md(c["guard_text"].as_str().unwrap_or("")),
            escape_md(c["result_text"].as_str().unwrap_or("")),
            c["precision"].as_str().unwrap_or(""),
            src
        ));
    }
    body.push('\n');
    if !rules.is_empty() {
        body.push_str("## Règles et statut\n\n");
        for r in &rules {
            let badge = match r["validation_status"].as_str() {
                Some("STALE") => "`STALE`",
                Some("HUMAN_VALIDATED") => "`HUMAN_VALIDATED`",
                _ => "`UNVALIDATED`",
            };
            body.push_str(&format!(
                "- {} — *{}* · {badge} · origine `{}`\n",
                local(&r["rule_key"])
                    .as_str()
                    .unwrap_or("")
                    .split_once('#')
                    .map(|(s, _)| s)
                    .unwrap_or(""),
                escape_md(r["description"].as_str().unwrap_or("")),
                r["origin"].as_str().unwrap_or("")
            ));
        }
        body.push('\n');
    }
    if !unreas.is_empty() {
        body.push_str("## Branches dont l'inaccessibilité est prouvée\n\n");
        for u in &unreas {
            body.push_str(&format!(
                "- `{}` — {} ({}) via {} {}\n",
                local(&u["outcome"]).as_str().unwrap_or(""),
                u["reason"].as_str().unwrap_or(""),
                u["verification_level"].as_str().unwrap_or(""),
                u["solver_name"].as_str().unwrap_or(""),
                u["solver_version"].as_str().unwrap_or("")
            ));
        }
        body.push('\n');
    }
    Ok(Some((package, filename, body)))
}

/// `ske docs render --out DIR`: write one fiche per function with a complete
/// summary in the snapshot, plus an index. Returns the DTO of written files.
pub fn render(db: &Connection, snapshot: i64, out: &Path) -> Result<Value> {
    std::fs::create_dir_all(out).map_err(|e| {
        Error::new(
            "E_INTERNAL",
            format!("cannot create output directory {}: {e}", out.display()),
        )
    })?;
    let funcs = rows(
        db,
        "SELECT s.qualified_name AS qualified, u.unit_key, s.id AS symbol_id, f.id AS function_id
         FROM functions f JOIN symbols s ON s.id=f.symbol_id
         JOIN compilation_units u ON u.id=s.unit_id
         WHERE s.snapshot_id=?1 AND f.is_external=0
         ORDER BY s.qualified_name, u.unit_key",
        [snapshot],
    )?;
    let mut packages: std::collections::BTreeMap<String, Vec<Value>> = Default::default();
    let mut files: Vec<Value> = Vec::new();
    let mut fiche_count = 0usize;
    for f in &funcs {
        let qualified = f["qualified"].as_str().unwrap();
        let unit_key = f["unit_key"].as_str().unwrap();
        let (package, filename, body) =
            match fiche(db, f["function_id"].as_i64().unwrap(), qualified, unit_key)? {
                Some(x) => x,
                None => continue,
            };
        let file_path = out.join(&filename);
        let mut handle = std::fs::File::create(&file_path)
            .map_err(|e| Error::new("E_INTERNAL", format!("cannot write {file_path:?}: {e}")))?;
        handle
            .write_all(body.as_bytes())
            .map_err(|e| Error::new("E_INTERNAL", format!("write {file_path:?}: {e}")))?;
        packages.entry(package.clone()).or_default().push(json!({
            "function": qualified,
            "file": filename,
            "package": package,
        }));
        files.push(json!({"function": qualified, "package": package, "file": filename}));
        fiche_count += 1;
    }
    // Index.
    let mut index = String::new();
    index.push_str("# Documentation fonctionnelle — index\n\n");
    index.push_str("> Générée par SKE (v0.8). Ce dossier décrit, pour chaque fonction ayant\n");
    index.push_str("> un scénario reconstruit, son comportement reconstruit : matrice\n");
    index.push_str("> condition → résultat, règles et statuts, branches prouvées\n");
    index.push_str("> inatteignables. Les statuts `STALE` signalent une source modifiée\n");
    index.push_str("> depuis le snapshot analysé (à rejouer).\n\n");
    index.push_str(&format!(
        "Ce snapshot référence **{} fiche(s)** : suivez les liens ci-dessous ou\n",
        fiche_count
    ));
    index.push_str("ouvrez `CAPABILITIES.md` (si généré) pour la vue métier top-down.\n\n");
    index.push_str("---\n\n");
    index.push_str(LEGEND);
    index.push_str("\n---\n\n");
    index.push_str("## Fonctions documentées\n\n");
    for (package, entries) in &packages {
        index.push_str(&format!("### `{package}`\n\n"));
        for e in entries {
            index.push_str(&format!(
                "- [{}]({})\n",
                e["function"].as_str().unwrap(),
                e["file"].as_str().unwrap()
            ));
        }
        index.push('\n');
    }
    let index_path = out.join("INDEX.md");
    let mut handle = std::fs::File::create(&index_path)
        .map_err(|e| Error::new("E_INTERNAL", format!("cannot write index: {e}")))?;
    handle
        .write_all(index.as_bytes())
        .map_err(|e| Error::new("E_INTERNAL", format!("write index: {e}")))?;
    files.push(json!({"package": "__index__", "file": "INDEX.md"}));
    Ok(json!({
        "snapshot_id": snapshot,
        "dir": out.to_string_lossy(),
        "written": fiche_count,
        "index": true,
        "files": files,
        "page": {"limit": 0, "offset": 0, "total": fiche_count + 1, "truncated": false}
    }))
}
