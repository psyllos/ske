use ske_symbolic::*;
use std::hash::{BuildHasherDefault, Hasher};

fn bool_ty() -> Type {
    Type {
        local_key: "bool".into(),
        sort: Sort::Bool,
    }
}
fn int_ty(width: u32, signed: bool) -> Type {
    Type {
        local_key: format!("bv{width}:{signed}"),
        sort: Sort::BitVec { width, signed },
    }
}
#[derive(Default)]
struct Collision;
impl Hasher for Collision {
    fn write(&mut self, _: &[u8]) {}
    fn finish(&self) -> u64 {
        0
    }
}

#[test]
fn expr_interning_structural() {
    let mut a = ExprArena::with_hasher(BuildHasherDefault::<Collision>::default());
    let ty = int_ty(8, true);
    let x = a.symbol(ty.clone(), "param:f:0").unwrap();
    assert_eq!(x, a.symbol(ty.clone(), "param:f:0").unwrap());
    let y = a.symbol(ty.clone(), "param:f:1").unwrap();
    assert_ne!(x, y); // Every hash in this arena is exactly zero.
    let xy = a.binary(ty.clone(), BinaryOp::Add, x, y).unwrap();
    assert_eq!(xy, a.binary(ty.clone(), BinaryOp::Add, x, y).unwrap());
    assert_ne!(xy, a.binary(ty.clone(), BinaryOp::Sub, x, y).unwrap());
    assert_ne!(x, a.symbol(int_ty(8, false), "param:f:0").unwrap());
    for kind in [
        ExprKind::Field { base: x, index: 0 },
        ExprKind::Uninterpreted {
            name: "f".into(),
            args: vec![x, y],
        },
        ExprKind::Havoc(0),
    ] {
        let n = Node {
            ty: ty.clone(),
            kind,
        };
        let id = a.intern(n.clone()).unwrap();
        assert_eq!(id, a.intern(n).unwrap());
    }
    assert_ne!(a.havoc(ty.clone(), 0).unwrap(), a.havoc(ty, 1).unwrap());
}

#[test]
fn expr_simplify_each_rule() {
    let mut a = ExprArena::new();
    let b = bool_ty();
    let t = a.constant(b.clone(), Constant::Bool(true)).unwrap();
    let f = a.constant(b.clone(), Constant::Bool(false)).unwrap();
    let x = a.symbol(b.clone(), "param:f:0").unwrap();
    let y = a.symbol(b.clone(), "param:f:1").unwrap();
    assert_eq!(a.unary(b.clone(), UnaryOp::Not, t).unwrap(), f);
    assert_eq!(a.unary(b.clone(), UnaryOp::Not, f).unwrap(), t);
    let nx = a.unary(b.clone(), UnaryOp::Not, x).unwrap();
    assert_eq!(a.unary(b.clone(), UnaryOp::Not, nx).unwrap(), x);
    assert_eq!(a.ite(t, x, y).unwrap(), x);
    assert_eq!(a.ite(f, x, y).unwrap(), y);
    assert_eq!(a.ite(x, y, y).unwrap(), y);
    for (op, c, expected) in [
        (BinaryOp::And, t, x),
        (BinaryOp::And, f, f),
        (BinaryOp::Or, t, t),
        (BinaryOp::Or, f, x),
    ] {
        assert_eq!(a.binary(b.clone(), op, c, x).unwrap(), expected);
        assert_eq!(a.binary(b.clone(), op, x, c).unwrap(), expected);
    }
    let ty = int_ty(8, true);
    let max = a.constant(ty.clone(), Constant::Bits(127)).unwrap();
    let one = a.constant(ty.clone(), Constant::Bits(1)).unwrap();
    let min = a.constant(ty.clone(), Constant::Bits(128)).unwrap();
    assert_eq!(a.binary(ty.clone(), BinaryOp::Add, max, one).unwrap(), min);
    let zero = a.constant(ty.clone(), Constant::Bits(0)).unwrap();
    for op in [BinaryOp::Div, BinaryOp::Rem] {
        let id = a.binary(ty.clone(), op, one, zero).unwrap();
        assert!(matches!(a.get(id).unwrap().kind, ExprKind::Binary { .. }));
    }
    let minus_one = a.constant(ty.clone(), Constant::Bits(255)).unwrap();
    for op in [BinaryOp::Shl, BinaryOp::Shr] {
        let id = a.binary(ty.clone(), op, one, minus_one).unwrap();
        assert!(matches!(a.get(id).unwrap().kind, ExprKind::Binary { .. }));
    }
    let wide = a.constant(int_ty(16, false), Constant::Bits(256)).unwrap();
    assert_eq!(a.binary(ty, BinaryOp::Shl, one, wide).unwrap(), zero);
}

