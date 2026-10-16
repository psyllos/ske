# Pure symbolic engine — T-0.3-03 / T-0.3-05

## Boundary and API

`crates/ske-symbolic/src/program.rs` contains a storage-independent DTO using
validated CIR protobuf types, ordered maps, blocks, and edges. `Edge.outcome_id`
is a persistence identity supplied by the loader, absent for JUMP. The parent
owns SQL loading, persistence, source activation evaluation, verification and CLI.
There is no rusqlite dependency in symbolic.

`engine::analyze` takes Program, Budget, Solver, an initial-constraints closure,
a cancellation callback, and an event callback. The closure receives the arena
and the initial SSA environment and returns ordered constraints. Budget exposes
all twelve §6.7 keys; only `loop_mode=unroll` is accepted.

The report owns the expression arena, query records, arrivals, exits, assumptions,
diagnostics, metrics and classifications. Queries have zero-based run-local IDs.
Arrivals retain ordered constraints and immutable cons-list outcome provenance;
the candidate outcome is included in arrival provenance. This supports checking
whole source activation cases rather than aggregating individual IR statuses.
SAT models use keys `encode::symbol_name("param:<function.id>:<index>")`.
The name is `s` followed by the UTF-8 bytes in lowercase hex.

Events are emitted incrementally before being retained in the report. Solver
unavailability returns Err after emitting its query and diagnostic; the caller
must retain already streamed rows and mark the run FAILED. Cancellation returns
a CANCELLED report. Callbacks must not panic; durable callback errors can trigger
the caller's cancellation flag.

Unreachability classification is **only `UnreachableCandidate`**, never an
accepted proof. The independent SQL/replay verifier must accept it before any
PROVEN_UNREACHABLE persistence. Unsupported terminator arrivals are represented
as Unknown with reason UNSUPPORTED_TERMINATOR and soundness Unsupported; SQL
must map that pair to arrival result UNSUPPORTED.

## Implemented behavior

- FIFO instruction-level exploration; outcome edges ordered by ordinal.
- Arc/COW SSA and constraint snapshots; compact shared outcome histories.
- Actual predecessor selection for PHI with a predecessor environment snapshot,
  so consecutive PHIs see parallel assignments rather than each other's writes.
- Exact-state tracking uses expression IDs and explicit memory snapshot IDs,
  including provenance to avoid losing distinct source activation histories.
- DFS retreating edges, including irreducible CFGs; per-state traversal counts.
- DIV/REM and signed-count shifts query panic and normal successors, sharing
  query/state budgets; implicit exits do not invent IR outcomes.
- Global closure, state/instruction/query/wall/memory-estimate budgets.
- M1 allocation/load/store/field addressing and memory epoch invalidation through
  the independently implemented memory module. Forked states receive distinct
  future allocation lineages.
- Conservative unknown effects and concurrency, transparent-call UFs only under
  explicit effect claims, otherwise fresh results and memory havoc as required.
- Standalone deterministic SMT text for every effective solver call, ordered
  constraints, full text plus seed/rlimit fingerprint, collision check and run
  cache that reuses IDs without calls or extra query records.
- Uninterpreted sorts/nil, typed bitvector operations, casts and nontruncated
  mixed-width shift counts; real parameter model projection.
- Classification preserves SAT priority and best SAT soundness; no local UNSAT
  can create an unreachability candidate when global exploration is open.

## Commands and evidence

Executed in the worker worktree:

- `cargo test -p ske-symbolic --test engine`: initially 6 scripted tests PASS;
  after additions, 11 tests PASS.
- `cargo test -p ske-symbolic`: **26 tests PASS**, zero ignored (8 memory,
  5 expression, 11 engine, 2 encoder).
- `cargo clippy -p ske-symbolic --all-targets -- -D warnings`: PASS.
- `rustfmt --edition 2021` on the owned source/test files: completed.

Tests cover state/query budgets, scripted Unknown/Timeout, candidate-only
classification, streamed cancellation, query-cache reuse, implicit division
budgets, real Z3 joint constraints and parameter models, M1 store/reload, PHI
Count3 at unroll 2 and 3, mixed-width `1 << 256`, and uninterpreted nil encoding.
These are hand-built DTO/arena tests, not fixture-specialized engine behavior.

## Limits and remaining integration checks

- No SQL/CLI acceptance suite was run by this worker; parent integration owns it.
- No proof replay acceptance is performed here and no WITNESSED is produced.
- The memory budget is a deterministic estimate, not a hard process RSS cap.
- v0.3 has no interprocedural unfolding, summary use, or concurrent exploration.
- INDEX/INDEX_ADDR/DOWNCAST panics remain explicitly unmodeled diagnostics;
  unknown memory and aggregate effects are conservatively havocked.
- The arena and solver support machine bitvectors through 128 bits; larger CIR
  widths return an explicit error rather than truncate.
- Full §15.3 laboratory validation remains a parent integration obligation.
