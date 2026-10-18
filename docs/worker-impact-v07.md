# v0.7 targeted impact correction

## Scope and rule

Read root AGENTS, implementation status, latest work log, historical v0.1/v0.2
validation reports and normative AC-0.7-05. Existing v0.7 changes were already
present; this task does not reimplement them.

Changes are limited to `knowledge.rs` impact/resolution, its new test module
`impact_tests.rs`, and this report. No schema, migration, explain body,
commit, push or shared-branch merge.

- Traverse every known callee ID, including targets of incomplete/open sites.
- Compute reverse closure once by function ID; deduplicate callers and terminate
  cycles. The changed function itself is not listed as its own impacted caller.
- Return caller function IDs and unit keys; sort deterministically by
  qualified name, unit key and ID.
- Reject ambiguous unqualified functions with `E_AMBIGUOUS`; candidates and
  explicit selectors use `unit_key|qualified_name`.
- Preserve database errors instead of relabeling all failures as not-found.
- Return `incomplete_sites`, deduplicated by caller ID and site key, containing
  caller/unit identity, dispatch, resolution, world closure and
  `on_affected_path`.
- Any unresolved, external, dynamic, targetless or open site in the snapshot
  makes the answer incomplete. Sites in the known affected reverse closure
  are explicitly marked relevant; unrelated uncertain sites are retained
  because they can hide additional callers. An empty known caller list is
  therefore not proof of no impact with an incomplete graph.

The resolver is shared with explain; the explain implementation was not changed.
Ambiguous names now fail explicitly for both consumers. Qualified selectors are
covered for impact only; this report does not claim expanded explain label support.

## Executed evidence

- `cargo test -p ske-query impact_ --locked`: 5 passed, 0 failed,
  0 ignored, 22 filtered out (rerun after Clippy correction: same result).
- `cargo test -p ske-query --lib --locked`: 27 passed, 0 failed, 0 ignored.
- `cargo test -p ske-cli --test mcp mcp_impact_helper --locked -- --test-threads=1`:
  1 passed, 0 failed, 0 ignored, 3 filtered out; real Go fixture ingestion.
- `cargo clippy -p ske-query --all-targets --locked -- -D warnings`:
  initially failed on the map-entry lint; changed to the entry API and reran:
  exit 0.
- `rustfmt --edition 2021 crates/ske-query/src/knowledge.rs crates/ske-query/src/impact_tests.rs`:
  exit 0.
- `git diff --check`: exit 0.

Five new regressions use real SQLite with production migrations, foreign keys
enabled and persisted call-site/target rows (no fake query result):

1. helper modification finds Entry while a separate unresolved site belongs
   to Entry; asserts `incomplete=true` and affected-path site identity.
2. Open unresolved multi-target site, duplicate edges, diamond/cycle:
   all known callers retained exactly once and uncertainty site deduplicated.
3. Duplicate function names in different units: ambiguity plus exact
   unit-qualified selection, with no caller identity leakage.
4. Empty known caller set with uncertainty elsewhere cannot appear complete;
   a graph without uncertain sites remains complete.
5. Open resolved site, EXTERNAL resolution and external target stub each
   mark the affected path incomplete.

## Limits / parent follow-up

No full v0.7 validator, full workspace suite or historical AC replay run here.
The existing MCP impact test checks Entry but not an unresolved site; the new
SQLite regression supplies that stronger assertion at the query boundary.
Unknown targets are not invented as definite callers. `incomplete=true` is the
explicit boundary on the known caller list, not a proof that every possibly
affected function has been enumerated. Full milestone certification and
integration of other workers remain the parent's responsibility.
