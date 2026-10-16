# Analysis read worker

## API and integration

- `analysis_read::summary(db: &Connection, run_id: i64) -> Result<Value>`
  emits the §11.5 analyze response, with all seven status counts initialized.
- `analysis_read::apply(db: &Connection, run_id: i64, function_id: i64,
  dto: &mut Value) -> Result<()>` enriches the existing `branches_page` DTO.
  The parent owns module registration, Cargo, CLI selection and orchestration.
- Missing runs/functions return `E_NOT_FOUND`; wrong snapshot, entry function
  or analysis kind returns `E_USAGE`. Existing inventory, pagination and local
  string IDs remain intact.

## Persisted path contract

`outcome_arrivals.path_json` is an array of SQL `ir_outcomes.id` values:
prior taken outcomes followed by the current candidate. The query result is
for the conjunction of that entire ordered path, including the candidate.
SAT paths match ordered activation terms as a subsequence **ending at the
current arrival**. Individual SAT edges from unrelated paths never combine.
Empty legacy/default paths provide no positive source evidence.

Source links are not consulted. Elided outcomes stay UNANALYZED. Incomplete,
empty or approximate activation mappings yield UNKNOWN, even with IR proofs.
For exact mappings, one jointly SAT case proves POSSIBLE. Every case must
have a globally eliminated term backed by this run's referenced accepted,
closed MODEL_UNREACHABLE proof before source PROVEN_UNREACHABLE is emitted.
This handles zero-arrival suffixes after a verified impossible prefix.
An UNSAT arrival alone is deliberately insufficient: there may be other
unexplored prefixes. Jointly impossible cases without a globally proved term
remain conservatively UNKNOWN; the reader does not call a solver or manufacture
new proof records.

Proof evidence preserves immutable proof/run/outcome IDs, verifier metadata,
verification level, model soundness, assumptions and solver version. Arrival
evidence carries arrival ID and nullable query ID. Metrics come from the run's
persisted JSON, with normative zero defaults when absent.

## Validation

Validation: a scratch Cargo harness imported this module unchanged and copied
the current query façade's `sql`, `rows`, `one`, `local`, and `boolean` helpers.
`cargo test --manifest-path "$DELTA_SCRATCH_DIR/read-test/Cargo.toml"` passed
all three tests: ordered joint matching, SQL summary/counts/defaults/missing run,
and SQL-backed source enrichment (independent SAT edges remain UNKNOWN, a joint
path becomes POSSIBLE, incomplete mapping becomes UNKNOWN, elided becomes
UNANALYZED, mismatched snapshot returns E_USAGE). `git diff --check` passed.

An initial `cargo test -p ske-query analysis_read` was interrupted during
dependency compilation at the one-second tool deadline; it is not a passing
workspace validation. The module is not registered in the parent's library yet.
Full CLI Or/And/Mixed and Decide proof validation remain with the parent once
module wiring and orchestration are integrated.
