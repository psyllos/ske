//! Typed symbolic execution, concrete CIR interpretation and summary application.
pub mod expression;
pub use expression::*;
pub mod concrete;
pub mod encode;
pub mod engine;
pub mod memory;
pub mod program;
pub mod summary;
