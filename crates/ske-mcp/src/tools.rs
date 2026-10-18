//! The five MCP tools (AC-0.7-03) with hand-authored JSON input/output
//! schemas (no added dependency). `rules` for `Check` returns the 3 §v0.5
//! rules with their evidence.
use serde_json::{json, Value};
use ske_common::Result;
use ske_query::Query;

#[derive(Clone)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub output_schema: Value,
}

/// The MCP tools exposed exactly as: `inspect`, `branches`, `explain`,
/// `rules`, `impact` (AC-0.7-03). Each carries JSON input/output schemas.
pub fn list_tools() -> Vec<ToolSpec> {
    let mut tools = vec![
        ToolSpec {
            name: "inspect",
            description: "Inspect a snapshot: units, inventory status and diagnostics summary.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "snapshot": {"type": "string", "description": "snapshot id or latest"},
                    "repository": {"type": ["string", "null"]}
                },
                "additionalProperties": false
            }),
            output_schema: json!({"type": "object"}),
        },
        ToolSpec {
            name: "branches",
            description: "List source and IR decisions/outcomes of a function, optionally with the latest run status.",
            input_schema: json!({
                "type": "object",
                "required": ["function"],
                "properties": {
                    "snapshot": {"type": "string", "description": "snapshot id or latest"},
                    "repository": {"type": ["string", "null"]},
                    "function": {"type": "string"},
                    "limit": {"type": "integer", "default": 100},
                    "offset": {"type": "integer", "default": 0}
                },
                "additionalProperties": false
            }),
            output_schema: json!({"type": "object"}),
        },
        ToolSpec {
            name: "explain",
            description: "Explain one source outcome: status, run assumptions, arrivals with SMT-LIB text and verifier verdict.",
            input_schema: json!({
                "type": "object",
                "required": ["outcome"],
                "properties": {
                    "snapshot": {"type": "string", "description": "snapshot id or latest"},
                    "repository": {"type": ["string", "null"]},
                    "outcome": {"type": "string", "description": "source outcome id, e.g. example.Decide/sd1/o0"},
                    "limit": {"type": "integer", "default": 100},
                    "offset": {"type": "integer", "default": 0}
                },
                "additionalProperties": false
            }),
            output_schema: json!({"type": "object"}),
        },
        ToolSpec {
            name: "rules",
            description: "List functional rules, optionally with their evidence.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "snapshot": {"type": "string", "description": "snapshot id or latest"},
                    "repository": {"type": ["string", "null"]},
                    "with_evidence": {"type": "boolean", "default": false},
                    "limit": {"type": "integer", "default": 100},
                    "offset": {"type": "integer", "default": 0}
                },
                "additionalProperties": false
            }),
            output_schema: json!({"type": "object"}),
        },
        ToolSpec {
            name: "impact",
            description: "Conservative impact of changing a function: reverse callers, with incomplete marking when the call graph is not closed.",
            input_schema: json!({
                "type": "object",
                "required": ["function"],
                "properties": {
                    "snapshot": {"type": "string", "description": "snapshot id or latest"},
                    "repository": {"type": ["string", "null"]},
                    "function": {"type": "string"}
                },
                "additionalProperties": false
            }),
            output_schema: json!({"type": "object"}),
        },
    ];
    for tool in &mut tools {
        let properties = tool.input_schema["properties"].as_object_mut().unwrap();
        if properties.contains_key("limit") {
            properties.insert(
                "limit".into(),
                json!({"type":"integer","minimum":1,"maximum":10000,"default":100}),
            );
            properties.insert(
                "offset".into(),
                json!({"type":"integer","minimum":0,"default":0}),
            );
        }
        match tool.name {
            "branches" => {
                properties.insert("run".into(), json!({"type":["string","null"]}));
            }
            "rules" => {
                properties.insert("function".into(), json!({"type":["string","null"]}));
                properties.insert(
                    "with_evidence".into(),
                    json!({"type":"boolean","default":true}),
                );
            }
            "impact" => {
                properties.insert(
                    "limit".into(),
                    json!({"type":"integer","minimum":1,"maximum":10000,"default":100}),
                );
                properties.insert(
                    "offset".into(),
                    json!({"type":"integer","minimum":0,"default":0}),
                );
            }
            _ => {}
        }
        tool.output_schema = output_schema(tool.name);
    }
    tools
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct InspectInput {
    #[serde(default = "default_snapshot")]
    snapshot: String,
    #[serde(default)]
    repository: Option<String>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BranchesInput {
    #[serde(default = "default_snapshot")]
    snapshot: String,
    #[serde(default)]
    repository: Option<String>,
    function: String,
    #[serde(default)]
    run: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default = "default_offset")]
    offset: usize,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ExplainInput {
    #[serde(default = "default_snapshot")]
    snapshot: String,
    #[serde(default)]
    repository: Option<String>,
    outcome: String,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default = "default_offset")]
    offset: usize,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RulesInput {
    #[serde(default = "default_snapshot")]
    snapshot: String,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default = "default_true")]
    with_evidence: bool,
    #[serde(default)]
    function: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default = "default_offset")]
    offset: usize,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ImpactInput {
    #[serde(default = "default_snapshot")]
    snapshot: String,
    #[serde(default)]
    repository: Option<String>,
    function: String,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_true() -> bool {
    true
}

fn validate_page(limit: usize) -> Result<()> {
    if !(1..=10_000).contains(&limit) {
        return Err(ske_common::Error::new("E_USAGE", "limit must be 1..10000"));
    }
    Ok(())
}

fn default_snapshot() -> String {
    "latest".to_string()
}
fn default_limit() -> usize {
    100
}
fn default_offset() -> usize {
    0
}

fn output_schema(name: &str) -> Value {
    let page = json!({
        "type":"object", "required":["limit","offset","total","truncated"],
        "properties":{
            "limit":{"type":"integer","minimum":0},
            "offset":{"type":"integer","minimum":0},
            "total":{"type":"integer","minimum":0},
            "truncated":{"type":"boolean"}
        }
    });
    let evidence = json!({"type":"array","items":{
        "type":"object","required":["kind"],
        "properties":{"kind":{"type":"string"},"id":{},"relation":{"type":"string"},
            "span":{"type":"string"},"proof":{"type":"object"}}
    }});
    let mut properties = json!({"page":page});
    let required = match name {
        "inspect" => {
            properties["snapshot"] = json!({"type":"object","required":["id","inventory_status"],
                "properties":{"id":{"type":"integer"},"inventory_status":{"type":"string"}}});
            properties["units"] = json!({"type":"array","items":{"type":"object",
                "required":["unit_key","language","inventory_status","counts"],
                "properties":{"unit_key":{"type":"string"},"language":{"type":"string"},
                    "inventory_status":{"type":"string"},"counts":{"type":"object"}}}});
            properties["analysis_defaults"] = json!({"type":"object"});
            properties["diagnostics_summary"] = json!({"type":"object"});
            vec![
                "snapshot",
                "units",
                "analysis_defaults",
                "diagnostics_summary",
                "page",
            ]
        }
        "branches" => {
            properties["snapshot_id"] = json!({"type":"integer"});
            properties["function"] = json!({"type":"string"});
            properties["inventory"] = json!({"type":"object"});
            properties["analysis"] = json!({"type":["object","null"]});
            properties["source_decisions"] = json!({"type":"array","items":{"type":"object",
                "required":["id","outcomes"],"properties":{"id":{"type":"string"},
                    "outcomes":{"type":"array","items":{"type":"object","required":["id","status"],
                        "properties":{"id":{"type":"string"},"status":{"type":"string"}}}}}}});
            properties["ir_outcomes"] = json!({"type":"array","items":{"type":"object",
                "required":["id","status","evidence"],"properties":{"id":{"type":"string"},
                    "status":{"type":"string"},"evidence":evidence}}});
            vec![
                "snapshot_id",
                "function",
                "inventory",
                "analysis",
                "source_decisions",
                "ir_outcomes",
                "page",
            ]
        }
        "explain" => {
            properties["source_outcome"] = json!({"type":"string"});
            properties["status"] = json!({"type":"string"});
            properties["run"] = json!({"type":"object","required":["run_id","run_status"],
                "properties":{"run_id":{"type":"integer"},"run_status":{"type":"string"}}});
            properties["assumptions"] = json!({"type":"array","items":{"type":"string"}});
            properties["ir_outcomes"] = json!({"type":"array","items":{"type":"object",
                "required":["status","arrivals"],"properties":{"status":{"type":"string"},
                "arrivals":{"type":"array","items":{"type":"object",
                    "required":["result","smtlib"],"properties":{"result":{"type":"string"},
                    "smtlib":{"type":["string","null"]}}}},
                "proof":{"type":"object","required":["verdict","verification_level"],
                    "properties":{"verdict":{"enum":["ACCEPTED","REJECTED"]},
                        "verification_level":{"type":"string"}}}}}});
            vec![
                "source_outcome",
                "status",
                "run",
                "assumptions",
                "ir_outcomes",
                "page",
            ]
        }
        "rules" => {
            properties["snapshot_id"] = json!({"type":"integer"});
            properties["rules"] = json!({"type":"array","items":{"type":"object",
                "required":["rule_key","description","origin","validation_status"],
                "properties":{"rule_key":{"type":"string"},"description":{"type":"string"},
                    "origin":{"enum":["DETERMINISTIC","ABSTRACT_DERIVED","LLM_INFERRED"]},
                    "validation_status":{"type":"string"},"evidence":evidence}}});
            vec!["snapshot_id", "rules", "page"]
        }
        "impact" => {
            properties["function"] = json!({"type":"string"});
            properties["incomplete"] = json!({"type":"boolean"});
            properties["incomplete_sites"] = json!({"type":"array","items":{"type":"object"}});
            properties["callers"] = json!({"type":"array","items":{"type":"object",
                "required":["qualified_name","unit_key"],"properties":{
                    "qualified_name":{"type":"string"},"unit_key":{"type":"string"}}}});
            vec![
                "function",
                "callers",
                "incomplete",
                "incomplete_sites",
                "page",
            ]
        }
        _ => unreachable!("fixed tool catalog"),
    };
    for (field, schema) in [
        ("truncated", json!({"type":"boolean"})),
        ("truncation_reason", json!({"type":["string","null"]})),
        ("visited_count", json!({"type":"integer","minimum":0})),
        ("frontier_count", json!({"type":"integer","minimum":0})),
    ] {
        if matches!(name, "explain" | "impact") {
            properties[field] = schema;
        }
    }
    json!({"$schema":"https://json-schema.org/draft/2020-12/schema",
        "type":"object","required":required,"properties":properties})
}

/// Invoke one tool by name with an argument object. `Query` is passed so the
/// tools stay one step below the CLI (`ske` owns the database path).
pub fn call_tool(query: &Query, name: &str, args: &Value) -> Result<Value> {
    fn parse<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T> {
        serde_json::from_value(value.clone())
            .map_err(|e| ske_common::Error::new("E_USAGE", format!("invalid tool arguments: {e}")))
    }
    match name {
        "inspect" => {
            let input: InspectInput = parse(args)?;
            let mut result = query.inspect(&input.snapshot, input.repository.as_deref())?;
            let total = result["units"].as_array().map_or(0, Vec::len);
            result["page"] = json!({"limit":total,"offset":0,"total":total,"truncated":false});
            Ok(result)
        }
        "branches" => {
            let input: BranchesInput = parse(args)?;
            validate_page(input.limit)?;
            if let Some(run) = input.run {
                return query.branches_run(
                    &input.snapshot,
                    input.repository.as_deref(),
                    &input.function,
                    &run,
                    input.limit,
                    input.offset,
                );
            }
            query.branches_page(
                &input.snapshot,
                input.repository.as_deref(),
                &input.function,
                input.limit,
                input.offset,
            )
        }
        "explain" => {
            let input: ExplainInput = parse(args)?;
            validate_page(input.limit)?;
            let mut result = query.explain(
                &input.snapshot,
                input.repository.as_deref(),
                &input.outcome,
                input.limit,
                input.offset,
            )?;
            report_paging(&mut result, "ir_outcomes", input.limit, input.offset);
            Ok(result)
        }
        "rules" => {
            let input: RulesInput = parse(args)?;
            validate_page(input.limit)?;
            if let Some(function) = input.function {
                return query.list_function_rules(
                    &input.snapshot,
                    input.repository.as_deref(),
                    &function,
                    input.with_evidence,
                    input.limit,
                    input.offset,
                );
            }
            query.list_rules(
                &input.snapshot,
                input.repository.as_deref(),
                input.with_evidence,
                input.limit,
                input.offset,
            )
        }
        "impact" => {
            let input: ImpactInput = parse(args)?;
            validate_page(input.limit)?;
            let mut result = query.impact(
                &input.snapshot,
                input.repository.as_deref(),
                &input.function,
            )?;
            let callers = result["callers"].as_array().cloned().unwrap_or_default();
            let total = callers.len();
            result["callers"] = json!(callers
                .into_iter()
                .skip(input.offset)
                .take(input.limit)
                .collect::<Vec<_>>());
            result["page"] = json!({"limit":input.limit,"offset":input.offset,
                "total":total,"truncated":input.offset.saturating_add(input.limit)<total});
            report_paging(&mut result, "callers", input.limit, input.offset);
            Ok(result)
        }
        other => Err(ske_common::Error::new(
            "E_USAGE",
            format!("unknown MCP tool {other}"),
        )),
    }
}

fn report_paging(result: &mut Value, field: &str, _limit: usize, offset: usize) {
    let total = result["page"]["total"].as_u64().unwrap_or(0);
    let emitted = result[field].as_array().map_or(0, Vec::len) as u64;
    let truncated = emitted < total || offset > 0;
    result["truncated"] = json!(truncated);
    result["truncation_reason"] = if truncated {
        json!("PAGE_LIMIT")
    } else {
        Value::Null
    };
    result["visited_count"] = json!(total);
    // Traversal is complete here; this frontier is the known output not
    // included in this page, not a claim about unknown dynamic call targets.
    result["frontier_count"] = json!(total.saturating_sub(emitted));
}
