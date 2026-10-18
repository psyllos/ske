//! Query-connected Context Builder adapter (AC-0.7-06).
//!
//! Materializes an identity-resolved `rule -> case -> issue -> proof` graph
//! for one selected function/snapshot from **public, read-only** `ske_query`
//! DTOs, then prices the Context Builder budget ([`crate::context`]).
//!
//! No SQL and no direct database access live in `ske-api`: the adapter only
//! calls `Query::list_rules` and `Query::branches_run`. Function scoping uses
//! the rule-key prefix (`{qualified_name}@`) that `ske-behavior` assigns, so
//! unrelated functions and unreachable graph nodes are excluded by
//! construction rather than by text matching.
use crate::context::{build_from_roots, Section, Status};
use serde_json::Value;
use ske_common::{Error, Result};
use ske_query::Query;
use std::collections::{BTreeMap, BTreeSet};

/// Page size for DTO listings. `Query` caps at 10 000 and pagination is
/// exhaustion-based, so any page size is safe as long as we keep following
/// pages until `total` is covered.
const PAGE: usize = 10_000;

/// Build a bounded knowledge context for `function` in `snapshot`.
///
/// The function is resolved and validated through `Query` itself: any name
/// `ske-query` can resolve (e.g. `example.Check` or a bare resolvable name)
/// works, and the authoritative qualified name returned by `Query` scopes
/// every graph node, so unrelated rules/outcomes are never selected.
pub fn build_function_context(
    query: &Query,
    snapshot: &str,
    repository: Option<&str>,
    function: &str,
    max_tokens: usize,
) -> Result<crate::context::Context> {
    // 0) Resolve/validate the function once, read-only. `Query` errors on
    //    unresolved or ambiguous names (fail closed); `function` in the DTO is
    //    the authoritative qualified name used for structural scoping.
    let resolved = query.branches_page(snapshot, repository, function, 1, 0)?;
    let function = resolved["function"]
        .as_str()
        .ok_or_else(|| Error::new("E_DB_INTEGRITY", "branches_page returned no function"))?
        .to_owned();

    // 1) Rules + case evidence for the function, read-only, following pages.
    let scope = format!("{function}@");
    let mut rules = Vec::new();
    let mut offset = 0usize;
    loop {
        let dto = query.list_rules(snapshot, repository, true, PAGE, offset)?;
        let listed = dto["rules"]
            .as_array()
            .ok_or_else(|| Error::new("E_DB_INTEGRITY", "list_rules returned no rules array"))?;
        if listed.is_empty() {
            break;
        }
        for rule in listed {
            if rule["rule_key"]
                .as_str()
                .is_some_and(|key| key.starts_with(&scope))
            {
                rules.push(rule.clone());
            }
        }
        let total = dto["page"]["total"].as_u64().unwrap_or(0) as usize;
        offset += listed.len();
        if total == 0 || offset >= total {
            break;
        }
    }

    // 2) IR outcomes (status + proofs) of the function, read-only.
    //    A missing BRANCH_ANALYSIS run is not an adapter failure: rules still
    //    materialize, and the unreferenced statuses are left UNANALYZED.
    let mut outcomes: BTreeMap<String, Value> = BTreeMap::new();
    match ir_outcomes_all(query, snapshot, repository, &function) {
        Ok(map) => outcomes = map,
        Err(e) if e.code == "E_NOT_FOUND" => { /* no analysis run: no statuses */ }
        Err(e) => return Err(e),
    }

    // 3) Assemble the graph and let the Context Builder price the budget.
    let (graph, roots) = materialize(&function, &rules, &outcomes)?;
    build_from_roots(&graph, &roots, max_tokens)
}

/// All IR outcomes (status + evidence) of a function, read-only, exhausted by
/// paging through `branches_run(..., "latest", ...)`. `Query` resolves and
/// validates the qualified name; the returned DTO's `function` is authoritative.
fn ir_outcomes_all(
    query: &Query,
    snapshot: &str,
    repository: Option<&str>,
    function: &str,
) -> Result<BTreeMap<String, Value>> {
    let mut outcomes = BTreeMap::new();
    let mut offset = 0usize;
    let mut total = usize::MAX;
    while offset < total {
        let dto = query.branches_run(snapshot, repository, function, "latest", PAGE, offset)?;
        total = dto["page"]["total"].as_u64().unwrap_or(0) as usize;
        if let Some(ir) = dto["ir_outcomes"].as_array() {
            for outcome in ir {
                if let Some(id) = outcome["id"].as_str() {
                    outcomes.insert(id.to_owned(), outcome.clone());
                }
            }
        }
        offset += PAGE;
        if total == 0 {
            break;
        }
    }
    Ok(outcomes)
}

