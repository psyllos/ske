//! Independent SQL consistency and solver replay verification (§6.6).
mod verify;
pub use verify::{verify_unreachability, ReplayResult, Verification};
