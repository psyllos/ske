//! AC-0.5-01: one test per rendering rule R1–R6 plus the exact strings of the
//! §v0.5 reference table. Builds typed `ske_symbolic` arenas only — no SQL.
use ske_behavior::{
    dedup_cases, order_cases, render_guard, render_value, rule_description, BehaviorCaseRow,
    BehaviorCaseRowInput, Names,
};
use ske_symbolic::{BinaryOp, Constant, ExprArena, ExprId, Sort, Type, UnaryOp};

fn i64ty() -> Type {
    Type {
        local_key: "int".into(),
        sort: Sort::BitVec {
            width: 64,
            signed: true,
        },
    }
}

fn boolty() -> Type {
    Type {
        local_key: "bool".into(),
        sort: Sort::Bool,
    }
}

fn u8ty() -> Type {
    Type {
        local_key: "u8".into(),
        sort: Sort::BitVec {
            width: 8,
            signed: false,
        },
    }
}

fn cons(a: &mut ExprArena, ty: Type, bits: u128) -> ExprId {
    a.constant(ty, Constant::Bits(bits)).unwrap()
}

fn names_check() -> Names {
    let mut names = Names::new();
    names.insert("param:example.Check:0", "age");
    names.insert("param:example.Check:1", "enabled");
    names.insert(
        "init:{Global(\"example.ErrUnderage\")}@0:havoc#0",
        "ErrUnderage",
    );
    names
}

#[test]
fn render_guard_r1_conjunction_in_path_order() {
    // "age < 18"
    let mut a = ExprArena::new();
    let names = names_check();
    let age = a.symbol(i64ty(), "param:example.Check:0").unwrap();
    let eighteen = cons(&mut a, i64ty(), 18);
    let lt = a.binary(boolty(), BinaryOp::Lt, age, eighteen).unwrap();
    assert_eq!(render_guard(&a, &[lt], &names), "age < 18");
    assert_eq!(render_guard(&a, &[], &names), "true");
}

#[test]
fn render_guard_r2_negation_push_and_bare_bool() {
    let mut a = ExprArena::new();
    let names = names_check();
    let age = a.symbol(i64ty(), "param:example.Check:0").unwrap();
    let enabled = a.symbol(boolty(), "param:example.Check:1").unwrap();
    let eighteen = cons(&mut a, i64ty(), 18);
    let lt = a.binary(boolty(), BinaryOp::Lt, age, eighteen).unwrap();
    let not_lt = a.unary(boolty(), UnaryOp::Not, lt).unwrap();
    let not_enabled = a.unary(boolty(), UnaryOp::Not, enabled).unwrap();
    // "age >= 18 AND enabled == false"
    assert_eq!(
        render_guard(&a, &[not_lt, not_enabled], &names),
        "age >= 18 AND enabled == false"
    );
    // "age >= 18 AND enabled == true" — bare bool param at guard root.
    assert_eq!(
        render_guard(&a, &[not_lt, enabled], &names),
        "age >= 18 AND enabled == true"
    );
    // Single negated comparison only.
    assert_eq!(render_guard(&a, &[not_lt], &names), "age >= 18");
}

#[test]
fn render_guard_decide_negations_and_signed_constants() {
    // Decide case 1: NOT(x < 0) AND NOT(x < -10) -> "x >= 0 AND x >= -10"
    let mut a = ExprArena::new();
    let mut names = Names::new();
    names.insert("param:example.Decide:0", "x");
    let x = a.symbol(i64ty(), "param:example.Decide:0").unwrap();
    let zero = cons(&mut a, i64ty(), 0);
    let neg10 = cons(&mut a, i64ty(), 18446744073709551606);
    let lt0 = a.binary(boolty(), BinaryOp::Lt, x, zero).unwrap();
    let lt_neg10 = a.binary(boolty(), BinaryOp::Lt, x, neg10).unwrap();
    let n1 = a.unary(boolty(), UnaryOp::Not, lt0).unwrap();
    let n2 = a.unary(boolty(), UnaryOp::Not, lt_neg10).unwrap();
    assert_eq!(render_guard(&a, &[n1, n2], &names), "x >= 0 AND x >= -10");
    // R3: signed constants render in decimal per type signature.
    assert_eq!(render_value(&a, neg10, &names), "-10");
    assert_eq!(render_value(&a, zero, &names), "0");
}

