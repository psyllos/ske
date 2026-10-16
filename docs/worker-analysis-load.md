# SQL → pure Program loader

Scope: `crates/ske-query/src/analysis_load.rs` only, plus this report.
No Cargo/module wiring, orchestration, persistence, CLI, schema or migration
changes. Parent must add `prost`, `ske-cir`, `ske-symbolic` dependencies and the
module declaration after integrating the engine.

## Contract

`pub fn load(db: &rusqlite::Connection, function_id: i64)
-> ske_common::Result<ske_symbolic::program::Program>`.

- SQL function ID is input only. `FunctionDef.id` is the original symbol ID:
  §4.6 mandates function ID = symbol ID. Its stripped symbol local key therefore
  reconstructs the original function ID without guessing from block names.
- Strip the exact `unit_key + "|"` prefix from SQL local keys, including keys
  containing extra delimiters. Keep original protobuf references inside payloads,
  type blobs and return-type JSON unchanged.
- Load every type and symbol in the function's unit, plus the selected function's
  values and unit-global values (NULL function owner). Exclude other functions'
  local values. Global values retain empty `function_id`.
- Reconstruct ordered parameter IDs from `param_index`; reject gaps/missing indices.
- Preserve block/instruction/edge ordinals and ordered operands/case values.
- Decode TypeDef, LiteralValue and OperationPayload with prost; PHI predecessor
  refs and type-test/call payload metadata remain untouched.
- Edge outcome IDs are SQL `ir_outcomes.id` decimal strings, not local edge IDs;
  edges without IR outcomes have `None`.
- Instruction spans are omitted from the engine input (empty vector); persisted
  source provenance remains in SQL. This DTO is for execution, not CIR export.
- Missing function returns `E_NOT_FOUND`; SQL, decoding and malformed local-key
  failures return `E_DB_INTEGRITY`.

Caller owns transaction/read consistency and database integrity checks; this
function does not mutate the database, start a run or validate all CIR invariants.

## Validation

- `rustfmt --edition 2021 crates/ske-query/src/analysis_load.rs`: PASS.
- `rustfmt --edition 2021 --check crates/ske-query/src/analysis_load.rs`: PASS.
- `git diff --check`: PASS.
- Compilation and loader runtime tests: **NOT RUN**. Engine DTO crate is not
  merged into this isolated worktree and the requested module/dependency wiring
  belongs to the parent. Formatting is not evidence of compilation or semantics.

Integration checks still required: compile against the actual Program DTO;
ingest trusted decide/adjusted/switch/invoke fixtures and assert original IDs,
parameter ordering, PHI refs, case ordering, global lookup and SQL outcome IDs.
Also exercise malformed blobs, absent function and unit keys containing `|`.