/// Build the structural graph from already-resolved, read-only DTOs.
///
/// Extracted as a pure function so the real DTO shapes are exercised without a
/// database. `function` is the authoritative qualified name, `rules` only the
/// rows of that function, `outcomes` the IR status map keyed by local label.
fn materialize(
    function: &str,
    rules: &[Value],
    outcomes: &BTreeMap<String, Value>,
) -> Result<(BTreeMap<String, Section>, Vec<String>)> {
    let mut graph: BTreeMap<String, Section> = BTreeMap::new();
    let mut root_links: Vec<String> = Vec::new();
    let mut referenced = BTreeSet::new();

    // Rules -> rule + case nodes; case links its GUARD_OUTCOME IR anchors.
    // Only rules under the selected function survive; any unrelated rule is
    // dropped here, so the graph is scoped even if given a mixed listing.
    let scope = format!("{function}@");
    for rule in rules {
        let rule_key = rule["rule_key"]
            .as_str()
            .ok_or_else(|| Error::new("E_DB_INTEGRITY", "rule without rule_key"))?;
        if !rule_key.starts_with(&scope) {
            continue;
        }
        let description = rule["description"].as_str().unwrap_or("");
        let short = rule["short_name"].as_str().unwrap_or("");
        let text = if description.is_empty() {
            short.to_owned()
        } else {
            description.to_owned()
        };
        let rule_key_node = node_key("rule", rule_key);
        graph.insert(
            rule_key_node.clone(),
            Section {
                kind: "rule",
                text,
                status: None,
                links: Vec::new(),
            },
        );
        let case_key = node_key("case", rule_key);
        let mut case_links = Vec::new();
        if let Some(evidence) = rule["evidence"].as_array() {
            for anchor in evidence {
                if anchor["kind"].as_str() == Some("ir_outcome") {
                    if let Some(id) = anchor["id"].as_str() {
                        let issue = node_key("issue", id);
                        case_links.push(issue.clone());
                        referenced.insert(issue);
                    }
                }
            }
        }
        graph.insert(
            case_key.clone(),
            Section {
                kind: "case",
                text: format!("case for {rule_key}"),
                status: None,
                links: case_links,
            },
        );
        root_links.push(rule_key_node);
        root_links.push(case_key);
    }

    // IR outcomes -> issue status + proof links.
    for (label, entry) in outcomes {
        let status = entry_status(entry).unwrap_or(Status::Unanalyzed);
        let mut links = Vec::new();
        if let Some(evidence) = entry["evidence"].as_array() {
            for anchor in evidence {
                if anchor["kind"].as_str() == Some("proof") {
                    if let Some(id) = anchor["id"].as_u64() {
                        links.push(node_key("proof", &id.to_string()));
                    }
                }
            }
        }
        let issue = node_key("issue", label);
        graph.insert(
            issue.clone(),
            Section {
                kind: "issue",
                text: format!("outcome {label} {}", entry_explanation(entry)),
                status: Some(status),
                links,
            },
        );
        // A relevant unknown never referenced by a rule is a function-level
        // unknown: mandatory and linked from the function root.
        if status.is_unknown() && !referenced.contains(&issue) {
            root_links.push(issue);
        }
    }

    // GUARD_OUTCOME anchors that had no analysis entry: still structural facts,
    // but their status is unknown (no source status claim is invented).
    for issue in &referenced {
        if graph.contains_key(issue) {
            continue;
        }
        let label = issue.trim_start_matches("issue:");
        graph.insert(
            issue.clone(),
            Section {
                kind: "issue",
                text: format!("outcome {label} (no analysis result)"),
                status: Some(Status::Unanalyzed),
                links: Vec::new(),
            },
        );
    }

    // Proof nodes, reading the proof payload from the outcome evidence.
    let proof_links: BTreeSet<String> = graph
        .values()
        .flat_map(|s| s.links.iter().cloned())
        .filter(|k| k.starts_with("proof:"))
        .collect();
    for proof in &proof_links {
        if graph.contains_key(proof) {
            continue;
        }
        let id: i64 = proof
            .trim_start_matches("proof:")
            .parse()
            .map_err(|_| Error::new("E_DB_INTEGRITY", format!("bad proof id in {proof}")))?;
        let mut text = format!("proof {id}");
        for entry in outcomes.values() {
            if let Some(evidence) = entry["evidence"].as_array() {
                for anchor in evidence {
                    if anchor["kind"].as_str() == Some("proof")
                        && anchor["id"].as_u64() == Some(id as u64)
                    {
                        if let Some(p) = anchor["proof"].as_object() {
                            let verifier = p
                                .get("verifier_name")
                                .and_then(Value::as_str)
                                .unwrap_or("verifier");
                            let verdict = p.get("verdict").and_then(Value::as_str).unwrap_or("");
                            let level = p
                                .get("verification_level")
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            text = format!("proof {id}: {verifier} {verdict} {level}")
                                .trim_end()
                                .to_string();
                        }
                        break;
                    }
                }
            }
        }
        graph.insert(
            proof.clone(),
            Section {
                kind: "proof",
                text,
                status: Some(Status::ProvenUnreachable),
                links: Vec::new(),
            },
        );
    }

    // Function root: rule layer, linking the selected rule/case/unknown nodes.
    root_links.sort();
    root_links.dedup();
    let root_key = node_key("rule", function);
    graph.insert(
        root_key.clone(),
        Section {
            kind: "rule",
            text: format!("function {function}"),
            status: None,
            links: root_links,
        },
    );
    Ok((graph, vec![root_key]))
}

