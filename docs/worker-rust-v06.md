# Rust v0.6 worker — implementation and executed evidence

## Outcome

Delivered a real, isolated compiler-backed **bounded pilot**, not full §10.1:
`adapters/rust/`, exact normative `examples/rust-branch-lab/`, and
`docs/adr/ADR-022-rust.md`. No core, schema, root workspace, integration CLI or
shared documentation files were changed by this worker. No push/shared merge.

The adapter consumes existing framed protobuf ListUnits/Extract requests and
emits CIR from actual pinned nightly MIR. Source inventory is independently
parsed before compilation. Four normative source decisions and their source
spans are checked against hand-transcribed expectations. The exact fixture is
successfully extracted and imported by existing `ske-ingest`, with clean SQLite
integrity and foreign keys. Neither output-derived goldens nor fixture-specific
extractor constants are used.

**Full Rust/v0.6 acceptance is not claimed.** Every unit is PARTIAL because
implicit panic/Drop and broader Rust semantics remain incomplete. MATCH and `?`
are inventoried with correct outcomes but their complete activation mappings
are not attested. Missing mappings are not mislabeled elided.

## Starting evidence and coordination

Read root AGENTS.md, implementation status, last work-log section, v0.1/v0.2/
v0.5 and reconciliation validation, relevant prior blocking report, soundness
limits, normative §10.1 and v0.6 §24, both protobuf schemas, existing Go profile,
framing, emitter and lowerer, core validation/import APIs. Git started clean.

Early message to coordinator confirmed:

- Actual protobuf field names: `project_root`, `build_profile_json`.
- Descriptor key `rust:<entry>` (default `rust:src/lib.rs`).
- Profile:
  `{"profile_version":1,"language":"rust","target":null,"features":[],"cfg":[],"entry":"src/lib.rs"}`.
- Executable: `adapters/rust/target/debug/ske-adapter-rust`.
- `--version`: `ske-adapter-rust 0.6.0`.
- Symbol qualified names: `decide`, `kind`, `parse_len`; function IDs e.g.
  `rust:fn:decide`.
- No core/schema change needed/requested; coordinator owns ingest/CLI integration.

The coordinator approved textual MIR instead of unavailable `rustc_private`
APIs, provided provenance is genuine and limits explicit. The coordinator also
requested separation of adapter build pin (1.99.0) from compiler producer pin.

## Environment / compiler compatibility

Executed:

```sh
rustup toolchain list
rustc +nightly --version
rustc +nightly -vV
rustup component list --toolchain nightly --installed
rustup which --toolchain nightly rustc
rustup which --toolchain nightly-2026-10-15 rustc
```

Observed macOS aarch64 host. Rust 1.99.0 already installed. Existing `nightly`:

```text
rustc 1.101.0-nightly (4ebd25825 2026-10-15)
commit-hash: 4ebd258257737d5b7635c116fa7d281627a7e937
commit-date: 2026-10-15
release: 1.101.0-nightly
LLVM version: 23.1.4
```

Installed nightly components: cargo, clippy, rust-docs, rust-std, rustc, rustfmt.
**rustc-dev absent.** No installation/update/component/global-setting commands
were executed by this worker. Cargo fetched ordinary project dependencies into
its cache and generated the adapter's isolated Cargo.lock.

A dated `nightly-2026-10-15` alias subsequently became visible on this shared
machine with **different** hash `7df81381d027710708209f36b98d11b1c88f8d2e`,
commit date 2026-10-14 / LLVM 23.1.3. That alias is rejected. Discovery verifies
each existing candidate independently, then accepts the installed matching
nightly. Explicit `SKE_RUSTC` is also fully verified and never falls through to
another compiler when mismatched.

**Open setup blocker:** reconcile rustup distribution date vs compiler commit
date before advertising a portable dated installation command. The compiler
identity is exact and tested locally; a Linux/macOS compatibility matrix is
not tested. README documents approval-required setup, never silent installation.

## Implemented behavior

- Own Cargo workspace, lockfile, toolchain 1.99.0; separate MIR compiler manifest.
- Full compiler release/hash verification. Framed TOOLCHAIN_MISMATCH, stderr
  explanation and exit 3; no studied compiler invocation before verification.
- One request/process, u32-BE framing, checked truncation/oversize/zero-length,
  16 MiB message cap; request message-count and smaller timeout bounds.
- Closed profile parsing, normalized sorted/deduplicated cfg/features,
  relative confined entry path, canonical-root/symlink escape checks.
- Source-only safety gate before compiler: no studied Cargo, build.rs,
  `--extern`, proc-macro loading, linker or studied executable execution.
- Private immutable source snapshot avoids source/IR TOCTOU drift.
- Compiler timeout up to 600 s, polled 64 MiB output-file caps, kill and wait.
  This is not an OS sandbox or enforced process-memory limit.
- Source `if`, `match`, `?`, while/for/loop inventory and separate count visitor.
  Source and compiler character columns are converted to normative UTF-8 byte
  columns; a non-ASCII same-line function-name regression protects the rule.
- Atomic IF mapping only with an exact compiler span match and unique complete
  boolean pair; compound conditions are explicitly not guessed.
- Real MIR local declarations, blocks, assignments, comparison/copy/constants,
  return, `switchInt` BRANCH/SWITCH, SSA renaming and predecessor-exit PHIs.
- Real calls lower to INVOKE with NORMAL/EXCEPTION and open-world unknown
  effects, unresolved target diagnostic, explicit unwind-continue THROW sink.
- Unsupported rvalues: OPAQUE_OP + conservative effects and diagnostic.
- Types: fixed-width integers preserve width/signedness; bool/string; host-only
  usize/isize; aggregates remain OPAQUE.
