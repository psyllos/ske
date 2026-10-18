mod go_support;
use go_support::*;
use ske_query::Query;
use std::sync::{Arc, Mutex};

fn lab_with_check_rules() -> (Lab, Query) {
    let lab = Lab::new();
    let q = Query::open(&lab.db).unwrap();
    q.reconstruct_behaviors("latest", None, "example.Check", 100, 0)
        .unwrap();
    (lab, q)
}

fn setup_client(captured: Arc<Mutex<Vec<String>>>) -> ske_api::llm::LlmClient {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut bytes = Vec::new();
        let end = loop {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            bytes.push(byte[0]);
            if bytes.ends_with(b"\r\n\r\n") {
                break bytes.len();
            }
        };
        let headers = String::from_utf8(bytes.clone()).unwrap();
        assert!(headers.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"));
        let length: usize = headers
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().unwrap())
            })
            .unwrap();
        bytes.resize(end + length, 0);
        socket.read_exact(&mut bytes[end..]).unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes[end..]).unwrap();
        assert_eq!(body["model"], "mock-openai");
        let facts: serde_json::Value =
            serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
        let items = facts["items"].as_array().unwrap();
        let has_source = !facts["source_snippets"].as_array().unwrap().is_empty();
        captured.lock().unwrap().push(format!(
            "{has_source}|{}",
            items
                .iter()
                .map(|x| x.as_str().unwrap())
                .collect::<Vec<_>>()
                .join("\n")
        ));
        let fact: serde_json::Value = serde_json::from_str(items[0].as_str().unwrap()).unwrap();
        let content = serde_json::json!({"rules":[{
            "short_name":"llm-derived-extra-rule","description":"generated hypothesis",
            "evidence_refs":[fact["rule_key"]], "llm_model":"FORGED","model_parameters":"FORGED"
        }]})
        .to_string();
        let reply =
            serde_json::json!({"choices":[{"message":{"role":"assistant","content":content}}]})
                .to_string();
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", reply.len(), reply).unwrap();
    });
    let mut client = ske_api::llm::LlmClient::new()
        .enabled()
        .with_model("mock-openai")
        .with_transport(Arc::new(ske_api::llm_http::HttpTransport::new(None)));
    client.base_url = endpoint;
    client
}

#[test]
fn no_llm_default_behaviors_reconstruct_works_without_config() {
    let lab = Lab::new();
    // The default client is disabled and never reaches the network.
    let client = ske_api::llm::LlmClient::new();
    assert!(!client.can_enrich());
    // Reconstructing behaviors needs no LLM configuration at all.
    // Verify the OS denial in the same process before exec; the restriction
    // survives exec into the actual CLI (not a substitute library call).
    let script = r#"
import ctypes, errno, os, platform, socket, sys
if sys.platform == 'linux':
    numbers = {'x86_64': (41,42), 'aarch64': (198,203)}
    denied = numbers[platform.machine()]
    class Filter(ctypes.Structure):
        _fields_ = [('code',ctypes.c_ushort),('jt',ctypes.c_ubyte),('jf',ctypes.c_ubyte),('k',ctypes.c_uint)]
    class Program(ctypes.Structure):
        _fields_ = [('len',ctypes.c_ushort),('filter',ctypes.POINTER(Filter))]
    # Load syscall number; deny socket/connect; allow everything else.
    code = (Filter * 6)(Filter(0x20,0,0,0),Filter(0x15,2,0,denied[0]),Filter(0x15,1,0,denied[1]),Filter(0x06,0,0,0x7fff0000),Filter(0x06,0,0,0x50000|errno.EPERM),Filter(0x06,0,0,0x7fff0000))
    libc = ctypes.CDLL(None, use_errno=True)
    assert libc.prctl(38,1,0,0,0) == 0
    assert libc.prctl(22,2,ctypes.byref(Program(6,code)),0,0) == 0
try:
    s = socket.socket()
    s.connect(('127.0.0.1',9))
except OSError as e:
    assert e.errno in (errno.EPERM, errno.EACCES), e
else:
    raise AssertionError('network was not blocked')
os.execv(sys.argv[1],sys.argv[1:])
"#;
    let mut command = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("/usr/bin/sandbox-exec");
        c.args(["-p", "(version 1)(allow default)(deny network*)", "python3"]);
        c
    } else if cfg!(target_os = "linux") {
        std::process::Command::new("python3")
    } else {
        // The network isolation techniques are macOS/Linux specific.
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        compile_error!("network isolation test supports macOS/Linux only");
        std::process::Command::new("python3")
    };
    let result = command
        .args([
            "-c",
            script,
            env!("CARGO_BIN_EXE_ske"),
            "behaviors",
            "reconstruct",
            "--entry",
            "example.Check",
            "--db",
        ])
        .arg(&lab.db)
        .args(["--format", "json"])
        .env_remove("SKE_LLM_API_KEY")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let out: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(out["cases"].as_array().unwrap().len(), 3);
}