fn node_key(kind: &str, id: &str) -> String {
    format!("{kind}:{id}")
}

/// Explanatory prose for an IR outcome entry, if any.
fn entry_explanation(entry: &Value) -> String {
    entry["explanation"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| format!("({s})"))
        .unwrap_or_default()
}

/// Map a persisted IR outcome status onto the Context Builder status.
fn entry_status(entry: &Value) -> Option<Status> {
    match entry["status"].as_str()? {
        "PROVEN_UNREACHABLE" => Some(Status::ProvenUnreachable),
        "POSSIBLE" => Some(Status::Possible),
        "WITNESSED" => Some(Status::Witnessed),
        "UNANALYZED" => Some(Status::Unanalyzed),
        "UNKNOWN" => Some(Status::Unknown),
        "TIMEOUT" => Some(Status::Timeout),
        "UNSUPPORTED" => Some(Status::Unsupported),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::estimate_tokens;
    use std::sync::OnceLock;

    fn ir(label: &str, status: &str, proofs: &[u64]) -> Value {
        let evidence: Vec<Value> = proofs
            .iter()
            .map(|id| {
                serde_json::json!({
                    "kind": "proof", "id": id, "proof": {
                        "verifier_name": "z3", "verdict": "ACCEPTED",
                        "verification_level": "MODEL_UNREACHABLE",
                        "model_soundness": "EXACT", "eliminated_arrivals": 0,
                        "residual_frontier": 0, "reason": "proved"
                    }
                })
            })
            .collect();
        serde_json::json!({
            "id": label, "status": status, "soundness": "EXACT",
            "explanation": Value::Null, "evidence": evidence
        })
    }

    fn rule(rule_key: &str, outcome_labels: &[&str]) -> Value {
        let evidence: Vec<Value> = outcome_labels
            .iter()
            .map(|id| {
                serde_json::json!({
                    "kind": "ir_outcome", "id": id, "unit_key": "u",
                    "relation": "GUARD_OUTCOME"
                })
            })
            .collect();
        serde_json::json!({
            "rule_key": rule_key, "short_name": "age < 18",
            "description": "WHEN age < 18 THEN return ErrUnderage",
            "origin": "DETERMINISTIC", "validation_status": "UNVALIDATED",
            "evidence": evidence
        })
    }

    #[test]
    fn excludes_unrelated_functions_keeps_unknowns_and_budget() {
        let function = "example.Check";
        // A rule of the *selected* function plus an unrelated function.
        let rules = vec![
            rule("example.Check@run1#case0", &["e1", "u1"]),
            rule("example.Other@run9#case0", &["z9"]),
        ];
        // Outcomes are function-scoped by `ir_outcomes_all` (labels are local
        // per function), so a foreign function's `z9` is never offered.
        let outcomes = BTreeMap::from([
            ("e1".to_owned(), ir("e1", "POSSIBLE", &[])),
            ("u1".to_owned(), ir("u1", "UNKNOWN", &[])),
            ("t1".to_owned(), ir("t1", "TIMEOUT", &[])),
        ]);
        let (graph, roots) = materialize(function, &rules, &outcomes).unwrap();
        // Unrelated function's rule is excluded by the scope guard, and its
        // outcome label is never materialized (function-scoped upstream).
        assert!(!graph.contains_key("rule:example.Other@run9#case0"));
        assert!(!graph.contains_key("issue:z9"));
        // The selected function's unknown AND timeout are mandatory unknowns.
        let ctx = build_from_roots(&graph, &roots, 500).unwrap();
        assert!(ctx.text.contains("UNKNOWN"), "{}", ctx.text);
        assert!(ctx.text.contains("TIMEOUT"), "{}", ctx.text);
        assert!(ctx.text.contains("age < 18"));
        assert!(!ctx.text.contains("example.Other"));
        assert!(ctx.tokens_used <= 500);
        assert_eq!(
            ctx.tokens_used,
            estimate_tokens(&serde_json::to_string(&ctx).unwrap())
        );
        // Deterministic across repeated builds with reversed rule order.
        let mut reversed = rules.clone();
        reversed.reverse();
        let (graph2, roots2) = materialize(function, &reversed, &outcomes).unwrap();
        let again = build_from_roots(&graph2, &roots2, 500).unwrap();
        assert_eq!(
            serde_json::to_string(&ctx).unwrap(),
            serde_json::to_string(&again).unwrap()
        );
    }

    #[test]
    fn proofs_and_missing_analysis_refs_resolve() {
        let function = "example.Check";
        let rules = vec![rule("example.Check@run1#case0", &["e1", "absent"])];
        let outcomes = BTreeMap::from([
            ("e1".to_owned(), ir("e1", "PROVEN_UNREACHABLE", &[7])),
            ("z9".to_owned(), ir("z9", "TIMEOUT", &[])),
        ]);
        let (graph, roots) = materialize(function, &rules, &outcomes).unwrap();
        // Case references `absent` (no analysis entry): kept as UNANALYZED, and
        // the unknown `z9` is a function-level relevant unknown.
        assert!(graph["case:example.Check@run1#case0"]
            .links
            .contains(&"issue:absent".to_string()));
        assert_eq!(graph["issue:absent"].status, Some(Status::Unanalyzed));
        // Proof payload resolved from the outcome evidence.
        assert!(graph["issue:e1"].links.contains(&"proof:7".to_string()));
        assert!(graph["proof:7"]
            .text
            .contains("z3 ACCEPTED MODEL_UNREACHABLE"));
        let ctx = build_from_roots(&graph, &roots, 500).unwrap();
        assert!(ctx.text.contains("TIMEOUT"));
        assert!(ctx.text.contains("z3 ACCEPTED"));
        assert!(
            !ctx.text.contains("z9") || ctx.text.contains("outcome z9"),
            "unknown kept"
        );
        // Impossible budget for mandatory unknowns fails explicitly.
        let mut tiny = outcomes.clone();
        tiny.insert("z9".to_owned(), ir("z9", "TIMEOUT", &[]));
        let (g2, r2) = materialize(function, &rules, &tiny).unwrap();
        assert!(build_from_roots(&g2, &r2, 4).is_err());
    }

    // ----- Real fixture, end to end ----------------------------------------
    //
    // The full flow through the public `ske-query` façade against an actually
    // ingested Go repository: adapt the Go fixture, ingest, run BRANCH_ANALYSIS
    // (real Z3), reconstruct rules+cases+evidence, then build the context.
    // No SQL, no storage access, no Cargo changes: only `Query` methods.

    fn go_adapter() -> &'static std::path::PathBuf {
        static ADAPTER: OnceLock<std::path::PathBuf> = OnceLock::new();
        ADAPTER.get_or_init(|| {
            use std::process::Command;
            let root = {
                use std::path::PathBuf;
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../..")
                    .canonicalize()
                    .unwrap()
            };
            let out = root.join("target/ske-api-context-adapter-go");
            let status = Command::new("go")
                .current_dir(root.join("adapters/go"))
                .args(["build", "-p=1", "-o"])
                .arg(&out)
                .arg("./cmd/ske-adapter-go")
                .env("GOTOOLCHAIN", "local")
                .env("GOMAXPROCS", "2")
                .status()
                .expect("go adapter build");
            assert!(status.success(), "go adapter build failed");
            out
        })
    }

    fn lab() -> (tempfile::TempDir, Query) {
        use ske_query::{GoOptions, RunLimits};
        let root = {
            use std::path::PathBuf;
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()
                .unwrap()
        };
        let temp = tempfile::tempdir().unwrap();
        let query = Query::open(temp.path().join("context.sqlite")).unwrap();
        let repo = root.join("examples/go-branch-lab");
        let profile =
            std::fs::read_to_string(repo.join("build-profile.json")).expect("fixture profile");
        let limits = RunLimits::default();
        let out = query
            .ingest_go(&GoOptions {
                repo: &repo,
                adapter: go_adapter(),
                profile_json: &profile,
                limits: &limits,
                repository: None,
                redact: false,
            })
            .unwrap();
        assert_eq!(out["status"], "CLOSED");
        assert_eq!(out["diagnostics"]["ERROR"], 0);
        (temp, query)
    }

    fn analyzed(check: bool) -> (tempfile::TempDir, Query) {
        use ske_query::analysis_config::{Budget, Context};
        use std::sync::atomic::AtomicBool;
        let (_temp, mut query) = lab();
        // Real branch analysis (Z3) so statuses + proofs exist. `analysis::run`
        // also computes the summary `reconstruct_behaviors` needs.
        let summary = query
            .analyze(
                "latest",
                None,
                "example.Decide",
                &Budget::default(),
                &Context::default(),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert!(matches!(
            summary["run_status"].as_str(),
            Some("COMPLETED" | "PARTIAL")
        ));
        let rules = query
            .reconstruct_behaviors("latest", None, "example.Decide", 10_000, 0)
            .unwrap();
        assert!(!rules["cases"].as_array().unwrap().is_empty());
        if check {
            assert!(query.db_check().is_ok());
        }
        (_temp, query)
    }

    #[test]
    fn build_function_context_real_fixture_end_to_end() {
        let (_temp, query) = analyzed(true);
        // Only statuses known to Query for this fixture are used for the
        // structural facts; explanations are short and not dumped to stdout.
        let big = build_function_context(&query, "latest", None, "example.Decide", 4096).unwrap();
        assert!(big.tokens_used <= 4096);
        assert!(
            big.text.contains("WHEN") || big.text.contains("function example.Decide"),
            "rules present: {}",
            big.text
        );
        // Smaller budgets never exceed the cap and never grow the output.
        // A sub-envelope budget (envelope alone > 8 tokens) yields an empty
        // truncated context: `selected` drops but `tokens_used` still reports
        // the real serialized envelope cost.
        let small = build_function_context(&query, "latest", None, "example.Decide", 8).unwrap();
        assert!(small.truncated);
        assert!(small.selected < big.selected);
        assert_eq!(
            small.tokens_used,
            estimate_tokens(&serde_json::to_string(&small).unwrap())
        );
        // Every UNKNOWN/TIMEOUT/UNSUPPORTED outcome on the real data is kept.
        let dto = query
            .branches_run("latest", None, "example.Decide", "latest", 10_000, 0)
            .unwrap();
        let mut unknown_labels = Vec::new();
        for outcome in dto["ir_outcomes"].as_array().into_iter().flatten() {
            if let Some(id) = outcome["id"].as_str() {
                if matches!(
                    outcome["status"].as_str(),
                    Some("UNKNOWN" | "TIMEOUT" | "UNSUPPORTED")
                ) {
                    unknown_labels.push(id.to_owned());
                }
            }
        }
        for label in &unknown_labels {
            assert!(big.text.contains(label), "mandatory unknown {label} kept");
        }
    }

    #[test]
    fn build_function_context_scopes_real_graph_and_fails_closed() {
        let (_temp, query) = analyzed(false);
        // The page describes `example.Decide` only; `Query` requires the
        // qualified name and errors closed for an unknown one.
        let ctx = build_function_context(&query, "latest", None, "example.Decide", 4096).unwrap();
        assert!(
            ctx.text.contains("function example.Decide"),
            "resolved scope: {}",
            ctx.text
        );
        // Real fixture has another function (example.Other): its rules are
        // excluded from this function's context by the structural scope.
        assert!(
            !ctx.text.contains("example.Other"),
            "{} was excluded",
            ctx.text
        );
        // An unknown function fails closed inside Query (no panics, no stubs).
        let err = build_function_context(&query, "latest", None, "no.such.function", 4096);
        assert!(err.is_err());
        // Integrity holds across the whole fixture flow.
        assert!(query.db_check().is_ok());
    }
}
