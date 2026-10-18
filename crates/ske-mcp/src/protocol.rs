//! Minimal MCP JSON-RPC 2.0 transport over stdio (AC-0.7-03/04/05).
//!
//! No remote service is required: the local MCP server reads newline-delimited
//! JSON-RPC requests from `stdin` and writes responses to `stdout`. The `Query`
//! is provided by the embedding CLI (`ske`), so this crate never opens SQLite
//! itself.
use crate::tools::{call_tool, list_tools};
use serde_json::{json, Value};
use ske_common::Result;
use ske_query::Query;
use std::io::{BufRead, Read, Write};

pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, serde::Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: Option<String>,
    #[serde(default)]
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

fn error(id: Value, code: i64, message: String, data: Option<Value>) -> JsonRpcResponse {
    let mut err = json!({ "code": code, "message": message });
    if let Some(data) = data {
        err["data"] = data;
    }
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(err),
    }
}

/// Handle a single JSON-RPC request against the MCP tool registry.
pub fn handle(query: &Query, request: &JsonRpcRequest) -> JsonRpcResponse {
    let id = request.id.clone();
    if request.jsonrpc.as_deref() != Some("2.0") {
        return error(
            id,
            -32600,
            "invalid request: jsonrpc must be 2.0".into(),
            None,
        );
    }
    match request.method.as_str() {
        "initialize" => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({
                "protocolVersion": "2025-06-18",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "ske-mcp", "version": env!("CARGO_PKG_VERSION") }
            })),
            error: None,
        },
        "ping" => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({})),
            error: None,
        },
        "tools/list" => {
            let tools: Vec<Value> = list_tools()
                .into_iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.input_schema,
                        "outputSchema": t.output_schema
                    })
                })
                .collect();
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(json!({ "tools": tools })),
                error: None,
            }
        }
        "tools/call" => {
            let name = request.params["name"].as_str().unwrap_or("");
            let args = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if name.is_empty() {
                return error(id, -32602, "tools/call requires name".into(), None);
            }
            match call_tool(query, name, &args) {
                Ok(result) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(json!({
                        "content": [{"type": "text", "text": result.to_string()}],
                        "structuredContent": result,
                        "isError": false,
                    })),
                    error: None,
                },
                Err(e) if e.code == "E_USAGE" => {
                    error(id, -32602, e.message, Some(json!({"code": e.code})))
                }
                Err(e) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(json!({
                        "content": [{"type":"text", "text": e.to_string()}],
                        "isError": true,
                    })),
                    error: None,
                },
            }
        }
        // Requests with no tools/list contract are rejected rather than guessed.
        other => error(id, -32601, format!("method not found: {other}"), None),
    }
}

/// Serve newline-delimited JSON-RPC on stdin/stdout until EOF.
pub fn serve_stdio(query: &Query) -> Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    serve(query, stdin.lock(), &mut out)
}

/// Bounded newline-delimited JSON-RPC. Responses are flushed per request:
/// an interactive host must not need to close stdin to receive its result.
pub fn serve(query: &Query, mut input: impl BufRead, out: &mut impl Write) -> Result<()> {
    let io_error = |e: std::io::Error| ske_common::Error::new("E_INTERNAL", e.to_string());
    loop {
        let mut line = Vec::new();
        let count = input
            .by_ref()
            .take((MAX_MESSAGE_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)
            .map_err(io_error)?;
        if count == 0 {
            return out.flush().map_err(io_error);
        }
        if count > MAX_MESSAGE_BYTES {
            write_response(
                out,
                &error(Value::Null, -32600, "message too large".into(), None),
            )?;
            // Stop rather than treating the tail of an oversized message as
            // another independent request.
            return Err(ske_common::Error::new(
                "E_USAGE",
                "MCP message exceeds 1 MiB",
            ));
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let value: Value = match serde_json::from_slice(&line) {
            Ok(value) => value,
            Err(_) => {
                write_response(out, &error(Value::Null, -32700, "parse error".into(), None))?;
                continue;
            }
        };
        let request: JsonRpcRequest = match serde_json::from_value(value.clone()) {
            Ok(request) if value.is_object() => request,
            _ => {
                write_response(
                    out,
                    &error(Value::Null, -32600, "invalid request".into(), None),
                )?;
                continue;
            }
        };
        if value.get("id").is_none() {
            // JSON-RPC notifications (including notifications/initialized)
            // never get a response and cannot invoke read tools accidentally.
            continue;
        }
        if !(request.id.is_null() || request.id.is_string() || request.id.is_number()) {
            write_response(
                out,
                &error(Value::Null, -32600, "invalid request id".into(), None),
            )?;
            continue;
        }
        let response = handle(query, &request);
        write_response(out, &response)?;
    }
}

fn write_response(out: &mut impl Write, response: &JsonRpcResponse) -> Result<()> {
    let io_error = |e: std::io::Error| ske_common::Error::new("E_INTERNAL", e.to_string());
    serde_json::to_writer(&mut *out, response)
        .map_err(|e| ske_common::Error::new("E_INTERNAL", e.to_string()))?;
    out.write_all(b"\n").map_err(io_error)?;
    out.flush().map_err(io_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(id: Value, method: &str, params: Value) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: Some("2.0".into()),
            id,
            method: method.into(),
            params,
        }
    }

    #[test]
    fn mcp_tools_list_exposes_five_tools() {
        // Uses the handler against a null-id request; opening a Query is not
        // needed for tools/list. We still need a Query for the handler; use a
        // temporary database (empty) to construct one.
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("k.sqlite");
        let query = Query::open(&db).unwrap();
        query.init().unwrap();
        let response = handle(&query, &request(json!(1), "tools/list", json!({})));
        let result = response.result.unwrap();
        let names: Vec<&str> = result["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec!["inspect", "branches", "explain", "rules", "impact"]
        );
        assert!(result["tools"][0]["inputSchema"].is_object());
        assert!(result["tools"][0]["outputSchema"].is_object());
    }

    #[test]
    fn protocol_rejects_unknown_method() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("k.sqlite");
        let query = Query::open(&db).unwrap();
        query.init().unwrap();
        let response = handle(&query, &request(json!(2), "unknown/method", json!({})));
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap()["code"], -32601);
    }
}
