# v0.6 independent multilang CLI acceptance tests

## Scope and ownership

Added only `crates/ske-cli/tests/multilang.rs` and this report. No adapter,
schema, migration, production code, golden generator, or fixture implementation
was changed. No push, landing, or CIR freeze tag was performed.

Read root instructions, implementation status, latest work-log section,
v0.1/v0.2/v0.6 validation reports, relevant resolved v0.4 blocking report,
specification §10 / v0.6, existing common/Go support tests, protobuf contracts,
storage schema, and concrete-interpreter API.

## Oracles

All twelve requested test names exist, without ignores, environment-based skips,
or substitute results:

- `rust_golden_decisions`, `java_golden_decisions`: manually written kinds,
  ordered labels, and exact condition text recovered using persisted byte spans.
  Java `TRY_CATCH` uses the `try` keyword span, coordinated with the Java worker.
- `lang_types`: actual parameter TypeDef payloads, widths and signedness.
- `cross_language_decide`: actual ingested CIR interpreted over nine boundary
  inputs (`i32::MIN`, -11, -10, -9, -1, 0, 1, 10, `i32::MAX`), compared to the
  independent relation `x < 0 ? -1 : 0`. Separate actual CLI analyses constrain
  `x` with a context file and check closure, assumptions and exact source-outcome
  statuses. Elided outcomes must remain `UNANALYZED` / `ELIDED`, not proofs.
  No CFG/IR edge-count equality is required across languages.
- `java_switch_ir`: real terminating SWITCH and machine literal values 1/2/3.
- `java_invoke_exception`: NORMAL and typed EXCEPTION on actual INVOKEs.
- `java_reflection_unknown`: actual call-site UNRESOLVED/open-world plus diagnostic.
- `rust_toolchain_mismatch`: subprocess-only `SKE_RUSTC` points to the absolute
  rustc obtained by `rustup which --toolchain 1.99.0 rustc`; expect CLI exit 3,
  readable persisted TOOLCHAIN_MISMATCH and no CLOSED snapshot.
- `multilang_inspect`: noninteractive refusal, explicit consent and saved
  executable identity; per-language JSON and persisted profile/capabilities.
- `multilang_integrity_framing`: SQLite integrity/FKs and actual CLI db check;
  independent u32 big-endian frame parsing, real list responses and rejection
  of protocol major 99, across all three adapters.
- `cir_compat_multilang`: real CIR validation/import into unchanged schema,
  no import ERROR, real instructions, retained non-elided source links.
- `java_multi_catch_finally`: same INVOKE has ordered NumberFormatException /
  RuntimeException filters, NORMAL and catch-all continuation. Either every
  terminating CFG path visits the finally println call or a function-scoped
  EXCEPTION_FLOW_PARTIAL diagnostic is required. INVENTORY impact forbids
  function COMPLETE. No assertion that a println count proves path coverage.

The public concrete executor is in `ske-symbolic`, not `ske-summary`.
Its Program requires a full reconstructed proto function/type/value/block/edge
graph; the query storage loader is private. To avoid duplicating that loader
and expanding production dependencies, the test uses a narrow independent
SQL scalar interpreter for Decide only. It handles COPY/PHI, signed comparisons,
logical negation, integer negation, JUMP/BRANCH/RETURN, selects PHIs by predecessor,
and fails on unsupported operations or after 100 blocks. It never executes
studied source code or invents a result for unsupported CIR. The parent approved
this narrow fallback after inspection of the public API.

## Build and fixture safety

Each adapter is built once per test process using OnceLock. Go reuses existing
support. Rust runs `cargo build` in `adapters/rust`, expecting its own
`target/debug/ske-adapter-rust`. Java runs `mvn -B package` and uses
`adapters/java/ske-adapter-java`.

Each test copies trusted repository fixtures into an isolated temporary tree,
excluding target/.ske/.git; studied build scripts are never invoked.
Java compiles only the three explicitly named specification files using
`javac --release 17 -g -d target/classes`. Fixture code is not executed.
CLI ingestion uses real profiles and explicit `--yes-run-adapter`.
No global environment mutation is used.

Rust worker reports the installed local `nightly` matches the pinned compiler
identity but the dated rustup alias is absent. Tests intentionally do not hide
this by setting RUSTUP_TOOLCHAIN internally. A parent-run explicitly overridden
build is a distinct check, not evidence for exact AC-0.6-01.

## Commands and observed results

- `cargo test -p ske-cli --test multilang --no-run --locked`: initially failed
  on rusqlite `usize` decoding; corrected to supported u32 decoding. Rerun PASS.
- `rustfmt --edition 2021 crates/ske-cli/tests/multilang.rs`: completed.
- `cargo clippy -p ske-cli --test multilang --locked -- -D warnings`: PASS,
  including final rerun.
- `rustfmt --edition 2021 --check crates/ske-cli/tests/multilang.rs`: PASS
  on final source.
- `cargo test -p ske-cli --test multilang cross_language_decide -- --exact
  --nocapture`: FAIL at the missing Rust fixture after the Go leg (including
  concrete relation and constrained analyses).
- `cargo test -p ske-cli --test multilang -- --test-threads=1`: 0 passed,
  12 failed, 0 ignored. Eleven tests reached missing Rust/Java fixtures
  (workers' isolated changes not merged); framing additionally exposed a test
  oracle sorting raw Go keys rather than normative package path then profile.
  Corrected the test sort rule, without changing the adapter.
- `cargo test -p ske-cli --test multilang multilang_integrity_framing --
  --exact --nocapture`: final rerun FAIL at missing
  `examples/rust-branch-lab`, after Go framing, protocol-error and DB assertions.

No v0.6 AC is declared PASS from these partial runs. Rust/Java ingestion,
their adapter builds/extraction, Java trusted compilation, Rust mismatch
execution, and their assertions remain unexercised until the parent merges the
workers. Full workspace regression and `./scripts/verify.sh` were not run here.
The parent must run the full suite after integration and fix actual interface
or implementation defects, not weaken the independent oracles to obtain green.
