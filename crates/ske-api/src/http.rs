//! Small loopback-only HTTP/1.1 JSON endpoint. One bounded request per connection;
//! no transfer encoding, pipelining, remote binding, or per-request database paths.
use crate::{
    service::{clamp_page, paginate, DEFAULT_LIMIT},
    ApiService,
};
use serde::Deserialize;
use serde_json::{json, Value};
use ske_common::{Error, Result};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::{Duration, Instant},
};

pub const MAX_BODY: usize = 64 * 1024;
const MAX_HEADER: usize = 8192;
const DEADLINE: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    operation: String,
    #[serde(default = "latest")]
    snapshot: String,
    repository: Option<String>,
    function: Option<String>,
    source: Option<String>,
    target: Option<String>,
    rule_key: Option<String>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    max_depth: Option<usize>,
    max_nodes: Option<usize>,
    max_paths: Option<usize>,
    #[serde(default)]
    include_external: bool,
    #[serde(default)]
    with_evidence: bool,
}
fn latest() -> String {
    "latest".into()
}
fn usage(message: impl Into<String>) -> Error {
    Error::new("E_USAGE", message)
}
fn needed(value: &Option<String>, key: &str) -> Result<String> {
    value.clone().ok_or_else(|| usage(format!("missing {key}")))
}

pub fn dispatch(api: &ApiService, body: &[u8]) -> Result<Value> {
    let r: Request = serde_json::from_slice(body).map_err(|e| usage(e.to_string()))?;
    let (limit, offset) = clamp_page(r.limit.unwrap_or(DEFAULT_LIMIT), r.offset.unwrap_or(0));
    let repo = r.repository.as_deref();
    let mut value = match r.operation.as_str() {
        "inspect_snapshot" => api.inspect_snapshot(&r.snapshot, repo)?,
        "list_functions" => {
            api.list_functions(&r.snapshot, repo, r.include_external, limit, offset)?
        }
        "list_branches" => api.list_branches(
            &r.snapshot,
            repo,
            &needed(&r.function, "function")?,
            limit,
            offset,
        )?,
        "get_function_cfg" => {
            api.get_function_cfg(&r.snapshot, repo, &needed(&r.function, "function")?)?
        }
        "explore_paths" => api.explore_paths(
            &r.snapshot,
            repo,
            &needed(&r.source, "source")?,
            &needed(&r.target, "target")?,
            r.max_depth,
            r.max_paths,
            r.max_nodes,
            limit,
            offset,
        )?,
        "get_function_summary" => {
            api.get_function_summary(&r.snapshot, repo, &needed(&r.function, "function")?)?
        }
        "reconstruct_behaviors" => api.reconstruct_behaviors(
            &r.snapshot,
            repo,
            &needed(&r.function, "function")?,
            limit,
            offset,
        )?,
        "list_rules" => api.list_rules(&r.snapshot, repo, r.with_evidence, limit, offset)?,
        "get_rule_evidence" => {
            api.get_rule_evidence(&r.snapshot, repo, &needed(&r.rule_key, "rule_key")?)?
        }
        "compare_snapshots" => api.compare_snapshots(
            &needed(&r.repository, "repository")?,
            &needed(&r.from, "from")?,
            &needed(&r.to, "to")?,
        )?,
        _ => return Err(usage("unknown operation")),
    };
    for key in ["summaries", "changes"] {
        if value[key].is_array() {
            paginate(&mut value, key, limit, offset);
        }
    }
    Ok(value)
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let invalid = || {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid or oversized HTTP request",
        )
    };
    let start = Instant::now();
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= MAX_HEADER || start.elapsed() >= DEADLINE {
            return Err(invalid());
        }
        stream.set_read_timeout(Some(DEADLINE.saturating_sub(start.elapsed())))?;
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        header.push(byte[0]);
    }
    let header = std::str::from_utf8(&header).map_err(|_| invalid())?;
    let mut lines = header.split("\r\n");
    if lines.next() != Some("POST /v1/query HTTP/1.1") {
        return Err(invalid());
    }
    let mut length = None;
    let mut content_type = false;
    for line in lines.filter(|s| !s.is_empty()) {
        let (name, value) = line.split_once(':').ok_or_else(invalid)?;
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(invalid());
        }
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err(invalid());
            }
            length = Some(value.trim().parse::<usize>().map_err(|_| invalid())?);
        }
        if name.eq_ignore_ascii_case("content-type") {
            content_type = value.trim() == "application/json";
        }
    }
    let length = length.filter(|n| *n <= MAX_BODY).ok_or_else(invalid)?;
    if !content_type {
        return Err(invalid());
    }
    let mut body = vec![0; length];
    let mut read = 0;
    while read < length {
        if start.elapsed() >= DEADLINE {
            return Err(invalid());
        }
        stream.set_read_timeout(Some(DEADLINE.saturating_sub(start.elapsed())))?;
        let n = stream.read(&mut body[read..])?;
        if n == 0 {
            return Err(invalid());
        }
        read += n;
    }
    Ok(body)
}

