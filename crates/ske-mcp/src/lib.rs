//! MCP tools for SKE v0.7: `inspect`, `branches`, `explain`, `rules`,
//! `impact` with JSON input/output schemas, plus a stdio JSON-RPC 2.0 server
//! (local MCP transport). Depends only on `ske-common` + `ske-query`.

pub mod protocol;
pub mod tools;

pub use protocol::{serve_stdio, JsonRpcRequest, JsonRpcResponse};
pub use tools::{call_tool, list_tools};
