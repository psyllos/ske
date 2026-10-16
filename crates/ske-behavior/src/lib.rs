//! Storage- and solver-independent behavior knowledge (§7.2, v0.5).
//!
//! Rendering is deterministic: rules R1–R6 of §7.2 are implemented exactly.
//! The expression DAG is supplied by the caller as a [`ske_symbolic::ExprArena`];
//! name resolution (parameters -> `ValueDef.name`, globals -> short name) is a
//! pure callback so this crate never touches SQL.

pub mod behavior;
pub mod render;

pub use behavior::{
    dedup_cases, order_cases, rule_description, BehaviorCaseRow, BehaviorCaseRowInput,
};
pub use render::{render_guard, render_value, Names};