/// Handle one connection; useful for a bounded embedding and real transport tests.
pub fn serve_one(listener: &TcpListener, api: &ApiService) -> std::io::Result<()> {
    if !listener.local_addr()?.ip().is_loopback() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "loopback only",
        ));
    }
    let (mut stream, _) = listener.accept()?;
    stream.set_write_timeout(Some(DEADLINE))?;
    let (status, value) = match read_request(&mut stream) {
        Ok(body) => match dispatch(api, &body) {
            Ok(value) => ("200 OK", value),
            Err(e) => ("400 Bad Request", json!({"error":e.to_string()})),
        },
        Err(_) => (
            "400 Bad Request",
            json!({"error":"malformed, oversized or timed out request"}),
        ),
    };
    let body = serde_json::to_vec(&value)?;
    if body.len() > 16 * 1024 * 1024 {
        return stream.write_all(
            b"HTTP/1.1 413 Content Too Large\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
    }
    write!(stream,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len())?;
    stream.write_all(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open() -> (ApiService, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (ApiService::open(dir.path().join("k.sqlite")).unwrap(), dir)
    }

    #[test]
    fn dispatch_rejects_unknown_operation() {
        let (api, _dir) = open();
        let err = dispatch(&api, br#"{"operation":"nope"}"#).unwrap_err();
        assert_eq!(err.code, "E_USAGE");
    }

    #[test]
    fn dispatch_requires_mandatory_fields() {
        let (api, _dir) = open();
        let err = dispatch(
            &api,
            br#"{"operation":"list_branches","snapshot":"latest"}"#,
        )
        .unwrap_err();
        assert_eq!(err.code, "E_USAGE");
        let err = dispatch(&api, br#"{"operation":"compare_snapshots"}"#).unwrap_err();
        assert_eq!(err.code, "E_USAGE");
    }

    #[test]
    fn dispatch_rejects_unknown_fields_and_bad_json() {
        let (api, _dir) = open();
        let err =
            dispatch(&api, br#"{"operation":"inspect_snapshot","unknown":true}"#).unwrap_err();
        assert_eq!(err.code, "E_USAGE");
        let err = dispatch(&api, b"{").unwrap_err();
        assert_eq!(err.code, "E_USAGE");
    }

    #[test]
    fn dispatch_clamps_oversized_limit() {
        let (api, _dir) = open();
        // The oversized limit is accepted at the HTTP boundary (clamped to the
        // query maximum) rather than rejected as malformed JSON; the empty DB
        // then fails on the missing repository, proving the request parsed and
        // reached the service layer (not a body/limit parse error).
        let err = dispatch(
            &api,
            br#"{"operation":"list_functions","limit":99999999,"offset":0}"#,
        )
        .unwrap_err();
        assert_eq!(err.code, "E_NOT_FOUND", "unexpected: {err:?}");
        // Direct clamp contract (service layer) is covered separately by the
        // api_limits unit tests.
        assert_eq!(clamp_page(99_999_999, 0), (10_000, 0));
    }
}
