//! Exhaustive stable public error registry (§5.5).
macro_rules! error_codes {
    ($( $variant:ident => ($code:literal, $exit:literal) ),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum ErrorCode { $( $variant ),+ }
        impl ErrorCode {
            pub const ALL: &'static [Self] = &[$( Self::$variant ),+];
            pub const fn as_str(self) -> &'static str {
                match self { $( Self::$variant => $code ),+ }
            }
            pub const fn exit_code(self) -> i32 {
                match self { $( Self::$variant => $exit ),+ }
            }
            pub fn from_code(code: &str) -> Option<Self> {
                match code { $( $code => Some(Self::$variant), )+ _ => None }
            }
        }
    };
}

error_codes! {
    ProtoDecode => ("E_PROTO_DECODE", 3),
    MsgTooLarge => ("E_MSG_TOO_LARGE", 3),
    AdapterLimit => ("E_ADAPTER_LIMIT", 3),
    HeaderMissing => ("E_HEADER_MISSING", 3),
    RecordOrder => ("E_RECORD_ORDER", 3),
    CirMajorUnsupported => ("E_CIR_MAJOR_UNSUPPORTED", 3),
    EndUnitMissing => ("E_END_UNIT_MISSING", 3),
    DuplicateId => ("E_DUPLICATE_ID", 3),
    OrdinalDuplicate => ("E_ORDINAL_DUPLICATE", 3),
    ParamOrder => ("E_PARAM_ORDER", 3),
    EmptyId => ("E_EMPTY_ID", 3),
    UnknownOpcode => ("E_UNKNOWN_OPCODE", 3),
    UnknownKind => ("E_UNKNOWN_KIND", 3),
    DanglingRef => ("E_DANGLING_REF", 3),
    ArityMismatch => ("E_ARITY_MISMATCH", 3),
    TypeMismatch => ("E_TYPE_MISMATCH", 3),
    PayloadMissing => ("E_PAYLOAD_MISSING", 3),
    PayloadKind => ("E_PAYLOAD_KIND", 3),
    NoTerminator => ("E_NO_TERMINATOR", 3),
    MultiTerminator => ("E_MULTI_TERMINATOR", 3),
    TerminatorNotLast => ("E_TERMINATOR_NOT_LAST", 3),
    EdgeSourceMismatch => ("E_EDGE_SOURCE_MISMATCH", 3),
    EdgeCrossFunction => ("E_EDGE_CROSS_FUNCTION", 3),
    EdgeShape => ("E_EDGE_SHAPE", 3),
    EntryBlock => ("E_ENTRY_BLOCK", 3),
    SsaMultiDef => ("E_SSA_MULTI_DEF", 3),
    SsaDominance => ("E_SSA_DOMINANCE", 3),
    PhiPredMismatch => ("E_PHI_PRED_MISMATCH", 3),
    PathInvalid => ("E_PATH_INVALID", 3),
    SpanInvalid => ("E_SPAN_INVALID", 3),
    LiteralWidth => ("E_LITERAL_WIDTH", 3),
    CounterMismatch => ("E_COUNTER_MISMATCH", 3),
    SourceHashMismatch => ("E_SOURCE_HASH_MISMATCH", 3),
    UnitDuplicate => ("E_UNIT_DUPLICATE", 3),
    SnapshotClosed => ("E_SNAPSHOT_CLOSED", 5),
    MigrationChecksum => ("E_MIGRATION_CHECKSUM", 6),
    DbIntegrity => ("E_DB_INTEGRITY", 6),
    ProofRequired => ("E_PROOF_REQUIRED", 5),
    ProofImmutable => ("E_PROOF_IMMUTABLE", 5),
    ProofSupportFrozen => ("E_PROOF_SUPPORT_FROZEN", 5),
    SourceActivationInvalid => ("E_SOURCE_ACTIVATION_INVALID", 3),
    HashCollision => ("E_HASH_COLLISION", 5),
    WitnessedNotProducible => ("E_WITNESSED_NOT_PRODUCIBLE", 5),
    NotFound => ("E_NOT_FOUND", 7),
    Ambiguous => ("E_AMBIGUOUS", 2),
    Usage => ("E_USAGE", 2),
    Internal => ("E_INTERNAL", 5),
}
