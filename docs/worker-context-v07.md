# Context Builder v0.7 — AC-0.7-06 worker report

## Scope and implementation

Read AGENTS, implementation status, latest work log, validation v0.1/v0.2,
BLOCKED reports, validation v0.7 / review-v0.7-open, spec §7.4 / AC-0.7-06.
Only `crates/ske-api/src/context.rs`, `crates/ske-api/src/context_query.rs` and
this report changed by this worker. No schema, DDL, protocol, commit, push or
shared merge.

`build_from_roots(&BTreeMap<String, Section>, &[String], max_tokens)` follows
actual `Section.links` from caller-selected roots. Stable graph keys identify
nodes; unrelated nodes, including unknowns and broken unrelated references,
are excluded. Reachable missing references (including roots) return
`E_CONTEXT_REFERENCE`. Cycles/duplicate roots terminate with one visit per key.
The reachable closure is ordered rule → case → issue/outcome → proof, then
by stable key. Relevant UNKNOWN/TIMEOUT/UNSUPPORTED nodes precede ordinary
facts and are mandatory, never silently dropped.

`try_build` budgets the full compact serde JSON Context, not merely its text:
kind/status labels, escaping, Unicode character count, newlines, metadata
and the final `tokens_used` field are included using ceil(chars/4).
The self-referential token count stabilizes before comparing the budget.
Explicit `E_CONTEXT_BUDGET` failure is reserved for budgets that cannot hold
the metadata plus all relevant unknowns (impossible tiny budgets **with
mandatory facts**). When there are no relevant unknowns and the budget is
smaller than the serialized envelope itself, the builder returns an **empty
`truncated` context** whose `tokens_used` still reports the real envelope cost
— nothing is silently dropped and nothing is falsely claimed to fit.
Optional sections are admitted whole if they fit, otherwise `truncated=true`.
`total` means reachable candidates, not all unrelated graph nodes.

Compatibility `build(Vec<Section>, ...) -> Result<Context>` remains available
for already-selected flat facts and is fallible (no panic path); new callers
should use the fallible functions. It cannot resolve links without an identity
map; graph callers must use `build_from_roots`.

## Query-connected adapter — `crates/ske-api/src/context_query.rs`

`build_function_context(query, snapshot, repository, function, max_tokens)`
materializes an identity-resolved `rule → case → issue → proof` graph from
**public, read-only** `ske_query` DTOs and prices it with the Context Builder.
No SQL and no direct DB access live in `ske-api`; only `Query::list_rules` and
`Query::branches_run` (paged to exhaustion) are used.

- Functions are scoped structurally by the `{qualified_name}@` rule-key prefix
  that `ske-behavior` assigns; an unrelated rule is dropped inside
  `materialize` too, so the graph never contains nodes outside the selection.
- Rules link their case; each case links its GUARD_OUTCOME IR outcomes (from
  `evidence` anchors, identity-resolved — never a cross-product).
- IR statuses/proofs come from `branches_run(..., "latest", ...)`: issue nodes
  carry the mapped status, proof nodes carry verifier/verdict/level. A missing
  BRANCH_ANALYSIS run degrades to UNANALYZED, never an invented claim.
- Relevant UNKNOWN/TIMEOUT/UNSUPPORTED outcomes of the function not referenced
  by any rule are function-level unknowns, linked from the function root and
  mandatory before optional facts; impossible budgets return `E_CONTEXT_BUDGET`.
- The upstream `Query::list_function_rules` (function-filtered) is not required:
  the adapter already pages the read-only `list_rules` and filters by scope.

`lib.rs` is **not** changed by this worker (the parent wires `pub mod
context_query;`). The module was compile-tested transiently with a local
`mod` wiring that was reverted byte-identically afterwards.

## Checks executed

- `cargo test -p ske-api --lib -- --test-threads=1` (with the adapter wired):
  **16 passed** — 6 context + 4 context_query + 2 llm + 1 service (+3 others).
- The two `context_query` unit tests cover mixed rules (unrelated function
  excluded), unknown priority, budget 500, proofs, missing-analysis refs and
  impossible tiny budget (mandatory unknown).
- **Real fixture, end to end** (inside `context_query.rs`):
  `build_function_context_real_fixture_end_to_end` to
  `build_function_context_scopes_real_graph_and_fails_closed` ingest
  `examples/go-branch-lab` through `Query::ingest_go` (Go adapter compiled in
  process), run real `Query::analyze` (Z3) over `example.Decide`, persist
  rules/cases/evidence via `Query::reconstruct_behaviors`, then build the
  context. Assertions run against the live DTO: budget respected, rules
  present, unknown-status outcomes (when present on the data) kept, and
  `example.Other` rules excluded. Only public `Query` + std are used — no
  SQL, no storage access, no Cargo changes.
- `cargo clippy -p ske-api --all-targets -- -D warnings`: exit 0 (wired).
- `rustfmt --edition 2021 --check crates/ske-api/src/context.rs
  crates/ske-api/src/context_query.rs`: exit 0.
- `git diff --check`: exit 0; `git diff -- crates/ske-api/src/lib.rs` empty.

## Limits (remaining, deliberate)

The budget covers compact serialization by `serde_json::to_string(Context)`;
pretty-printing or wrapping the result in another payload needs its own budget.
This is the normative character estimate, not a provider-specific tokenizer.
Whole-section optional omission can omit ancestor text while retaining a
mandatory unknown; selection itself is still from the complete graph closure.
`Query` resolves only exact qualified names (verified: a bare `Decide` yields
`E_NOT_FOUND`), so callers pass the qualified name; the adapter validates it
through `Query` and fails closed on unknown names. A function with no
BRANCH_ANALYSIS run materializes rules with UNANALYZED statuses (no analysis
claim is invented); statuses/proofs require analysis to have run.
No full v0.7 validation campaign or workspace regression was executed here;
these results do not attest the whole v0.7 milestone.