- Wire types/values/instructions/edges validated using existing ske-cir before
  publishing Header. Existing importer independently validates full unit/SSA.
- Header semantic profile/capability flags expose partial Rust precision.

## Executed validation

Final commands on this worker:

```sh
(cd adapters/rust && cargo build)
(cd adapters/rust && cargo fmt --check)
(cd adapters/rust && cargo test --locked)
(cd adapters/rust && cargo clippy --all-targets --locked -- -D warnings)
(cd adapters/rust && cargo test rust_golden_decisions --locked)
(cd adapters/rust && cargo test lang_types --locked)
(cd adapters/rust && cargo test rust_toolchain_mismatch --locked)
(cd adapters/rust && cargo test rust_adapter_framing --locked)
adapters/rust/target/debug/ske-adapter-rust --version
./scripts/check-deps.sh
git diff --check
```

Results: all above commands **exit 0**. Full adapter suite: **11 tests passed,
0 failed, 0 ignored**; each explicitly named filtered test: 1 passed.
`--version` prints the expected identifier. Named tests:

| Test | Executed result / evidence |
|---|---|
| `rust_golden_decisions` | PASS: 2 IF expressions, MATCH n with CASE:0/CASE:1/DEFAULT, ERROR_PROPAGATION s.parse()? with SUCCESS/ERROR; spans reread in source, count 4, atomic IF mappings complete |
| `lang_types` | PASS: i32 BITVECTOR(32,signed), u8 BITVECTOR(8,unsigned) |
| `rust_real_mir_cfg_and_import` | PASS: real kind SWITCH, parse_len INVOKE, existing ingest_many import, PARTIAL retained, PRAGMA integrity_check=ok and zero FK violations |
| `rust_generic_not_fixture_constants` | PASS: unrelated i16 function with changed identifier/comparison/constants extracts GE and changed source span |
| `rust_toolchain_mismatch` | PASS: subprocess missing compiler and actual incompatible installed Rust 1.99.0 overrides => framed TOOLCHAIN_MISMATCH, readable stderr, exit 3 |
| `rust_adapter_framing` | PASS: subprocess ListUnits/done and Extract Header…EndUnit framing; malformed/truncated/oversized input rejected |
| `rust_no_untrusted_cargo_or_macro_execution` | PASS: include!/derive/cfg attributes/modules/arbitrary rustflags/escape path rejected; build.rs root rejected |
| `rust_profile_closed_contract` | PASS: canonical shape, feature normalization, wrong language/version/trailing content rejected |
| `rust_utf8_source_provenance` | PASS: non-ASCII identifier before condition gives exact byte span and compiler mapping |
| `rust_compound_mapping_never_guessed` | PASS: compound IF outcomes remain non-elided and mapping incomplete with diagnostic |
| `rust_timeout_and_pin_consistency` | PASS: zero compiler deadline kills/waits with LIMIT_EXCEEDED; manifest identity matches implementation constants |

`./scripts/check-deps.sh` also executed successfully: **DEPS OK**, exit 0.
A source/dependency grep over `crates/` for rustc_private/
rustc_driver/rustc_interface found no matches.

### Failures found and fixed, not suppressed

1. First build: sha2 0.11 digest array has no LowerHex; fixed generic hex
   encoding, rebuilt successfully.
2. First tests: temporary root `/var` vs canonical `/private/var` on macOS
   caused false escape rejection; canonicalized root before comparison.
3. First tests: incompatible dated alias masked existing matching nightly;
   now each existing candidate must independently match full identity.
4. Clippy rejected a needless test argument borrow; fixed without allowances.
5. Added Unicode test initially failed (`" x < "` instead of `"x < 0"`),
   proving syn character columns cannot be copied directly into CIR byte
   columns. Corrected both source and MIR span conversion. Regression now green.

No tests were ignored, goldens generated or normative expected results changed.

## Acceptance boundaries / remaining work

This report establishes the locally executed **Rust portions** only:
isolated build, Rust golden source inventory, Rust scalar types, mismatch
handling, adapter framing, real compiler extraction and existing-core import.
It does not claim all AC-0.6-01/-05/-11 globally (adapter-contract documentation,
Java types, cross-runtime Go/Java round trips and CLI db-check belong to
integration).

Not executed here: full root verify.sh, preceding milestone validators,
cross-language symbolic/relation acceptance, CLI ingest/inspect/branches,
Go↔Java semantic frame round trips, Linux CI, or a toolchain matrix.
No CIR freeze tag is created.

Open §10.1 limitations:

- Macro/cfg-attribute provenance, modules, proc macros, closures, async/await,
  generic definitions, FFI and dependency-aware Cargo configuration are rejected,
  not silently analyzed with an incomplete source inventory.
- Implicit panic/Drop/abort coverage incomplete; assert/drop and unsupported
  terminators fail. Broader trait dispatch unresolved. Result/Option semantics
  and arbitrary projections are opaque. Unsafe operations remain partial.
- Match range/`?` ordered activation mapping incomplete. No mapping absence is
  asserted to be elision or proven unreachable.
- Cross-target layout rejected. Host-only layout and Rust 2021 are explicit.
- `max_memory_mb` is not independently enforced by this worker; the parent
  launcher is responsible for OS resource/process-group limits. Compiler output
  polling caps are bounded checks, not hard filesystem quotas.
- Text MIR parsing is pinned and fragile; unsupported forms fail before Header.
- Distribution-date reconciliation requires additional verified setup work.

Next coordinator step: pull the owned changes, build the adapter in the parent
worktree, run its integration/E2E tests, and retain the above limitations in
milestone status rather than reporting full Rust/v0.6 PASS.
