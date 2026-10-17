# ADR-022-rust — Isolated pinned textual MIR producer

Status: accepted for the bounded Rust pilot; full §10.1 coverage remains open.

## Context and alternatives

v0.6 requires real compiler IR, source provenance, isolated toolchains and no
compiler types leaking into the core. Installed nightly was inspected with
`rustc +nightly -vV`; `rustc-dev` is not installed. Installing toolchains or
components without the machine owner's consent is forbidden.

1. `rustc_driver`/`rustc_interface` provide richer typed MIR/provenance, but require
   the missing rustc-dev component and are not usable as installed.
2. Stable `--emit=mir` provides real MIR but lacks the requested unstable controls
   and source provenance comments.
3. **Selected:** run the existing exact-hash nightly with
   `-Zunpretty=mir -Zmir-opt-level=0 -Zmir-include-spans=yes`, after collecting source
   decisions with `syn`. Text MIR is a fragile compiler API; unrecognized shapes
   fail explicitly, and wire type/instruction/edge validation runs before output.
   No hand-written fake CFG, fixture-name lookup or fixture-specific constants.

## Toolchain split

`adapters/rust/rust-toolchain.toml` pins **adapter implementation compilation** to
installed 1.99.0. `adapters/rust/compiler-pin.toml` separately pins the **studied MIR
producer** release 1.101.0-nightly and full commit
`4ebd258257737d5b7635c116fa7d281627a7e937` (compiler commit date 2026-10-15).
The producer is verified at runtime even when selected by `SKE_RUSTC`.
Mismatch is framed `TOOLCHAIN_MISMATCH`, readable stderr, exit 3.

The initially requested `nightly-2026-10-15` distribution name was found to have a
different compiler hash (`7df81381d`, commit date 2026-10-14). Existing `nightly`
has the tested exact hash. Discovery only accepts a candidate after verification;
the name is not an identity. No installation was requested or performed by this
worker. Reconcile the actual rustup distribution date before claiming a portable
setup or compiler compatibility matrix. An explicitly supplied matching
`SKE_RUSTC` is reproducible by identity today, not by an unverified date alone.

## Contract and safety

The existing protobuf protocol and CIR v1.3 schema are unchanged. One request per
process, u32-BE framing, 16 MiB message ceiling, ListUnits descriptor then done,
Extract Header to EndUnit, existing fields `project_root/build_profile_json`.
The profile is closed:
`profile_version=1, language=rust, target=null, features=[], cfg=[], entry=src/lib.rs`.
No studied Cargo invocation, build script or proc macro execution; no linker.
Source-only accepted input is snapshotted privately before rustc reads it.
Unsupported expansion/cfg attributes, modules, generic definitions, closures,
async and FFI are explicit errors. Custom cfg/features are compiler arguments
and fingerprint inputs, never arbitrary rustc flags.

## Precision

Source and IR inventories are distinct. Four normative fixture decisions come
from AST expressions with exact source spans and an independent count visitor.
Atomic IF mappings use exact compiler span equality and a unique boolean pair.
MIR mutable locals become SSA assignments/PHIs. `switchInt` produces real SWITCH
or BRANCH; calls preserve NORMAL/EXCEPTION control and unwind propagation.
Unresolved targets stay open-world and effect-conservative. Unsupported rvalues
are explicit OPAQUE_OP; unsupported terminators fail the extraction.

Match-range/`?` activation cases are not yet proven complete. No missing mapping
is called `elided`; no unreachable proof is fabricated. Unit inventory remains
PARTIAL with diagnostic RUST_INVENTORY_PARTIAL. Implicit panic/Drop, broader trait
dispatch, macros, async, closures, unsafe/FFI, full Cargo configuration/dependency
resolution and cross-target layout remain blockers to complete §10.1 support.

## Consequences and validation

No root core/schema/workspace edits. Rust build/test dependencies are separately
locked. The normative Rust fixture is copied exactly. Executed commands, initial
failures, checks and unresolved acceptance scope are recorded in
`docs/worker-rust-v06.md`. Full v0.6, cross-language and root regression acceptance
belong to integration, not this ADR.