#[test]
fn llm_mock_provenance_records_separate_unvalidated_rules() {
    let (_lab, q) = lab_with_check_rules();
    // Snapshot the deterministic rule rows (hashes before enrichment).
    let before = q.list_rules("latest", None, true, 100, 0).unwrap()["rules"].clone();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let client = setup_client(captured.clone());
    client.enrich_rules(&q, "latest", None, Vec::new()).unwrap();
    // The LLM rule is persisted with full provenance and stays unvalidated.
    let after = q.list_rules("latest", None, true, 100, 0).unwrap();
    let rules = after["rules"].as_array().unwrap();
    let llm_rule = rules
        .iter()
        .find(|r| r["origin"] == "LLM_INFERRED")
        .expect("LLM rule persisted");
    assert_eq!(llm_rule["validation_status"], "UNVALIDATED");
    assert_eq!(llm_rule["prompt_version"], "ske-v0.7-context-v1");
    assert_eq!(llm_rule["llm_model"], "mock-openai");
    assert!(llm_rule["model_parameters"].is_string());
    // Deterministic rules are byte-for-byte unchanged.
    let before_rules = before.as_array().unwrap();
    let after_deterministic: Vec<_> = rules
        .iter()
        .filter(|r| r["origin"] != "LLM_INFERRED")
        .collect();
    assert_eq!(before_rules.len(), after_deterministic.len());
    for (b, a) in before_rules.iter().zip(after_deterministic) {
        assert_eq!(
            serde_json::to_string(b).unwrap(),
            serde_json::to_string(a).unwrap()
        );
    }
    // prop_no_llm_proof: no LLM rule may be referenced as a proof. LLM rules
    // are persisted without a behavior case (NULL behavior_case_id), so they
    // can never be attached to a deterministic derivation or its proof.
    let conn = rusqlite::Connection::open(&_lab.db).unwrap();
    let with_case: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM functional_rules WHERE origin='LLM_INFERRED' AND behavior_case_id IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(with_case, 0);
    // And no proof record ever references an LLM rule description/key.
    let proof_refs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM proof_records WHERE reason LIKE '%LLM_INFERRED%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(proof_refs, 0);
}

#[test]
fn llm_no_source_upload_body_has_structured_facts_only() {
    let (_lab, q) = lab_with_check_rules();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let client = setup_client(captured.clone());
    // No --allow-source-upload, no consent: source snippets must be excluded.
    client
        .with_source_upload(false, false)
        .enrich_rules(&q, "latest", None, vec!["func Decide() { ... }".into()])
        .unwrap();
    let guard = captured.lock().unwrap();
    assert_eq!(guard.len(), 1);
    let (has_source, items) = guard[0].split_once('|').unwrap();
    assert_eq!(has_source, "false");
    assert!(items.contains("age < 18"));
    assert!(!items.contains("func Decide"));
}

#[test]
fn llm_source_upload_requires_explicit_consent() {
    let (_lab, q) = lab_with_check_rules();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let client = setup_client(captured.clone());
    // allow_source_upload=true but NO recorded consent: still excluded.
    client
        .with_source_upload(true, false)
        .enrich_rules(&q, "latest", None, vec!["only if consented".into()])
        .unwrap();
    let guard = captured.lock().unwrap();
    let (has_source, _items) = guard[0].split_once('|').unwrap();
    assert_eq!(has_source, "false");
}
