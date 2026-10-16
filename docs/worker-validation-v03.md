# Worker validation v0.3

Scope: `scripts/validate-v03.py`, `scripts/verify.sh`, and the authorized
`SKE_EXPECT_VERIFY_VERSION` compatibility edit in `scripts/validate-v02.py`.
No manual edit to the parent's `docs/validation/v0.3.md`.

The new harness maps AC01–29, retains commands, exits, stderr, elapsed times
and assertion failures in `target/validation-v03/`, and writes the durable
validation report only during a real full execution. Tests must execute at
least one passing named case, with no ignored tests; normative multi-test
groups additionally require their individual names. Missing suites fail.
JSON counts and closure are asserted directly for Decide, with branch/source
statuses and proof evidence; database integrity requires all 22 controls.
Regression invokes v0.2 with v0.3 verify expectation, transitively v0.1, and
requires exactly 25 and 42 PASS records.

Verify now runs actual SAT/accepted UNSAT CLI cases, symbolic scripted
UNKNOWN/TIMEOUT, bounded partial Sum and solver failure persistence before
its five required messages and final `VERIFY OK v0.3`.

Checks executed: `python3 scripts/validate-v03.py --selftest`,
`bash -n scripts/verify.sh`, Python `ast.parse` for both edited harnesses,
and `git diff --check`: PASS.
Full acceptance, full verify and expensive legacy regressions intentionally
not run here: parent owns global final execution.

Integration prerequisites: parent-provided cancellation suite and
`ske-query solver_fail_preserves_validated_results`; missing normative
property or referential-transparency tests naturally FAIL. The local CIR
candidate tag is checked by the unchanged legacy criterion; no tags created.
No golden generation, schema changes, push or shared merge.
