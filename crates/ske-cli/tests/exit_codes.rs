mod common;
use common::Harness;
#[test]
fn normative_exit_codes() {
    let h = Harness::new();
    assert_eq!(h.run(&["init"]).0, 0);
    assert_eq!(h.run(&["nonsense"]).0, 2);
    assert_eq!(
        h.run(&[
            "ingest",
            "cir",
            "--input",
            &Harness::fixture("bad_proto_decode")
        ])
        .0,
        3
    );
    h.ingest("decide");
    assert_eq!(h.run(&["functions", "cfg", "--function", "nope"]).0, 7);
    h.conn()
        .execute("UPDATE compilation_units SET source_decision_count=99", [])
        .unwrap();
    assert_eq!(h.run(&["db", "check"]).0, 6);
}
#[test]
fn init_idempotent_and_environment_db() {
    let h = Harness::new();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ske"))
        .env("SKE_DB", &h.db)
        .arg("init")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("already up to date"));
}
