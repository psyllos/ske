//! CIR wire types and stateless local validation; no unit buffering or persistence.
pub mod ske {
    pub mod cir {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/ske.cir.v1.rs"));
        }
    }
    #[allow(clippy::large_enum_variant)]
    pub mod adapter {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/ske.adapter.v1.rs"));
        }
    }
}
pub use ske::adapter::v1 as adapter_proto;
pub use ske::cir::v1 as proto;
pub mod bitvec;
pub mod registry;
mod validation;
pub use validation::*;

/// Missing optional effect flags mean possible effects, never purity.
pub fn call_effects(call: &proto::CallSemantics) -> (bool, bool) {
    (
        call.may_raise.unwrap_or(true),
        call.may_mutate_memory.unwrap_or(true),
    )
}
pub fn opaque_effects(op: &proto::OpaqueSemantics) -> (bool, bool) {
    (
        op.may_exit_abnormally.unwrap_or(true),
        op.may_mutate_memory.unwrap_or(true),
    )
}
