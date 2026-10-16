//! Strict effective Go build profiles; canonical JSON is the wire representation.
use crate::{err, Result};
use serde_json::{json, Value};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub const MAX_PROFILE_BYTES: usize = 1024 * 1024;
// A trusted Go tool may need cold-start time under concurrent validation.
// This is separate from the adapter's execution budget and remains bounded.
const TARGET_QUERY_TIMEOUT: Duration = Duration::from_secs(30);

/// A cap on bytes read, not merely a metadata check (files may grow).
pub fn read_profile(path: &Path) -> Result<String> {
    if !path.is_file() {
        return Err(err("E_USAGE", "profile must be a regular file"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| {
            file.take(MAX_PROFILE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|e| err("E_USAGE", e.to_string()))?;
    if bytes.len() > MAX_PROFILE_BYTES {
        return Err(err("E_USAGE", "profile exceeds 1 MiB"));
    }
    String::from_utf8(bytes).map_err(|e| err("E_USAGE", e.to_string()))
}

fn targets(command: Command) -> Result<Vec<u8>> {
    targets_with_timeout(command, TARGET_QUERY_TIMEOUT)
}

fn targets_with_timeout(mut command: Command, timeout: Duration) -> Result<Vec<u8>> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| err("E_USAGE", format!("go tool dist list: {e}")))?;
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.take(64 * 1024 + 1).read_to_end(&mut bytes);
        let _ = tx.send(result.map(|_| bytes));
    });
    let start = Instant::now();
    let result = (|| {
        let bytes = rx
            .recv_timeout(timeout)
            .map_err(|_| err("E_USAGE", "go tool dist list timeout"))?
            .map_err(|e| err("E_USAGE", e.to_string()))?;
        if bytes.len() > 64 * 1024 {
            return Err(err("E_USAGE", "go tool dist list output exceeds 64 KiB"));
        }
        loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|e| err("E_USAGE", e.to_string()))?
            {
                return if status.success() {
                    Ok(bytes)
                } else {
                    Err(err("E_USAGE", "go tool dist list failed"))
                };
            }
            if start.elapsed() >= timeout {
                return Err(err("E_USAGE", "go tool dist list timeout"));
            }
            thread::sleep(Duration::from_millis(5));
        }
    })();
    // Kill descendants too: a child may exit leaving stdout inherited by a helper.
    #[cfg(unix)]
    let _ = Command::new("/bin/kill")
        .args(["-KILL", "--", &format!("-{}", child.id())])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
    let _ = child.wait();
    let _ = reader.join();
    result
}

pub fn normalize(input: Option<&str>) -> Result<String> {
    if input.is_some_and(|input| input.len() > MAX_PROFILE_BYTES) {
        return Err(err("E_USAGE", "profile exceeds 1 MiB"));
    }
    let mut profile = json!({
        "profile_version":1,"language":"go","go_version":null,"goos":"linux",
        "goarch":"amd64","tags":[],"include_tests":false,"cgo":false,
        "patterns":["./..."],"go_work":null
    });
    if let Some(input) = input {
        let supplied: Value =
            serde_json::from_str(input).map_err(|e| err("E_USAGE", e.to_string()))?;
        let object = supplied
            .as_object()
            .ok_or_else(|| err("E_USAGE", "profile must be an object"))?;
        for (key, value) in object {
            if !profile.as_object().unwrap().contains_key(key) {
                return Err(err("E_USAGE", format!("unknown profile key: {key}")));
            }
            profile[key] = value.clone();
        }
    }
    if profile["profile_version"].as_u64() != Some(1)
        || profile["language"] != "go"
        || !profile["include_tests"].is_boolean()
        || !profile["cgo"].is_boolean()
    {
        return Err(err("E_USAGE", "invalid profile version/language/boolean"));
    }
    for name in ["tags", "patterns"] {
        let array = profile[name]
            .as_array()
            .ok_or_else(|| err("E_USAGE", format!("{name} must be an array")))?;
        if (name == "patterns" && array.is_empty())
            || array.iter().any(|v| v.as_str().is_none_or(str::is_empty))
        {
            return Err(err(
                "E_USAGE",
                format!("{name} must contain nonempty strings"),
            ));
        }
    }
    for name in ["go_version", "go_work"] {
        if !profile[name].is_null() && profile[name].as_str().is_none_or(str::is_empty) {
            return Err(err(
                "E_USAGE",
                format!("{name} must be null or a nonempty string"),
            ));
        }
    }
    let os = profile["goos"]
        .as_str()
        .ok_or_else(|| err("E_USAGE", "goos must be a string"))?;
    let arch = profile["goarch"]
        .as_str()
        .ok_or_else(|| err("E_USAGE", "goarch must be a string"))?;
    let mut command = Command::new("go");
    command
        .args(["tool", "dist", "list"])
        .current_dir("/")
        .env_clear()
        .stdin(Stdio::null());
    for key in ["PATH", "HOME", "GOROOT", "GOCACHE"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
        .env("GOENV", "off")
        .env("GOTOOLCHAIN", "local")
        .env("GOPROXY", "off")
        .env("GOFLAGS", "");
    let output = targets(command)?;
    let target = format!("{os}/{arch}");
    if !String::from_utf8_lossy(&output)
        .lines()
        .any(|line| line == target)
    {
        return Err(err("E_USAGE", "goos/goarch absent from go tool dist list"));
    }
    if let Some(version) = profile["go_version"].as_str() {
        let version = version.strip_prefix("go").unwrap_or(version).to_owned();
        profile["go_version"] = json!(version);
    }
    ske_common::canonical_json(&profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_tool_timeout_kills_descendant_holding_stdout() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 30 & wait"]);
        let start = Instant::now();
        assert!(targets_with_timeout(command, Duration::from_millis(250))
            .unwrap_err()
            .message
            .contains("timeout"));
        assert!(start.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn profile_tool_respects_the_given_deadline() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 0.1; printf 'linux/amd64\\n'"]);
        assert_eq!(
            targets_with_timeout(command, Duration::from_secs(2)).unwrap(),
            b"linux/amd64\n"
        );
    }
    #[test]
    fn profile_bytes_and_tool_output_are_bounded() {
        assert_eq!(
            normalize(Some(&" ".repeat(MAX_PROFILE_BYTES + 1)))
                .unwrap_err()
                .code,
            "E_USAGE"
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile");
        std::fs::write(&path, vec![b' '; MAX_PROFILE_BYTES + 1]).unwrap();
        assert_eq!(read_profile(&path).unwrap_err().code, "E_USAGE");
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "dd if=/dev/zero bs=65536 count=2 2>/dev/null"]);
        assert!(targets(command).unwrap_err().message.contains("64 KiB"));
    }
    #[test]
    fn profile_rejects_unknown_and_wrong_types_before_tool_execution() {
        for text in [
            r#"{"oops":1}"#,
            r#"{"patterns":[]}"#,
            r#"{"include_tests":null}"#,
            r#"{"go_work":3}"#,
            r#"{"tags":[3]}"#,
        ] {
            assert_eq!(normalize(Some(text)).unwrap_err().code, "E_USAGE");
        }
    }
    #[test]
    fn profile_defaults_and_canonical_order() {
        let a = normalize(None).unwrap();
        let b = normalize(Some(r#"{"language":"go","profile_version":1}"#)).unwrap();
        assert_eq!(a, b);
        assert!(a.starts_with(r#"{"cgo":false,"go_version":null,"go_work":null,"goarch":"amd64""#));
        assert_eq!(
            normalize(Some(r#"{"goos":"bogus"}"#)).unwrap_err().code,
            "E_USAGE"
        );
    }
}