#[test]
fn expr_no_extra_simplification() {
    let mut a = ExprArena::new();
    let ty = int_ty(8, true);
    let x = a.symbol(ty.clone(), "x").unwrap();
    let y = a.symbol(ty.clone(), "y").unwrap();
    let zero = a.constant(ty.clone(), Constant::Bits(0)).unwrap();
    let one = a.constant(ty.clone(), Constant::Bits(1)).unwrap();
    assert_ne!(a.binary(ty.clone(), BinaryOp::Add, x, zero).unwrap(), x);
    assert_ne!(a.binary(ty.clone(), BinaryOp::Mul, x, one).unwrap(), x);
    assert_ne!(
        a.binary(ty.clone(), BinaryOp::Add, x, y).unwrap(),
        a.binary(ty.clone(), BinaryOp::Add, y, x).unwrap()
    );
    let eq = a.binary(bool_ty(), BinaryOp::Eq, x, x).unwrap();
    assert!(matches!(a.get(eq).unwrap().kind, ExprKind::Binary { .. }));
    let b = a.symbol(bool_ty(), "b").unwrap();
    let bb = a.binary(bool_ty(), BinaryOp::And, b, b).unwrap();
    assert_ne!(bb, b);
}

#[test]
fn expr_fold_bitvec_operations() {
    let mut a = ExprArena::new();
    let ty = int_ty(8, true);
    let l = a.constant(ty.clone(), Constant::Bits(249)).unwrap(); // -7
    let r = a.constant(ty.clone(), Constant::Bits(2)).unwrap();
    for (op, bits) in [
        (BinaryOp::Add, 251),
        (BinaryOp::Sub, 247),
        (BinaryOp::Mul, 242),
        (BinaryOp::Div, 253),
        (BinaryOp::Rem, 255),
        (BinaryOp::BitAnd, 0),
        (BinaryOp::BitOr, 251),
        (BinaryOp::Xor, 251),
        (BinaryOp::Shl, 228),
        (BinaryOp::Shr, 254),
    ] {
        let expected = a.constant(ty.clone(), Constant::Bits(bits)).unwrap();
        assert_eq!(a.binary(ty.clone(), op, l, r).unwrap(), expected, "{op:?}");
    }
    for (op, value) in [
        (BinaryOp::Eq, false),
        (BinaryOp::Ne, true),
        (BinaryOp::Lt, true),
        (BinaryOp::Le, true),
        (BinaryOp::Gt, false),
        (BinaryOp::Ge, false),
    ] {
        let expected = a.constant(bool_ty(), Constant::Bool(value)).unwrap();
        assert_eq!(a.binary(bool_ty(), op, l, r).unwrap(), expected);
    }
    for (op, bits) in [(UnaryOp::Neg, 7), (UnaryOp::BitNot, 6)] {
        let expected = a.constant(ty.clone(), Constant::Bits(bits)).unwrap();
        assert_eq!(a.unary(ty.clone(), op, l).unwrap(), expected);
    }
    let expected = a.constant(int_ty(16, true), Constant::Bits(65529)).unwrap();
    assert_eq!(
        a.unary(int_ty(16, true), UnaryOp::Cast, l).unwrap(),
        expected
    );
}

#[test]
fn expr_reject_invalid_nodes() {
    let mut a = ExprArena::new();
    assert_eq!(
        a.constant(int_ty(0, false), Constant::Bits(0)),
        Err(ExprError::InvalidType)
    );
    assert_eq!(
        a.constant(int_ty(8, false), Constant::Bits(256)),
        Err(ExprError::InvalidType)
    );
    assert_eq!(
        a.unary(bool_ty(), UnaryOp::Not, ExprId(0)),
        Err(ExprError::InvalidOperand(ExprId(0)))
    );
    assert!(a.is_empty());
    let x = a.symbol(int_ty(8, true), "x").unwrap();
    assert_eq!(
        a.unary(bool_ty(), UnaryOp::Not, x),
        Err(ExprError::InvalidType)
    );
}
