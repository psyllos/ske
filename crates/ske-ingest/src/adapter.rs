//! Bounded adapter IPC. One request, stdin EOF, framed responses and real child reaping.
use crate::{err, Result};
use prost::Message;
use ske_cir::{adapter_proto::*, proto::Record};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

pub use ske_cir::adapter_proto::Limits;

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            max_message_bytes: 16 * 1024 * 1024,
            max_messages: 50_000_000,
            timeout_ms: 600_000,
            max_memory_mb: 4096,
        }
    }
}

/// Enforced independently of the limits sent to an untrusted child.
#[derive(Clone, Debug)]
pub struct RunLimits {
    pub max_message_bytes: u64,
    pub max_messages: u64,
    pub timeout_ms: u64,
    pub max_memory_mb: u64,
}
impl RunLimits {
    fn wire(&self) -> Limits {
        Limits {
            max_message_bytes: self.max_message_bytes,
            max_messages: self.max_messages,
            timeout_ms: self.timeout_ms,
            max_memory_mb: self.max_memory_mb,
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.max_message_bytes == 0
            || self.max_message_bytes > u32::MAX as u64
            || self.max_messages == 0
            || self.timeout_ms == 0
            || self.max_memory_mb == 0
            || self.max_memory_mb.checked_mul(1024 * 1024).is_none()
        {
            return Err(err(
                "E_USAGE",
                "adapter limits must be positive; message size must fit u32",
            ));
        }
        Ok(())
    }
}

fn terminate(child: &mut Child) {
    // The dedicated process group also contains go list and any descendants.
    #[cfg(unix)]
    {
        let _ = Command::new("/bin/kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

struct Process {
    child: Arc<Mutex<Child>>,
    finished: Arc<AtomicBool>,
    exceeded: Arc<AtomicBool>,
    monitor: Option<thread::JoinHandle<()>>,
    stderr: Option<thread::JoinHandle<Vec<u8>>>,
}
impl Drop for Process {
    fn drop(&mut self) {
        self.finished.store(true, Ordering::SeqCst);
        if let Some(handle) = self.monitor.take() {
            let _ = handle.join();
        }
        if let Ok(mut child) = self.child.lock() {
            terminate(&mut child);
            let _ = child.wait();
        }
        if let Some(handle) = self.stderr.take() {
            let _ = handle.join();
        }
    }
}

pub struct Responses {
    stdout: std::process::ChildStdout,
    count: u64,
    limits: RunLimits,
    eof: bool,
}
impl Responses {
    fn read_exact(&mut self, bytes: &mut [u8]) -> Result<()> {
        self.stdout.read_exact(bytes).map_err(|e| {
            self.eof |= e.kind() == std::io::ErrorKind::UnexpectedEof;
            err("E_PROTO_DECODE", e.to_string())
        })
    }

    fn next(&mut self) -> Result<Option<adapter_response::Body>> {
        let mut prefix = [0; 4];
        if self
            .stdout
            .read(&mut prefix[..1])
            .map_err(|e| err("E_PROTO_DECODE", e.to_string()))?
            == 0
        {
            self.eof = true;
            return Ok(None);
        }
        self.read_exact(&mut prefix[1..])?;
        self.count += 1;
        let size = u32::from_be_bytes(prefix) as usize;
        if size as u64 > self.limits.max_message_bytes || self.count > self.limits.max_messages {
            return Err(err(
                "E_ADAPTER_LIMIT",
                "adapter message size/count limit exceeded",
            ));
        }
        if size == 0 {
            return Err(err("E_PROTO_DECODE", "empty adapter frame"));
        }
        let mut body = vec![0; size];
        self.read_exact(&mut body)?;
        let response = AdapterResponse::decode(body.as_slice())
            .map_err(|e| err("E_PROTO_DECODE", e.to_string()))?;
        match response.body {
            Some(adapter_response::Body::Error(e)) => {
                let code = ske_common::ErrorCode::from_code(&e.code)
                    .map_or("E_ADAPTER_LIMIT", |c| c.as_str());
                Err(err(code, format!("{}: {}", e.code, e.message)))
            }
            None => Err(err("E_PROTO_DECODE", "adapter response without body")),
            other => Ok(other),
        }
    }
}

fn run<T>(
    executable: &Path,
    request: AdapterRequest,
    limits: &RunLimits,
    consume: impl FnOnce(&mut Responses) -> Result<T>,
) -> Result<T> {
    limits.validate()?;
    let body = request.encode_to_vec();
    if body.len() as u64 > limits.max_message_bytes {
        return Err(err(
            "E_ADAPTER_LIMIT",
            "adapter request exceeds message limit",
        ));
    }
    // Linux prlimit applies a hard address-space ceiling before exec, not a
    // best-effort RSS sample after a potentially unbounded allocation.
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = Command::new("prlimit");
        let bytes = limits
            .max_memory_mb
            .checked_mul(1024 * 1024)
            .ok_or_else(|| err("E_USAGE", "memory limit overflow"))?;
        command
            .arg(format!("--as={bytes}"))
            .arg("--")
            .arg(executable);
        command
    };
    #[cfg(not(target_os = "linux"))]
    let mut command = Command::new(executable);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.env_clear();
    for key in ["PATH", "HOME", "GOCACHE", "GOMODCACHE", "GOROOT"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
        .env("GOENV", "off")
        .env("GOEXPERIMENT", "")
        .env("GOFLAGS", "-mod=readonly")
        .env("GOTOOLCHAIN", "local")
        .env("GOPROXY", "off")
        .env("GONOSUMDB", "*")
        .env("CGO_ENABLED", "0")
        .env("GOWORK", "off");
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| err("E_ADAPTER_LIMIT", format!("cannot start adapter: {e}")))?;
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let child = Arc::new(Mutex::new(child));
    let finished = Arc::new(AtomicBool::new(false));
    let exceeded = Arc::new(AtomicBool::new(false));
    let mut process = Process {
        child: child.clone(),
        finished: finished.clone(),
        exceeded: exceeded.clone(),
        monitor: None,
        stderr: None,
    };
    let timeout = Duration::from_millis(limits.timeout_ms);
    let memory_kb = limits.max_memory_mb.saturating_mul(1024);
    process.monitor = Some(thread::spawn(move || {
        let started = Instant::now();
        while !finished.load(Ordering::SeqCst) {
            let pid = child.lock().unwrap().id();
            let rss = Command::new("/bin/ps")
                .args(["-axo", "pgid=,rss="])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| {
                    String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .filter_map(|line| {
                            let mut fields = line.split_whitespace();
                            let group = fields.next()?.parse::<u32>().ok()?;
                            let rss = fields.next()?.parse::<u64>().ok()?;
                            (group == pid).then_some(rss)
                        })
                        .fold(0_u64, u64::saturating_add)
                })
                .unwrap_or(u64::MAX);
            if started.elapsed() >= timeout || rss > memory_kb {
                exceeded.store(true, Ordering::SeqCst);
                terminate(&mut child.lock().unwrap());
                break;
            }
            // Process-table scans are relatively expensive. A busy test runner (or
            // many concurrent ingests) must not become a ps fork storm.
            thread::sleep(Duration::from_millis(100));
        }
    }));
    process.stderr = Some(thread::spawn(move || {
        let mut saved = Vec::new();
        let mut bytes = [0; 4096];
        while let Ok(n) = stderr.read(&mut bytes) {
            if n == 0 {
                break;
            }
            let keep = n.min(64 * 1024 - saved.len());
            saved.extend_from_slice(&bytes[..keep]);
        }
        saved
    }));
    let write = stdin
        .write_all(&(body.len() as u32).to_be_bytes())
        .and_then(|()| stdin.write_all(&body));
    drop(stdin);
    write.map_err(|e| err("E_ADAPTER_LIMIT", e.to_string()))?;
    let mut responses = Responses {
        stdout,
        count: 0,
        limits: limits.clone(),
        eof: false,
    };
    let value = consume(&mut responses);
    // EOF can be caused by a hard allocation failure (Linux RLIMIT_AS), not
    // malformed IPC. Reap the child below before classifying an incomplete
    // response, even when stdout closes just before the process exits.
    // Other protocol errors still terminate promptly without waiting for EOF.
    if value.is_err() && !responses.eof {
        if process.exceeded.load(Ordering::SeqCst) {
            return Err(err(
                "E_ADAPTER_LIMIT",
                "adapter time/memory monitoring limit exceeded",
            ));
        }
        return value;
    }
    // Never hold the child mutex in wait(): the watchdog must be able to kill it.
    let status = loop {
        if let Some(status) = process
            .child
            .lock()
            .unwrap()
            .try_wait()
            .map_err(|e| err("E_ADAPTER_LIMIT", e.to_string()))?
        {
            break status;
        }
        thread::sleep(Duration::from_millis(5));
    };
    if process.exceeded.load(Ordering::SeqCst) {
        return Err(err(
            "E_ADAPTER_LIMIT",
            "adapter time/memory monitoring limit exceeded",
        ));
    }
    if !status.success() {
        return Err(err(
            "E_ADAPTER_LIMIT",
            format!("adapter exited with {status}"),
        ));
    }
    value
}

// Package import paths are the primary sort key (§9.3). Comparing the raw
// unit key would put "example/child@..." before "example@..." because '/' < '@'.
fn discovery_order(key: &str) -> (&str, &str) {
    key.rsplit_once('@').unwrap_or((key, ""))
}

pub fn list_units(
    executable: &Path,
    root: &Path,
    profile: &str,
    limits: &RunLimits,
) -> Result<Vec<UnitDescriptor>> {
    list_units_for(crate::Language::Go, executable, root, profile, limits)
}

pub fn list_units_for(
    language: crate::Language,
    executable: &Path,
    root: &Path,
    profile: &str,
    limits: &RunLimits,
) -> Result<Vec<UnitDescriptor>> {
    run(
        executable,
        AdapterRequest {
            protocol_major: 1,
            body: Some(adapter_request::Body::ListUnits(ListUnitsRequest {
                project_root: root.to_string_lossy().into(),
                build_profile_json: profile.into(),
                limits: Some(limits.wire()),
            })),
        },
        limits,
        |responses| {
            let mut units = Vec::new();
            let mut descriptor_bytes = 0_usize;
            let mut done = false;
            while let Some(body) = responses.next()? {
                match body {
                    adapter_response::Body::Unit(unit) if !done => {
                        descriptor_bytes = descriptor_bytes.saturating_add(unit.encoded_len());
                        if descriptor_bytes > 16 * 1024 * 1024 {
                            return Err(err("E_ADAPTER_LIMIT", "discovery exceeds 16 MiB"));
                        }
                        if unit.unit_key.is_empty() {
                            return Err(err(
                                "E_RECORD_ORDER",
                                "unit descriptors must have a nonempty unit key",
                            ));
                        }
                        if unit.language != language.as_str() {
                            return Err(err(
                                "E_TYPE_MISMATCH",
                                "unit descriptor language differs from selected ingest language",
                            ));
                        }
                        if units.last().is_some_and(|last: &UnitDescriptor| {
                            discovery_order(&last.unit_key) >= discovery_order(&unit.unit_key)
                        }) {
                            return Err(err(
                                "E_RECORD_ORDER",
                                "unit descriptors must be unique and sorted by package import path",
                            ));
                        }
                        // Discovery is small, but not unbounded even with a hostile adapter.
                        if units.len() >= 100_000 {
                            return Err(err("E_ADAPTER_LIMIT", "too many units"));
                        }
                        units.push(unit);
                    }
                    adapter_response::Body::ListDone(_) if !done => done = true,
                    _ => return Err(err("E_RECORD_ORDER", "invalid list_units response order")),
                }
            }
            if !done {
                return Err(err("E_RECORD_ORDER", "missing list_done"));
            }
            Ok(units)
        },
    )
}

pub fn extract_unit<T>(
    executable: &Path,
    root: &Path,
    profile: &str,
    key: &str,
    limits: &RunLimits,
    consume: impl FnOnce(&mut dyn FnMut() -> Result<Option<Record>>) -> Result<T>,
) -> Result<T> {
    run(
        executable,
        AdapterRequest {
            protocol_major: 1,
            body: Some(adapter_request::Body::ExtractUnit(ExtractUnitRequest {
                project_root: root.to_string_lossy().into(),
                build_profile_json: profile.into(),
                unit_key: key.into(),
                limits: Some(limits.wire()),
            })),
        },
        limits,
        |responses| {
            let value = consume(&mut || match responses.next()? {
                Some(adapter_response::Body::Record(record)) => Ok(Some(record)),
                None => Ok(None),
                _ => Err(err("E_RECORD_ORDER", "non-record extract_unit response")),
            })?;
            if responses.next()?.is_some() {
                return Err(err("E_RECORD_ORDER", "unconsumed extract_unit response"));
            }
            Ok(value)
        },
    )
}
