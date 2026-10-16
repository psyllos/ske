# v0.3 independent acceptance tests

## Ownership and contract

Only the five assigned CLI integration suites, `tests/differential/**`, and
this report changed. Existing `common` and `go_support` helpers are reused
without modification. No dependency, Cargo, schema, migration, analyzer,
normative fixture, or golden change is needed.

Parent confirmed §11 CLI: `branches analyze --entry`, optional `--budget`,
`--context`, `--require-complete`; `branches list --function --run latest`.
Budget uses `[budget]`; integer contexts use `[[param_range]]`, inclusive
`min`/`max`. Proof DTO uses `evidence[].proof` or flat `proof`.

## Coverage

- `analysis_table`: exactly 17 tests named by AC-0.3-08, handwritten §15.3
  statuses, run state, closure and cuts. Decide, Check, MemNoCall and MemOpaque
  execute on actual Go ingestion and fresh, single-fixture CIR databases.
  AddOne asserts the actual persisted `{"x":"127"}` SMT model. MemOpaque
  arrival classifications are checked on both input paths. Count3 u2 checks
  the closure explanation; Sum checks `--require-complete` exit 4.
- Every table run checks no WITNESSED, no proof in any nonclosed persisted
  run, unchanged inventory counts and post-analysis `db check`.
- Determinism compares ordered statuses, explanations and query fingerprints
  across two real Decide analyses; IDs and timing are intentionally excluded.
- Verification-level suite requires public proof metadata, explicit assumptions,
  solver version, MODEL_UNREACHABLE, and no CERTIFIED_UNREACHABLE or WITNESSED.
- Source activation checks ordered DNF using the existing independently named
  atom oracle, absence of false proofs on short-circuit outcomes, the actual
  AllocTwice final FALSE condition, incomplete→UNKNOWN and elided→UNANALYZED.
  Inventory mutations occur before proofs; they are not simulated results.
- Differential executes the trusted repository Go runner with the required
  module replacement. Forty concrete observations include all Check error
  classes and signed int8 overflow at 127. Integer input is constrained per
  observation in the real analyzer; observed source outcomes must not be
  excluded. Boolean parameters remain free because §11 only allows integer
  param ranges. These are hand-written mathematical expectations, not SKE goldens.

## Commands actually run

1. `cargo test -p ske-cli --test analysis_table --test analysis_deterministic --test differential --test source_activation_regression --test verification_level --no-run --locked`
   — PASS, five integration executables compiled; run three times, including
   after context, exit-code, and final source-activation corrections.
2. `(cd tests/differential && go run .)` — PASS, real JSON array of 40
   observations, including AddOne(127)=1 and no Decide=-2.
3. `(cd tests/differential && go vet ./... && go test ./...)` — PASS;
   Go reports `[no test files]` (this is a runner, not a Go unit-test suite).
4. `cargo test -p ske-cli --test analysis_deterministic --locked`
   — **FAIL, exit 101**, one failed / zero ignored: current CLI returns
   exit 2 / E_USAGE, `unrecognized subcommand 'analyze'`. This is the actual
   missing integration boundary, not bypassed or skipped.
5. Owned Rust files formatted with rustfmt (`skip_children=true`); trusted
   runner formatted with gofmt.

## Limits / next validation

Analyzer behavior is **not validated** by compilation or by the concrete runner.
No full expensive Go/table/workspace regression was run before CLI integration.
After parent integrates analyzer, rerun the exact five-suite command without
`--no-run`; failures must be resolved against the normative table, not relaxed.
Final compilation includes the source-activation correction selecting
AllocTwice's final FALSE, not every TRUE outcome. `git diff --check` also
returned exit 0.
Scripted solver/budget internals, cancellation, verifier rejection, SQL trigger
immutability, and complete v0.1/v0.2 regressions belong to other owners; no
claim that those acceptance criteria passed is made here.
