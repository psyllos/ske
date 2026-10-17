//! Isolated, fail-closed Rust compiler adapter. No Cargo invocation on studied code.
pub mod compiler;
pub mod frame;
pub mod lower;
pub mod profile;
pub mod source;

pub const VERSION: &str = "ske-adapter-rust 0.6.0";
