mod common;
use common::Harness;
fn negative(code: &str) {
    let h = Harness::new();
    let before = h.counts();
    let path = Harness::fixture(&format!(
        "bad_{}",
        code.trim_start_matches("E_").to_lowercase()
    ));
    let (exit, value) = h.run(&[
        "ingest",
        "cir",
        "--input",
        &path,
        "--source-root",
        &Harness::root(),
    ]);
    assert_eq!(exit, 3, "{value}");
    assert_eq!(value["error"]["code"], code);
    assert_eq!(h.counts(), before);
}
macro_rules! cases {
    ($($name:ident => $code:literal),* $(,)?) => {$(
        #[test] fn $name() { negative($code); }
    )*};
}
cases! {
    negative_proto_decode => "E_PROTO_DECODE",
    negative_msg_too_large => "E_MSG_TOO_LARGE",
    negative_header_missing => "E_HEADER_MISSING",
    negative_record_order => "E_RECORD_ORDER",
    negative_cir_major_unsupported => "E_CIR_MAJOR_UNSUPPORTED",
    negative_end_unit_missing => "E_END_UNIT_MISSING",
    negative_duplicate_id => "E_DUPLICATE_ID",
    negative_ordinal_duplicate => "E_ORDINAL_DUPLICATE",
    negative_param_order => "E_PARAM_ORDER",
    negative_empty_id => "E_EMPTY_ID",
    negative_unknown_opcode => "E_UNKNOWN_OPCODE",
    negative_unknown_kind => "E_UNKNOWN_KIND",
    negative_dangling_ref => "E_DANGLING_REF",
    negative_arity_mismatch => "E_ARITY_MISMATCH",
    negative_type_mismatch => "E_TYPE_MISMATCH",
    negative_payload_missing => "E_PAYLOAD_MISSING",
    negative_payload_kind => "E_PAYLOAD_KIND",
    negative_no_terminator => "E_NO_TERMINATOR",
    negative_multi_terminator => "E_MULTI_TERMINATOR",
    negative_terminator_not_last => "E_TERMINATOR_NOT_LAST",
    negative_edge_source_mismatch => "E_EDGE_SOURCE_MISMATCH",
    negative_edge_cross_function => "E_EDGE_CROSS_FUNCTION",
    negative_edge_shape => "E_EDGE_SHAPE",
    negative_entry_block => "E_ENTRY_BLOCK",
    negative_ssa_multi_def => "E_SSA_MULTI_DEF",
    negative_ssa_dominance => "E_SSA_DOMINANCE",
    negative_phi_pred_mismatch => "E_PHI_PRED_MISMATCH",
    negative_path_invalid => "E_PATH_INVALID",
    negative_span_invalid => "E_SPAN_INVALID",
    negative_literal_width => "E_LITERAL_WIDTH",
    negative_counter_mismatch => "E_COUNTER_MISMATCH",
    negative_source_hash_mismatch => "E_SOURCE_HASH_MISMATCH",
    negative_source_activation_invalid => "E_SOURCE_ACTIVATION_INVALID",
}
#[test]
fn negative_unit_duplicate() {
    let h = Harness::new();
    let before = h.counts();
    let path = Harness::fixture("decide");
    let (exit, value) = h.run(&[
        "ingest",
        "cir",
        "--input",
        &path,
        "--input",
        &path,
        "--source-root",
        &Harness::root(),
    ]);
    assert_eq!(exit, 3, "{value}");
    assert_eq!(value["error"]["code"], "E_UNIT_DUPLICATE");
    assert_eq!(h.counts(), before);
}