#[test]
fn render_guard_r6_adjusted_infix_without_simplification() {
    // Adjusted: NOT(x > 10) -> "x <= 10"; result "x - 0" and "x - 3"
    let mut a = ExprArena::new();
    let mut names = Names::new();
    names.insert("param:example.Adjusted:0", "x");
    let x = a.symbol(i64ty(), "param:example.Adjusted:0").unwrap();
    let ten = cons(&mut a, i64ty(), 10);
    let three = cons(&mut a, i64ty(), 3);
    let gt = a.binary(boolty(), BinaryOp::Gt, x, ten).unwrap();
    let not_gt = a.unary(boolty(), UnaryOp::Not, gt).unwrap();
    assert_eq!(render_guard(&a, &[gt], &names), "x > 10");
    assert_eq!(render_guard(&a, &[not_gt], &names), "x <= 10");
    // "x - 0" (no algebraic simplification in v0.5)
    let zero = zero_node(&mut a);
    let sub_x_0 = a.binary(i64ty(), BinaryOp::Sub, x, zero).unwrap();
    let sub_x_3_expr = sub_x_3(&mut a);
    assert_eq!(render_value(&a, sub_x_3_expr, &names), "x - 3");
    assert_eq!(render_value(&a, sub_x_0, &names), "x - 0");
    // R6: parens around a nested BINARY operand.
    let inner = a.binary(i64ty(), BinaryOp::Add, x, x).unwrap();
    let sub_inner = a.binary(i64ty(), BinaryOp::Sub, x, inner).unwrap();
    assert_eq!(render_value(&a, sub_inner, &names), "x - (x + x)");
    let _ = three;
}

fn zero_node(a: &mut ExprArena) -> ExprId {
    cons(a, i64ty(), 0)
}
fn sub_x_3(a: &mut ExprArena) -> ExprId {
    let x = a.symbol(i64ty(), "param:example.Adjusted:0").unwrap();
    let three = cons(a, i64ty(), 3);
    a.binary(i64ty(), BinaryOp::Sub, x, three).unwrap()
}

#[test]
fn render_guard_r1_dedup_identical_atoms_divmod() {
    // DivMod else case carries the same NOT(Eq(b,0)) twice (branch negation +
    // DIV's divisor-nonzero guard). R1 renders it once.
    let mut a = ExprArena::new();
    let mut names = Names::new();
    names.insert("param:example.DivMod:1", "b");
    let b = a.symbol(i64ty(), "param:example.DivMod:1").unwrap();
    let zero = cons(&mut a, i64ty(), 0);
    let eq = a.binary(boolty(), BinaryOp::Eq, b, zero).unwrap();
    let not_eq = a.unary(boolty(), UnaryOp::Not, eq).unwrap();
    // Same rendered atom twice.
    assert_eq!(render_guard(&a, &[not_eq, not_eq], &names), "b != 0");
    let eq2 = a.binary(boolty(), BinaryOp::Eq, b, zero).unwrap();
    let not_eq2 = a.unary(boolty(), UnaryOp::Not, eq2).unwrap();
    assert_eq!(render_guard(&a, &[not_eq, not_eq2], &names), "b != 0");
    // The "b == 0" case stays.
    assert_eq!(render_guard(&a, &[eq], &names), "b == 0");
}

#[test]
fn render_r4_fallbacks_and_globals() {
    let mut a = ExprArena::new();
    let mut names = Names::new();
    // Unresolved param -> p<index>.
    let p1 = a.symbol(i64ty(), "param:example.Check:0").unwrap();
    assert_eq!(render_value(&a, p1, &names), "p0");
    // Unresolved non-param symbol -> <expr:N>, incrementing.
    let u1 = a
        .symbol(u8ty(), "init:{ParamPointee(\"m\", 0)}@0:havoc#0")
        .unwrap();
    let u2 = a
        .symbol(u8ty(), "init:{ParamPointee(\"m\", 1)}@2:havoc#2")
        .unwrap();
    let two = cons(&mut a, u8ty(), 2);
    let b = a.binary(u8ty(), BinaryOp::Add, u1, two).unwrap();
    let text = render_value(&a, b, &names);
    assert_eq!(text, "<expr:0> + 2");
    // Second unresolved gets the next counter in a fresh call.
    assert_eq!(render_value(&a, u2, &names), "<expr:0>");
    // Resolved global via names -> short name.
    names.insert(
        "init:{Global(\"example.ErrUnderage\")}@0:havoc#0",
        "ErrUnderage",
    );
    let g = a
        .symbol(u8ty(), "init:{Global(\"example.ErrUnderage\")}@0:havoc#0")
        .unwrap();
    assert_eq!(render_value(&a, g, &names), "ErrUnderage");
    // Null renders nil.
    let nil_ty = Type {
        local_key: "ptr".into(),
        sort: Sort::Uninterpreted,
    };
    let nil_node = a.constant(nil_ty, Constant::Null).unwrap();
    assert_eq!(render_value(&a, nil_node, &names), "nil");
}

#[test]
fn render_r3_signed_and_unsigned_constants() {
    let mut a = ExprArena::new();
    let names = Names::new();
    let neg1 = cons(&mut a, i64ty(), 18446744073709551615);
    assert_eq!(render_value(&a, neg1, &names), "-1");
    let neg2 = cons(&mut a, i64ty(), 18446744073709551614);
    assert_eq!(render_value(&a, neg2, &names), "-2");
    let three = cons(&mut a, i64ty(), 3);
    assert_eq!(render_value(&a, three, &names), "3");
    // Unsigned: no two's-complement decode.
    let big = cons(&mut a, u8ty(), 250);
    assert_eq!(render_value(&a, big, &names), "250");
    let yes = a.constant(boolty(), Constant::Bool(true)).unwrap();
    let no = a.constant(boolty(), Constant::Bool(false)).unwrap();
    assert_eq!(render_value(&a, yes, &names), "true");
    assert_eq!(render_value(&a, no, &names), "false");
}

