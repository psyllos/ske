mod common;
use common::Harness;
#[test]
fn json_stdout_is_one_document_on_success_and_errors() {
    let h = Harness::new();
    h.ok(&["init"]);
    h.ok(&["db", "check"]);
    for args in [
        vec!["unknown-command"],
        vec!["functions", "cfg"],
        vec!["inspect", "--limit", "10001"],
        vec!["ingest", "cir", "--input", "/nonexistent/file.cir.pb"],
    ] {
        let (exit, value) = h.run(&args);
        assert_ne!(exit, 0);
        assert!(value["error"]["code"].is_string());
        assert!(value["error"]["message"].is_string());
        assert!(value["error"]["candidates"].is_array());
    }
}
