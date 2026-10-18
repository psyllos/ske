//! Optional LLM enrichment (§7.4, AC-0.7-07/08/09).
//!
//! The LLM is never required: without configuration, no transport is invoked
//! and no network request is made (AC-0.7-07). A request carries only
//! structured facts by default; raw source excerpts are added only when
//! `allow_source_upload` AND a recorded consent exist (AC-0.7-09). Rules
//! produced here are persisted with `origin='LLM_INFERRED'`,
//! `validation_status='UNVALIDATED'`, plus `prompt_version`, `llm_model` and
//! `model_parameters` (AC-0.7-08).
//!
//! [`crate::llm_http::HttpTransport`] implements optional OpenAI-compatible
//! HTTPS requests. Scripted transports are unit-test utilities, not HTTP mocks.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ske_common::{Error, Result};

pub const DEFAULT_PROMPT_VERSION: &str = "ske-v0.7-context-v1";

/// Structured facts sent to the model. No source text unless enabled.
#[derive(Clone, Debug, Serialize)]
pub struct Facts {
    /// Query-rendered deterministic knowledge; intentionally not constructible
    /// or deserializable by callers (no free-form source smuggling channel).
    items: Vec<String>,
    /// Optional source excerpts, only present when explicitly allowed.
    #[serde(default)]
    pub source_snippets: Vec<String>,
}

impl Facts {
    pub fn items(&self) -> &[String] {
        &self.items
    }
    pub fn contains_source(&self) -> bool {
        !self.source_snippets.is_empty()
    }
    /// Crate-internal constructor used by the HTTP transport tests. External
    /// callers cannot smuggle arbitrary text into structured facts.
    #[cfg(test)]
    pub(crate) fn from_items(items: Vec<String>) -> Self {
        Self {
            items,
            source_snippets: Vec::new(),
        }
    }
}

/// A candidate rule produced by the model, carrying provenance fields.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LlmRule {
    pub short_name: String,
    pub description: String,
    /// References to structural facts that justified the description.
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub llm_model: String,
    #[serde(default)]
    pub model_parameters: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmResponse {
    pub rules: Vec<LlmRule>,
}

/// A request envelope matching the OpenAI-compatible chat/completions shape
/// (subset sufficient for the tests and the documented contract).
pub struct LlmRequest {
    pub base_url: String,
    pub model: String,
    pub prompt_version: String,
    pub model_parameters: Value,
    pub(crate) facts: Facts,
}

impl LlmRequest {
    pub fn facts(&self) -> &Facts {
        &self.facts
    }
}

/// Wire transport for the optional LLM. Implementations must be injectable so
/// tests do not require network.
pub trait LlmTransport {
    fn send(&self, request: &LlmRequest) -> Result<LlmResponse>;
}

/// Scripted mock transport shaped like an OpenAI-compatible endpoint.
/// Returns one rule per struct fact item. Used by tests and by default in
/// offline validation; never calls the network.
#[derive(Clone, Debug, Default)]
pub struct ScriptedTransport {
    pub response_rules: Vec<LlmRule>,
}

impl ScriptedTransport {
    pub fn from_facts(facts: &Facts) -> Self {
        let rules = facts
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| LlmRule {
                short_name: format!("llm-rule-{i}"),
                description: item.clone(),
                evidence_refs: Vec::new(),
                llm_model: "mock-openai".to_string(),
                model_parameters: "{}".to_string(),
            })
            .collect();
        Self {
            response_rules: rules,
        }
    }
}

impl LlmTransport for ScriptedTransport {
    fn send(&self, _request: &LlmRequest) -> Result<LlmResponse> {
        Ok(LlmResponse {
            rules: self.response_rules.clone(),
        })
    }
}

/// Optional LLM client. `enabled=false` (default) means no transport is ever
/// called (AC-0.7-07).
#[derive(Clone)]
pub struct LlmClient {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub prompt_version: String,
    pub model_parameters: Value,
    pub allow_source_upload: bool,
    /// Consent flag persisted by the caller (e.g. `.ske/consent.json`).
    pub source_upload_consent: bool,
    pub consent_path: Option<std::path::PathBuf>,
    transport: Option<std::sync::Arc<dyn LlmTransport + Send + Sync>>,
}

