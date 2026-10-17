# v0.6 worker — shared ingestion and CLI integration

Base: `main` at `17036c0` (validated v0.5). This report covers integration
only, not v0.6 acceptance. No AC is declared PASS here.

## Scope and interfaces

Owned changes: `crates/ske-ingest/`, necessary façade additions in
`crates/ske-query/src/lib.rs`, `crates/ske-cli/src/`, and this report.
No edits to CIR, storage, symbolic execution, schemas, migrations, adapter
implementations, cross-language acceptance tests, or validation scripts.

The former `go.rs` implementation is now `language.rs`: one transactional
ingestion rule for Go, Rust, and Java. Existing `GoOptions`, `ingest_go`,
`Query::ingest_go`, `profile::normalize`, and `adapter::list_units` remain
source-compatible. New interfaces:

- `Language::{Go,Rust,Java}`, `Language::as_str()`.
- `IngestOptions` aliases the existing options structure (same fields).
- `ingest_language(storage, language, &options)`, `ingest_rust`, `ingest_java`.
- Corresponding `Query` methods and reexports.
- `profile::normalize_for(language, Option<&str>)`.
- `adapter::list_units_for(language, executable, root, profile, limits)`.
  Legacy `list_units` remains Go-only.

Discovery descriptors must match the selected language. The first extraction
header must independently match it, otherwise extraction rolls back and a
`UNIT_FAILED` diagnostic contains `E_TYPE_MISMATCH`. A mismatched header is
not retained as valid unit provenance. The standalone CIR import path is
unchanged and still accepts any supported language.

Independent discovery/per-unit commits, aborted empty snapshots, failure
provenance, source validation, framing, bounded subprocesses, limits, and
diagnostic redaction reuse the existing Go implementation. Inspect already
returns stored `semantic_profile` and `capability_flags`; no new inspect
implementation or compiler types were needed.

## CLI and profiles

`ske ingest go|rust|java --repo PATH` shares one clap argument structure:
`--profile`, `--adapter`, `--yes-run-adapter`, `--max-message-bytes`,
`--max-messages`, `--timeout-ms`, `--max-memory-mb`, `--redact`,
`--require-complete`. Repository/global formatting options are unchanged.
Default adapter paths:

- Go: `target/ske-adapter-go` (unchanged).
- Rust: `adapters/rust/target/debug/ske-adapter-rust`.
- Java: `adapters/java/ske-adapter-java`.

An explicit profile wins; otherwise use `REPO/build-profile.json` if present,
then language defaults. Rust/Java normalization never invokes Go or a studied
build. Effective defaults agreed with the coordinator:

```json
{"profile_version":1,"language":"rust","target":null,"features":[],"cfg":[],"entry":"src/lib.rs"}
```

```json
{"profile_version":1,"language":"java","class_path":["target/classes"],"source_roots":["src/main/java"],"target_release":17}
```

Normalization rejects unknown keys, wrong language/version, invalid field
types, empty entries in string lists, invalid Rust entry/target, and a Java
release outside positive u32. It emits canonical object JSON without reordering
arrays (classpath ordering is semantic). The existing 1 MiB profile bound
applies to direct API input as well as file reads. Toolchain compatibility and
actual target/release support remain adapter responsibilities.

Both new commands use the same consent store as Go, scoped to canonical
repository, executable path, and executable content hash. Nonterminal calls
without consent fail before execution; `--yes-run-adapter` records consent.
Redaction also applies to top-level failures for Rust and Java. As before,
direct library ingestion is a caller-authorized operation, not an interactive
consent API.

## Executed checks

Local commands, not acceptance claims:

1. `cargo fmt --all -- --check` initially reported formatting differences;
   `cargo fmt --all` fixed them.
2. `cargo test -p ske-ingest -p ske-cli --lib --bins --locked` succeeded
   before the new tests were added (8 ingestion unit tests).
3. `cargo test -p ske-ingest -p ske-cli --locked` succeeded:
   **123 CLI tests + 36 ingestion tests**, zero failures/ignored tests.
   This included existing Go golden, consent, build-tag, failure, behavior,
   reconciliation, SIGINT and importer regressions.
4. `cargo clippy -p ske-ingest -p ske-query -p ske-cli --all-targets --locked -- -D warnings`
   first found an unnecessary `UnitDef` struct update in the new test.
   Removed it; the exact command then succeeded.
5. `cargo fmt --all -- --check` and `git diff --check` succeeded.
6. After the confirmed Rust default-path adjustment:
   `cargo test -p ske-ingest --test adapter_ipc multilang --locked`
   succeeded (1 test); `cargo test -p ske-cli --bin ske --locked`
   succeeded (1 consent test).
7. A Python subprocess probe against `target/debug/ske` with
   `stdin=subprocess.DEVNULL`, temporary repository/database, and a trusted
   shell adapter that only writes a marker verified **both Rust and Java**:
   nonterminal refusal exits 2 without a marker; explicit consent permits
   execution (empty response exits 3); unchanged identity reuses consent;
   modifying executable bytes invalidates consent; `--redact` masks the
   renewed refusal. Both language probes succeeded.
8. After moving the shared implementation from `go.rs` to `language.rs`,
   repeated fmt, Clippy and diff checks succeeded; additionally
   `cargo test -p ske-ingest --lib --test adapter_ipc --locked`
   succeeded (9 unit tests + 20 IPC tests).

New durable tests:

- Strict defaults/canonical profiles, bounds, unknown keys, wrong types,
  language mismatch, classpath order.
- Real framed adversarial child IPC for Rust/Java descriptors, Go wrapper
  rejection, successful empty-unit ingestion, independent mismatched-header
  rejection/rollback, preserved profile/capability metadata.
- Consent idempotence and identity changes for executable content/path.

## Limits and remaining coordinator validation

- No real Rust/Java adapter extraction was executed in this isolated worker;
  adapter workers and coordinator own those implementations and acceptance.
- New shell children are protocol/safety tests, not simulated analysis results.
- No `verify.sh`, full workspace tests, cross-language behavior tests, v0.6
  validator, CI, or CIR freeze/tag was run here.
- The host applies structural profile validation; semantic compiler options,
  Java precompiled-only operation, unsupported constructs and extraction
  diagnostics must still be validated with the real adapters.
- Consent hashes the selected executable, as in Go; for a script launcher it
  does not separately hash its downstream jars/toolchain.
- No system packages, push, shared merge, or changes outside owned paths.
