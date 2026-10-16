mod go_support;
use go_support::*;

#[test]
fn exact_ordered_dnf_and_incomplete_provenance() {
    let lab = Lab::new();
    assert_activation(&lab.conn());
    assert_negative_activation(&lab);
}
