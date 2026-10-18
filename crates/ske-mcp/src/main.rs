//! Standalone MCP stdio server (`ske-mcp-server`): serves the five SKE tools
//! over newline-delimited JSON-RPC 2.0 on stdin/stdout against a database.
use ske_query::Query;
use std::path::PathBuf;

fn main() {
    let db = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("knowledge.sqlite"));
    let query = match Query::open(&db) {
        Ok(query) => query,
        Err(error) => {
            eprintln!("ske-mcp-server: {error}");
            std::process::exit(error.exit_code());
        }
    };
    if let Err(error) = ske_mcp::serve_stdio(&query) {
        // A parse error reaches the client as a JSON-RPC error; a broken pipe
        // at EOF is a normal shutdown, not a failure.
        if error.code != "E_IO" {
            eprintln!("ske-mcp-server: {error}");
            std::process::exit(error.exit_code());
        }
    }
}
