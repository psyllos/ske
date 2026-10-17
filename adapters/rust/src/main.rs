use ske_adapter_rust::{
    compiler::{Compiler, Error},
    frame, lower,
    profile::Profile,
    source, VERSION,
};
use ske_cir::adapter_proto::{
    self as a, adapter_request::Body as Request, adapter_response::Body as Response,
};
use std::{io, path::PathBuf};

fn error(code: &'static str, message: impl Into<String>) -> Error {
    Error {
        code,
        message: message.into(),
    }
}

fn run() -> Result<(), Error> {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("{VERSION}");
        return Ok(());
    }
    let request: a::AdapterRequest =
        frame::read(&mut io::stdin().lock()).map_err(|e| error("FRAME_INVALID", e))?;
    if request.protocol_major != 1 {
        return Err(error("PROTOCOL_MISMATCH", "protocol_major must be 1"));
    }
    let (root, json, extract_key, limits) = match request.body {
        Some(Request::ListUnits(r)) => (r.project_root, r.build_profile_json, None, r.limits),
        Some(Request::ExtractUnit(r)) => (
            r.project_root,
            r.build_profile_json,
            Some(r.unit_key),
            r.limits,
        ),
        None => return Err(error("REQUEST_INVALID", "request body missing")),
    };
    let profile = Profile::parse(&json).map_err(|e| error("BUILD_PROFILE_INVALID", e))?;
    let compiler = Compiler::discover()?;
    let raw_root = PathBuf::from(root);
    if !raw_root.is_absolute() {
        return Err(error("SOURCE_INVALID", "project_root must be absolute"));
    }
    let root = raw_root
        .canonicalize()
        .map_err(|e| error("SOURCE_INVALID", e.to_string()))?;
    if root.join("build.rs").exists() {
        return Err(error(
            "RUST_BUILD_SCRIPT_UNSUPPORTED",
            "build.rs is never executed; supply a source-only root without a build script",
        ));
    }
    let entry = root
        .join(&profile.entry)
        .canonicalize()
        .map_err(|e| error("SOURCE_INVALID", e.to_string()))?;
    if !entry.starts_with(&root) {
        return Err(error("SOURCE_INVALID", "entry escapes project root"));
    }
    let bytes = std::fs::read(&entry).map_err(|e| error("SOURCE_INVALID", e.to_string()))?;
    if bytes.len() > frame::LIMIT {
        return Err(error("SOURCE_INVALID", "entry exceeds source size limit"));
    }
    let text = String::from_utf8(bytes).map_err(|e| error("SOURCE_INVALID", e.to_string()))?;
    let inventory = source::inventory(&profile.entry, &text)
        .map_err(|e| error("RUST_SOURCE_UNSUPPORTED", e))?;
    let key = format!("rust:{}", profile.entry);
    let mut output = io::stdout().lock();
    let max_bytes = limits
        .as_ref()
        .map(|l| l.max_message_bytes as usize)
        .filter(|n| *n != 0)
        .unwrap_or(frame::LIMIT);
    let max_messages = limits
        .as_ref()
        .map(|l| l.max_messages)
        .filter(|n| *n != 0)
        .unwrap_or(50_000_000);
    let mut send = |body| {
        frame::write(
            &mut output,
            &a::AdapterResponse { body: Some(body) },
            max_bytes,
        )
        .map_err(|e| error("FRAME_WRITE_FAILED", e.to_string()))
    };
    if let Some(requested) = extract_key {
        if requested != key {
            return Err(error("UNIT_NOT_FOUND", requested));
        }
        let timeout = limits
            .as_ref()
            .map(|l| l.timeout_ms)
            .filter(|t| *t != 0)
            .unwrap_or(600_000);
        let mir =
            compiler.mir_source(&text, &profile, std::time::Duration::from_millis(timeout))?;
        let records = lower::extract(&compiler, &profile, &inventory, &mir, &text)
            .map_err(|e| error("MIR_UNSUPPORTED", e))?;
        // Buffer/validate the entire lowering before writing Header: an unsupported
        // terminator can never masquerade as an apparently successful partial unit.
        if records.len() as u64 > max_messages {
            return Err(error("LIMIT_EXCEEDED", "record count limit"));
        }
        for record in records {
            send(Response::Record(record))?;
        }
    } else {
        if max_messages < 2 {
            return Err(error("LIMIT_EXCEEDED", "ListUnits requires two responses"));
        }
        send(Response::Unit(a::UnitDescriptor {
            unit_key: key,
            language: "rust".into(),
            source_paths: vec![profile.entry],
        }))?;
        send(Response::ListDone(a::ListUnitsDone {}))?;
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("AdapterError{{code:{:?},message:{:?}}}", e.code, e.message);
        let _ = frame::write(
            &mut io::stdout().lock(),
            &a::AdapterResponse {
                body: Some(Response::Error(a::AdapterError {
                    code: e.code.into(),
                    message: e.message,
                })),
            },
            frame::LIMIT,
        );
        std::process::exit(if e.code == "TOOLCHAIN_MISMATCH" { 3 } else { 1 });
    }
}
