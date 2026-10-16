# T-0.3-02 — expression arena

Scope: only `crates/ske-symbolic/{Cargo.toml,src/lib.rs,src/expression.rs,tests/expressions.rs}`
and this report. Read root instructions, durable status, work log, v0.1/v0.2
validation and historical BLOCKED report before implementation. Those milestones
were not rerun or reimplemented.

## Contract

All expression types stay in `ske-symbolic`, exported at its root. No dependency
on solver; no move into CIR. The parent received the exact public API early for
relay to solver, memory and engine workers (direct sibling delivery was rejected
by this runtime as unknown thread).

- `Type { local_key: String, sort: Sort }`; sort is Bool,
  BitVec `{ width: u32, signed: bool }`, or Uninterpreted.
- `ExprId(usize)` is arena-local. `Node { ty, kind }` contains the complete
  typed structure. All eight normative node kinds are represented.
- `ExprArena::intern(Node) -> Result<ExprId, ExprError>` validates operands and
  supported operator sorts before simplifying/interning.
- `get`, `iter`, `len`, `is_empty`; constructors `symbol`, `constant`,
  `unary`, `binary`, `ite`, `havoc`. Constructors return Result.
- Havoc counter ownership belongs to the run: `havoc(ty, n)` interns that exact
  identity; callers supply successive n starting at zero. Symbol names likewise
  remain caller-supplied normative names.

The full Node is the HashMap key. A test hasher forces every key to hash to zero
and still obtains distinct IDs for different structures. IDs are append-only;
children must already exist. Invalid types, missing operands and unnormalized
bit constants are rejected without insertion. IDs from different arenas must
not be mixed.

Only §6.2 simplifications are applied: complete constant folding via CIR machine
arithmetic, boolean NOT/double NOT, the three ITE rules, and boolean AND/OR with
constants in either operand position. Zero division/remainder and negative shifts
stay symbolic. Wide shift counts are not truncated. No commutative reordering,
algebraic identities or symbolic reflexive equality reduction.

## Executed validation

1. `rustfmt --edition 2021 crates/ske-symbolic/src/lib.rs crates/ske-symbolic/src/expression.rs crates/ske-symbolic/tests/expressions.rs` — exit 0.
2. `cargo test -p ske-symbolic expr_` — exit 0, five tests passed, zero ignored:
   `expr_interning_structural`, `expr_simplify_each_rule`,
   `expr_no_extra_simplification`, `expr_fold_bitvec_operations`,
   `expr_reject_invalid_nodes`.
3. `cargo clippy -p ske-symbolic --all-targets -- -D warnings` — first exit 101
   for three unnecessary unit expressions; removed them, reran: exit 0.
4. Final `cargo test -p ske-symbolic expr_` and strict Clippy rerun — exit 0.

Cargo automatically added the new package to Cargo.lock during validation.
That generated-only delta was removed to keep ownership within this assignment;
the integrating parent must let Cargo regenerate the lockfile. No root manifest
or other crate changed.

## Limits

This is AC-0.3-04 evidence, not validation of all v0.3. No solver, engine,
memory, persistence, canonical persistent hash, database collision handling,
proof production or replay implemented here. FIELD is a typed projection with
caller-supplied result type; aggregate layout validation belongs to the transfer
layer. Uninterpreted functions are not evaluated. No workspace-wide regression,
CI, shared-branch merge or push executed.
