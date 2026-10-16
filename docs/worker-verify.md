# T-0.3-06 — independent verifier

Implemented in `crates/ske-verify/src/verify.rs`, no symbolic or classifier
dependency (production dependencies: rusqlite only). Schema/migration unchanged.

## API / integration contract

`verify_unreachability(&mut Connection, run_id, outcome_id, replay)` returns
`rusqlite::Result<Verification>` with proof ID, acceptance, reason, level and
soundness. The mutable connection permits an immediate transaction: all SQL
rereads, proof insertion and result insertion share a consistent locked snapshot.
The replay phase is read-only. SQL errors roll back both proof and result.
Rejected semantic checks persist a REJECTED proof and UNKNOWN result with
`PROOF_REJECTED:<reason>`; only accepted results reference a proof.

Replay closure signature: `FnMut(&str, u32) -> ReplayResult`. It MUST construct
a fresh solver for each invocation, must not write solver_queries, and use
the supplied seed (always 1). It must translate operational solver errors to
Unknown (or stop before calling the verifier for unavailable solver).
Production adapter for the pending solver crate:

```text
|text, seed| {
    let mut solver = Z3Solver::new();
    let budget = SolverBudget { seed, rlimit: ..., wall_timeout_ms: ... };
    match solver.check_smtlib(text, &budget) {
        Unsat => ReplayResult::Unsat,
        Sat { .. } => ReplayResult::Sat,
        Unknown { .. } => ReplayResult::Unknown,
        Timeout { .. } => ReplayResult::Timeout,
    }
}
```

No arbitrary program is executed: tests invoke trusted Z3 on manual SMT formulas.
The generic API itself cannot enforce closure freshness; the production adapter
must preserve this contract. Kept `&mut Connection` rather than an unchecked
deferred transaction on `&Connection` to retain the write-lock guarantee.

Checks include closed/completed/cut-free run, matching outcome snapshot,
required distinct assumptions, no unresolved arrivals anywhere in run,
all candidate arrivals UNSAT and supported, and arrival queries from same run
with UNSAT result. Soundness is worst arrival soundness, EXACT if none.
Every UNSAT query in the run is replayed, even if another check already failed,
and even for outcomes with no arrivals. Replay creates no solver_queries rows.
Proof is inserted before branch result; existing immutable support triggers
remain enabled and tested.

## Validation executed

- `cargo test -p ske-verify verifier_`: 7 passed, 0 ignored (real Z3).
- `cargo test -p ske-verify decide_proof_row`: 1 passed, 0 ignored.
- `cargo clippy -p ske-verify --all-targets -- -D warnings`: exit 0.
- `rustfmt --check --edition 2021 crates/ske-verify/src/lib.rs crates/ske-verify/src/verify.rs crates/ske-verify/tests/verification.rs`: exit 0.
- The Decide test imports the real CIR fixture and uses manual QF_BV signed
  64-bit Decide path constraints, replayed by a fresh Z3 subprocess at seed 1.
  Exactly one ACCEPTED/0/EXACT proof and four original solver query rows.
- False UNSAT test scripts the producer's persisted result for a satisfiable
  formula; real Z3 rejects it, leaves UNKNOWN/PROOF_REJECTED and a rejected proof.
- Run-wide replay test has no arrivals on target and a false upstream UNSAT;
  both run UNSAT queries are replayed and proof rejected.
- Closure, cuts, status, required assumption, unsupported soundness, SAT arrival
  and missing query mutations are independently rejected.
- Accepted proof checks result insertion order and immutable proof, assumptions,
  solver_queries, arrivals and final run status triggers.
- Additional tests cover EXACT/OVERAPPROX/OPAQUE preservation, accepted zero
  arrivals, UNKNOWN/TIMEOUT replay rejection, and forbidden direct dependencies.

## Limits / next integration

The exploration engine and solver crate were not yet present in this isolated
worktree. The positive test is NOT claimed as an end-to-end engine-generated
Decide run: it independently builds SQL evidence from the real imported fixture.
Parent must wire its production fresh-solver closure and run engine E2E before
claiming full v0.3 or AC-0.3-07 integration PASS. No CLI, source classification,
formal source semantics certification, or workspace-wide regression claimed.
Cargo automatically updated Cargo.lock for the new workspace member during
validation; that generated out-of-scope edit was restored after validation.
Parent owns final lockfile integration.
