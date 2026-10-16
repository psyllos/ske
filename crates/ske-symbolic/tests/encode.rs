use ske_solver::{CheckResult, Solver, SolverBudget, Z3Solver};
use ske_symbolic::*;

fn bv(key: &str, width: u32, signed: bool) -> Type {
    Type {
        local_key: key.into(),
        sort: Sort::BitVec { width, signed },
    }
}
fn boolean() -> Type {
    Type {
        local_key: "bool".into(),
        sort: Sort::Bool,
    }
}
#[test]
fn encode_wide_shift_count_is_compared_before_truncation() {
    let mut arena = ExprArena::new();
    let byte = bv("u8", 8, false);
    let count = bv("u16", 16, false);
    // Symbols, not constants, exercise the encoder rather than constant folding.
    let x = arena.symbol(byte.clone(), "x").unwrap();
    let n = arena.symbol(count.clone(), "n").unwrap();
    let one = arena.constant(byte.clone(), Constant::Bits(1)).unwrap();
    let big = arena.constant(count, Constant::Bits(256)).unwrap();
    let zero = arena.constant(byte.clone(), Constant::Bits(0)).unwrap();
    let shifted = arena.binary(byte, BinaryOp::Shl, x, n).unwrap();
    let cx = arena.binary(boolean(), BinaryOp::Eq, x, one).unwrap();
    let cn = arena.binary(boolean(), BinaryOp::Eq, n, big).unwrap();
    let bad = arena
        .binary(boolean(), BinaryOp::Ne, shifted, zero)
        .unwrap();
    let text = ske_symbolic::encode::encode(&arena, &[cx, cn, bad]).unwrap();
    assert!(text.contains("bvuge"));
    assert!(text.contains("extract 7 0"));
    assert_eq!(
        Z3Solver::new().check_smtlib(&text, &SolverBudget::default()),
        CheckResult::Unsat
    );
}
#[test]
fn encode_uninterpreted_nil_and_type_names_do_not_collide() {
    let mut arena = ExprArena::new();
    let ty = Type {
        local_key: "p:a|b".into(),
        sort: Sort::Uninterpreted,
    };
    let p = arena.symbol(ty.clone(), "parameter").unwrap();
    let nil = arena.constant(ty, Constant::Null).unwrap();
    let guard = arena.binary(boolean(), BinaryOp::Eq, p, nil).unwrap();
    let text = ske_symbolic::encode::encode(&arena, &[guard]).unwrap();
    assert!(matches!(
        Z3Solver::new().check_smtlib(&text, &SolverBudget::default()),
        CheckResult::Sat { .. }
    ));
    assert_ne!(
        ske_symbolic::encode::symbol_name("a:b"),
        ske_symbolic::encode::symbol_name("a_b")
    );
}