impl Default for LlmClient {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::new(),
            model: String::new(),
            prompt_version: DEFAULT_PROMPT_VERSION.to_string(),
            model_parameters: json!({}),
            allow_source_upload: false,
            source_upload_consent: false,
            consent_path: None,
            transport: None,
        }
    }
}

impl LlmClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach a scripted transport (tests / offline mock).
    pub fn with_scripted(mut self, transport: ScriptedTransport) -> Self {
        self.transport =
            Some(std::sync::Arc::new(transport) as std::sync::Arc<dyn LlmTransport + Send + Sync>);
        self
    }

    /// Attach any transport behind an `Arc` (recording mocks in tests).
    pub fn with_transport(
        mut self,
        transport: std::sync::Arc<dyn LlmTransport + Send + Sync>,
    ) -> Self {
        self.transport = Some(transport);
        self
    }

    /// Set the model name (test helper).
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Enable enrichment (explicit configuration).
    pub fn enabled(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// Set source-upload permission and recorded consent (test helper).
    pub fn with_source_upload(mut self, allow: bool, consent: bool) -> Self {
        self.allow_source_upload = allow;
        self.source_upload_consent = consent;
        self
    }

    /// Whether any call is possible at all.
    pub fn can_enrich(&self) -> bool {
        self.enabled && self.transport.is_some() && !self.model.is_empty()
    }

    /// Build the request body. Source snippets are excluded unless both
    /// `allow_source_upload` and a recorded consent exist (AC-0.7-09).
    fn build_request(&self, facts: Facts, source_snippets: Vec<String>) -> Result<LlmRequest> {
        if !self.enabled {
            return Err(Error::new(
                "E_USAGE",
                "LLM enrichment is disabled; no configuration supplied",
            ));
        }
        let include_source =
            self.allow_source_upload && self.source_upload_consent && !source_snippets.is_empty();
        let mut facts = facts;
        // Never trust snippets embedded in caller-supplied Facts. All source
        // must pass through the explicit upload gate, including reused facts.
        facts.source_snippets = if include_source {
            source_snippets
        } else {
            Vec::new()
        };
        Ok(LlmRequest {
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            prompt_version: self.prompt_version.clone(),
            model_parameters: self.model_parameters.clone(),
            facts,
        })
    }

    /// Run the optional enrichment over structured facts. Never called when
    /// `enabled=false` (AC-0.7-07 "no network request").
    fn enrich(&self, request: &LlmRequest) -> Result<LlmResponse> {
        if !self.can_enrich() {
            return Err(Error::new(
                "E_USAGE",
                "LLM enrichment is disabled; no configuration supplied",
            ));
        }
        if request.facts.contains_source()
            && !(self.allow_source_upload && self.source_upload_consent)
        {
            return Err(Error::new(
                "E_USAGE",
                "source upload requires explicit permission and recorded consent",
            ));
        }
        let transport = self.transport.as_ref().unwrap();
        transport.send(request)
    }

    /// Full enrichment pipeline over a `ske-query` database (AC-0.7-08/09).
    ///
    /// 1. Reads the snapshot's deterministic rules (with evidence) and renders
    ///    them as structured facts (no source text unless `allow_source_upload`).
    /// 2. Calls the configured transport (mock in tests).
    /// 3. Persists each LLM rule through `Query::insert_llm_rules` with
    ///    `origin='LLM_INFERRED'`, `validation_status='UNVALIDATED'`, plus
    ///    `prompt_version`, `llm_model` and `model_parameters`.
    ///
    /// Deterministic rules are never modified. Returns the persisted rule rows.
    pub fn enrich_rules(
        &self,
        query: &ske_query::Query,
        snapshot: &str,
        repository: Option<&str>,
        source_snippets: Vec<String>,
    ) -> Result<serde_json::Value> {
        let rules = query.list_rules(snapshot, repository, true, 10_000, 0)?;
        let sid = rules["snapshot_id"]
            .as_i64()
            .ok_or_else(|| Error::new("E_DB_INTEGRITY", "missing snapshot"))?;
        let scoped =
            crate::llm_consent::Consent::scope(&self.base_url, sid, repository, &source_snippets);
        let mut client = self.clone();
        client.source_upload_consent = self.allow_source_upload
            && self
                .consent_path
                .as_ref()
                .is_some_and(|path| scoped.matches_record(path));
        let rules = rules["rules"].as_array().cloned().unwrap_or_default();
        let facts = Facts {
            items: rules
                .iter()
                .filter(|r| r["origin"] != "LLM_INFERRED")
                .map(|r| {
                    json!({
                        "rule_key": r["rule_key"],
                        "description": r["description"],
                        "origin": r["origin"],
                        "validation_status": r["validation_status"],
                        "evidence": r.get("evidence").cloned().unwrap_or(json!([])),
                    })
                    .to_string()
                })
                .collect(),
            source_snippets: Vec::new(),
        };
        let request = client.build_request(facts, source_snippets)?;
        let response = client.enrich(&request)?;
        let prompt_version = request.prompt_version.clone();
        let inserts: Vec<ske_query::LlmRuleInsert> = response
            .rules
            .into_iter()
            .map(|rule| ske_query::LlmRuleInsert {
                rule_key: rule.short_name.clone(),
                short_name: rule.short_name,
                description: rule.description,
                evidence_refs: rule.evidence_refs,
                llm_model: request.model.clone(),
                model_parameters: request.model_parameters.to_string(),
            })
            .collect();
        query.insert_llm_rules(snapshot, repository, &inserts, &prompt_version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_no_source_upload_rejects_embedded_or_mutated_snippets() {
        let client = LlmClient::new()
            .enabled()
            .with_model("mock")
            .with_scripted(ScriptedTransport::default());
        let mut request = client
            .build_request(
                Facts {
                    items: vec!["structured fact".into()],
                    source_snippets: vec!["embedded source".into()],
                },
                vec!["explicit source".into()],
            )
            .unwrap();
        assert!(!request.facts.contains_source());
        // Public request fields can be modified after construction. Enforce
        // the gate again immediately before dispatching to a transport.
        request.facts.source_snippets.push("mutated source".into());
        assert_eq!(client.enrich(&request).unwrap_err().code, "E_USAGE");
    }

    #[test]
    fn no_llm_default_never_calls_transport() {
        // A client with default config is disabled: enrich is refused, no
        // transport exists and nothing is sent.
        let client = LlmClient::new();
        assert!(!client.can_enrich());
        let err = client.enrich(&LlmRequest {
            base_url: String::new(),
            model: String::new(),
            prompt_version: DEFAULT_PROMPT_VERSION.to_string(),
            model_parameters: json!({}),
            facts: Facts {
                items: vec!["anything".into()],
                source_snippets: vec![],
            },
        });
        assert!(err.is_err());
    }

    #[test]
    fn llm_no_source_upload_excludes_snippets() {
        let client = LlmClient::new()
            .enabled()
            .with_scripted(ScriptedTransport::default())
            .with_source_upload(false, false);
        let request = client
            .build_request(
                Facts {
                    items: vec!["rule: age < 18".into()],
                    source_snippets: vec![],
                },
                vec!["func Decide... source".into()],
            )
            .unwrap();
        assert!(!request.facts.contains_source());
    }

    #[test]
    fn llm_provenance_fields_recorded() {
        let facts = Facts {
            items: vec!["rule: age < 18 -> underage".into()],
            source_snippets: vec![],
        };
        let transport = ScriptedTransport::from_facts(&facts);
        let client = LlmClient::new()
            .enabled()
            .with_scripted(transport)
            .with_model("gpt-mock");
        let request = client.build_request(facts, vec![]).unwrap();
        let response = client.enrich(&request).unwrap();
        assert_eq!(response.rules.len(), 1);
        assert_eq!(response.rules[0].llm_model, "mock-openai");
        assert!(!response.rules[0].model_parameters.is_empty());
    }
}
