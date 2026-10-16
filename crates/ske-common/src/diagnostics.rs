//! Diagnostic defaults are data, shared by all producers (§5.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticSpec {
    pub code: &'static str,
    pub severity: &'static str,
    pub impact: &'static str,
}

macro_rules! diagnostics {
    ($(($code:literal, $severity:literal, $impact:literal)),+ $(,)?) => {
        pub const DIAGNOSTICS: &[DiagnosticSpec] = &[
            $(DiagnosticSpec { code: $code, severity: $severity, impact: $impact }),+
        ];
    };
}

diagnostics! {
    ("UNSUPPORTED_CONSTRUCT", "WARNING", "INVENTORY"),
    ("FUNCTION_TRANSLATION_FAILED", "ERROR", "INVENTORY"),
    ("SOURCE_MISMATCH", "WARNING", "INVENTORY"),
    ("INVENTORY_STATUS_DOWNGRADED", "WARNING", "INVENTORY"),
    ("RECOVER_UNMODELED", "WARNING", "INVENTORY"),
    ("CGO_UNSUPPORTED", "WARNING", "INVENTORY"),
    ("OUTCOME_ELIDED_BY_LOWERING", "INFO", "INVENTORY"),
    ("SOURCE_ACTIVATION_INCOMPLETE", "INFO", "SYMBOLIC"),
    ("EXCEPTION_FLOW_PARTIAL", "WARNING", "INVENTORY"),
    ("DEFER_UNMODELED", "WARNING", "EFFECTS"),
    ("IMPLICIT_PANIC_UNMODELED", "INFO", "EFFECTS"),
    ("EXTERNAL_CALL_OPAQUE", "INFO", "EFFECTS"),
    ("OPAQUE_OP_EMITTED", "INFO", "SYMBOLIC"),
    ("UNSUPPORTED_OVERFLOW_MODE", "WARNING", "SYMBOLIC"),
    ("CONCURRENCY_PARTIAL", "WARNING", "CONCURRENCY"),
    ("CALL_TARGET_UNRESOLVED", "INFO", "CALLGRAPH"),
    ("CALL_TARGET_APPROX", "INFO", "CALLGRAPH"),
    ("GENERIC_ORIGIN_OPAQUE", "INFO", "TYPES"),
    ("BUILD_TAG_EXCLUDED_FILE", "INFO", "OTHER"),
    ("SOURCE_NOT_VERIFIED", "INFO", "OTHER"),
    ("CIR_MINOR_NEWER", "INFO", "OTHER"),
    ("LOOP_BOUND_REACHED", "WARNING", "SYMBOLIC"),
    ("BUDGET_EXHAUSTED", "WARNING", "SYMBOLIC"),
    ("SOLVER_UNKNOWN", "WARNING", "SYMBOLIC"),
    ("SOLVER_TIMEOUT", "WARNING", "SYMBOLIC"),
    ("SOLVER_UNAVAILABLE", "ERROR", "SYMBOLIC"),
    ("UNSUPPORTED_TERMINATOR", "WARNING", "SYMBOLIC"),
    ("UNIT_FAILED", "ERROR", "INVENTORY"),
    ("ADAPTER_EFFECT_CLAIMS_TRUSTED", "INFO", "SYMBOLIC"),
    ("SYNTHETIC_FUNCTION", "INFO", "OTHER"),
    ("LOOP_ABSTRACTED", "INFO", "SYMBOLIC"),
}

pub fn lookup(code: &str) -> Option<&'static DiagnosticSpec> {
    DIAGNOSTICS
        .iter()
        .find(|diagnostic| diagnostic.code == code)
}

pub fn degrades_inventory(severity: &str, impact: &str) -> bool {
    impact == "INVENTORY" && matches!(severity, "WARNING" | "ERROR")
}
