# Isolated Rust MIR adapter

Build the adapter itself with its own pinned, installed Rust 1.99.0:

```sh
cd adapters/rust
cargo build
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

The adapter is **not** a member of the root Cargo workspace. Its isolated lockfile
includes the shared wire-only `ske-cir` crate through a path dependency. No
`rustc_private`, compiler internals, or compiler-specific dependency enters the core.

## MIR compiler

`compiler-pin.toml` records the separate, unstable MIR producer identity.
The tested producer is `rustc 1.101.0-nightly (7df81381d 2026-10-14)`, full hash
`7df81381d027710708209f36b98d11b1c88f8d2e`, LLVM 23.1.4.
No `rustc-dev` is needed: the adapter requests actual MIR with source span comments
using `-Zunpretty=mir -Zmir-opt-level=0 -Zmir-include-spans=yes`.

The pin uses the **dated** `nightly-2026-10-15` distribution so
`rustup toolchain install nightly-2026-10-15` (as CI does) reproduces this exact
build on any machine. Moving `nightly` was deliberately not pinned: moving
channels are not reproducible. The adapter tests pass on the dated build.

Discovery checks existing `nightly-2026-10-15` and `nightly` installations and
verifies the **full** hash and release before compiling studied sources. An alias
is never trusted just because its name matches. `SKE_RUSTC=/absolute/path/to/rustc`
selects an explicit existing compiler and still requires the same hash/release.
An incompatible/missing compiler produces a framed
`AdapterError{code:"TOOLCHAIN_MISMATCH"}` and exits 3.

No setup script installs or updates any toolchain/component beyond what the
operator explicitly runs (`rustup toolchain install nightly-2026-10-15` on CI).

## IPC

`target/debug/ske-adapter-rust --version` prints `ske-adapter-rust 0.6.0`.
Normal operation consumes **one** u32-big-endian-length-prefixed
`ske.adapter.v1.AdapterRequest` from stdin, and writes framed `AdapterResponse`s.
No text is written to stdout during protocol operation.

Protocol fields are exactly the existing schema's `project_root`,
`build_profile_json`, `unit_key`, and `limits`; `protocol_major=1`.
`ListUnits` emits the descriptor `rust:src/lib.rs` followed by `ListUnitsDone`.
`ExtractUnit` emits CIR `Header` first and `EndUnit` last. The key follows the
entry path (`rust:<entry>`), not a fixture constant.

The closed, canonical build profile is:

```json
{"profile_version":1,"language":"rust","target":null,"features":[],"cfg":[],"entry":"src/lib.rs"}
```

Omitted keys receive these defaults. Unknown keys/trailing content are rejected;
feature/cfg sets are sorted and deduplicated before hashing. Cross-target layout
is not yet supported (explicit failure). Rust 2021, opt-level 0, MIR opt-level 0,
and unwind panic strategy are fixed parts of the semantic profile.

## Safety and precision boundary

The adapter never invokes Cargo in the studied root. It does not execute
`build.rs`, load `--extern` or proc-macro libraries, invoke the linker, or execute
studied binaries. Roots containing `build.rs` are rejected. Attributes, macros
(including `include!`), modules, extern crates, generic definitions, closures,
async and FFI declarations are rejected before compilation. This is an explicit,
deliberately limited source-only subset, **not** general Cargo project support.
The accepted source is copied into a private immutable compiler scratch file.

The compiler subprocess has a 10-minute maximum (or smaller request timeout),
polled output limits of 64 MiB per file, null stdin, private working directory,
and no emitted executable. This is not an OS security sandbox or an enforced
memory limit; the parent launcher remains responsible for process-group and
resource enforcement.

Source `if`/`match`/loop/`?` inventory is parsed separately before MIR. Spans are
byte-based and source hashes cover the actual snapshot. Direct links for simple
atomic `if` conditions require exact compiler span equality and a unique complete
boolean edge pair. No position/order-based link guessing is used. Compound,
match-range and `?` mappings remain explicitly incomplete; outcomes are **not**
marked elided merely because mapping failed.

MIR locals are SSA-renamed at every assignment with block-entry PHIs, including
single-predecessor PHIs. `switchInt` becomes CIR `BRANCH` or `SWITCH`; comparisons,
copies, scalar/string constants and returns are modeled. External calls become
real `INVOKE` normal/exception edges with open-world unknown effects and unresolved
targets. MIR `unwind continue` reaches an explicit `THROW` block. Other unwind
actions/assert/Drop terminators currently fail rather than silently dropping paths.
Unmodeled rvalues become effect-conservative `OPAQUE_OP` plus diagnostics.
Result/Option aggregates remain opaque. `i32`/`u8` and other fixed-width integers
preserve width/signedness; usize/isize use only the adapter host layout.

Every extracted unit is currently `PARTIAL` because implicit panic/Drop inventory
and broad Rust semantics are incomplete. Do not claim full v0.6 or reachability
proofs for unknown mappings. See `docs/worker-rust-v06.md` and ADR-022-rust.