#[test]
fn rule_description_returns_and_panic() {
    assert_eq!(
        rule_description("age < 18", "RETURN", "ErrUnderage"),
        "WHEN age < 18 THEN return ErrUnderage"
    );
    assert_eq!(
        rule_description("x > 10", "RETURN", "x - 3"),
        "WHEN x > 10 THEN return x - 3"
    );
    assert_eq!(
        rule_description("b == 0", "PANIC", ""),
        "WHEN b == 0 THEN panic"
    );
    assert_eq!(
        rule_description("b != 0", "RETURN", "a / b, nil"),
        "WHEN b != 0 THEN return a / b, nil"
    );
}

fn row(
    guard: &str,
    kind: &str,
    result: &str,
    span: Option<(u64, u64)>,
    path: Vec<&str>,
) -> BehaviorCaseRowInput {
    BehaviorCaseRowInput {
        guard: guard.into(),
        exit_kind: kind.into(),
        result: result.into(),
        return_keys: vec![result.into()],
        span,
        ir_path: path.into_iter().map(str::to_owned).collect(),
        memory_delta: String::new(),
        effects: String::new(),
        precision: "EXACT".into(),
        fingerprint: "fp".into(),
        summary_case_id: None,
    }
}

fn crow(
    guard: &str,
    kind: &str,
    result: &str,
    span: Option<(u64, u64)>,
    path: Vec<&str>,
) -> BehaviorCaseRow {
    let input = row(guard, kind, result, span, path);
    BehaviorCaseRow {
        guard: input.guard,
        exit_kind: input.exit_kind,
        result: input.result,
        span: input.span,
        ir_path: input.ir_path,
        memory_delta: input.memory_delta,
        effects: input.effects,
        precision: input.precision,
        fingerprint: input.fingerprint,
        referenced_outcomes: Vec::new(),
        summary_case_id: None,
    }
}

#[test]
fn behavior_case_ordering_by_span_then_ir_path() {
    let rows = vec![
        crow("x <= 10", "RETURN", "x - 0", Some((17, 2)), vec!["0/0"]),
        crow("x > 10", "RETURN", "x - 3", Some((17, 2)), vec!["0/1"]),
        crow("no span", "RETURN", "z", None, vec!["0/0"]),
        crow(
            "age < 18",
            "RETURN",
            "ErrUnderage",
            Some((10, 3)),
            vec!["0/0"],
        ),
    ];
    let ordered = order_cases(rows);
    let guards: Vec<_> = ordered.iter().map(|r| r.guard.as_str()).collect();
    // Same span: IR path lexicographic (0/0 before 0/1). Span 10 before 17.
    // No-span last.
    assert_eq!(guards, vec!["age < 18", "x <= 10", "x > 10", "no span"]);
}

#[test]
fn behavior_case_dedup_conservative() {
    let two_returns = vec![
        row("a", "RETURN", "x", Some((1, 1)), vec!["0/0"]),
        row("b", "RETURN", "x", Some((2, 1)), vec!["0/1"]),
    ];
    let merged = dedup_cases(two_returns);
    assert_eq!(merged.len(), 1, "identical exit_kind/result merge");
    assert_eq!(merged[0].guard, "a OR b");
    assert_eq!(
        merged[0].referenced_outcomes,
        vec!["0/0".to_owned(), "0/1".to_owned()]
    );
    // Never merge RETURN with PANIC.
    let kind_change = vec![
        row("a", "RETURN", "x", Some((1, 1)), vec!["0/0"]),
        row("b", "PANIC", "", Some((1, 2)), vec!["0/1"]),
    ];
    assert_eq!(dedup_cases(kind_change).len(), 2);
    // Different result text does not merge.
    let diff_result = vec![
        row("a", "RETURN", "x", Some((1, 1)), vec!["0/0"]),
        row("b", "RETURN", "y", Some((1, 2)), vec!["0/1"]),
    ];
    assert_eq!(dedup_cases(diff_result).len(), 2);
}

#[test]
fn behavior_dedup_distinguishes_opaque_returns_and_preserves_precision() {
    let mut a = row("a", "RETURN", "<expr:0>", Some((1, 1)), vec!["0/0"]);
    let mut b = row("b", "RETURN", "<expr:0>", Some((1, 1)), vec!["0/1"]);
    a.return_keys = vec!["distinct-expression-a".into()];
    b.return_keys = vec!["distinct-expression-b".into()];
    assert_eq!(dedup_cases(vec![a.clone(), b.clone()]).len(), 2);
    b.return_keys = a.return_keys.clone();
    b.precision = "OVERAPPROX".into();
    let merged = dedup_cases(vec![a, b]);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].precision, "OVERAPPROX");
}
