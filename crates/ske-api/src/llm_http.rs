//! Optional OpenAI-compatible HTTPS client; no redirects or implicit retries.
use crate::llm::{LlmRequest, LlmResponse, LlmTransport};
use serde_json::{json, Value};
use ske_common::{Error, Result};

pub struct HttpTransport {
    agent: ureq::Agent,
    api_key: Option<String>,
}

impl HttpTransport {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .redirects(0)
                .timeout(std::time::Duration::from_secs(30))
                .build(),
            api_key,
        }
    }
}

impl LlmTransport for HttpTransport {
    fn send(&self, request: &LlmRequest) -> Result<LlmResponse> {
        if !(request.base_url.starts_with("https://")
            || request.base_url.starts_with("http://127.0.0.1:"))
        {
            return Err(Error::new("E_USAGE", "HTTPS required except IPv4 loopback"));
        }
        let mut parameters = request
            .model_parameters
            .as_object()
            .cloned()
            .ok_or_else(|| Error::new("E_USAGE", "model parameters must be an object"))?;
        // Reserved fields cannot be replaced by caller parameters.
        parameters.insert("model".into(), json!(request.model));
        parameters.insert("messages".into(), json!([
            {"role":"system","content":format!("{}: Return JSON {{\"rules\":[{{\"short_name\":\"...\",\"description\":\"...\",\"evidence_refs\":[\"deterministic rule key\"]}}]}}. Hypotheses only, never proofs.", request.prompt_version)},
            {"role":"user","content":serde_json::to_string(&request.facts).unwrap()}
        ]));
        let mut call = self.agent.post(&request.base_url);
        if let Some(key) = &self.api_key {
            call = call.set("Authorization", &format!("Bearer {key}"));
        }
        let response = call
            .send_json(Value::Object(parameters))
            .map_err(|_| Error::new("E_LLM", "LLM HTTP request failed"))?;
        let body: Value = serde_json::from_reader(response.into_reader().take(1_048_576))
            .map_err(|_| Error::new("E_LLM", "invalid completion response"))?;
        serde_json::from_str(
            body["choices"][0]["message"]["content"]
                .as_str()
                .ok_or_else(|| Error::new("E_LLM", "missing completion content"))?,
        )
        .map_err(|_| Error::new("E_LLM", "invalid rule JSON"))
    }
}

use std::io::Read;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::Facts;

    fn request(base_url: &str) -> LlmRequest {
        LlmRequest {
            base_url: base_url.into(),
            model: "mock".into(),
            prompt_version: "ske-v0.7-context-v1".into(),
            model_parameters: json!({}),
            facts: Facts::from_items(vec!["rule: age < 18".into()]),
        }
    }

    #[test]
    fn reject_non_https_and_remote_http_without_network_call() {
        let transport = HttpTransport::new(None);
        // A plain remote http:// endpoint is refused: never reaches ureq.
        let err = transport
            .send(&request("http://example.com/chat"))
            .unwrap_err();
        assert_eq!(err.code, "E_USAGE");
        // A non-URL string is refused as well.
        let err = transport.send(&request("not a url")).unwrap_err();
        assert_eq!(err.code, "E_USAGE");
        // Loopback HTTP is allowed by policy (no network fetcher is attached;
        // the transport cannot complete a request to an unbound port, but the
        // policy check itself passes and the error then comes from ureq).
        let err = transport
            .send(&request("http://127.0.0.1:1/chat"))
            .unwrap_err();
        assert_ne!(err.code, "E_USAGE");
    }
}
