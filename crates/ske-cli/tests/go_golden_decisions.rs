mod go_support;
use go_support::*;

#[test]
fn all_manual_source_decisions_and_byte_spans() {
    let lab = Lab::new();
    assert_golden(&lab.conn());
    lab.ok(&["db", "check"]);
}
