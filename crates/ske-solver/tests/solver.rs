use ske_solver::*;

fn bv(w: u32, n: i128) -> Term {
    Term::bitvec(w, n as u128).unwrap()
}
fn opts(signed: bool) -> OpOptions {
    OpOptions {
        signed,
        ..Default::default()
    }
}
fn op(name: &str, args: &[Term], options: &OpOptions) -> EncodedOp {
    Encoder::op(name, args, options).unwrap()
}
fn prove_equal(got: &Term, want: &Term) {
    let mut e = Encoder::new();
    e.assert(&op("NE", &[got.clone(), want.clone()], &opts(false)).value)
        .unwrap();
    let q = e.finish();
    assert_eq!(
        Z3Solver::new().check(&q, &SolverBudget::default()),
        CheckResult::Unsat,
        "{}",
        q.smtlib
    );
}

#[test]
fn z3_required_signed_sat_unsat_overflow_model() {
    let mut e = Encoder::new();
    let x = e.declare("x", Sort::BitVec(64)).unwrap();
    let neg = op("LT", &[x.clone(), bv(64, 0)], &opts(true)).value;
    e.assert(&neg).unwrap();
    e.assert(&op("LT", &[x, bv(64, -10)], &opts(true)).value)
        .unwrap();
    let mut solver = Z3Solver::new();
    assert!(!solver.version().is_empty());
    assert!(matches!(
        solver.check(&e.finish(), &SolverBudget::default()),
        CheckResult::Sat { .. }
    ));
    let mut e = Encoder::new();
    let x = e.declare("x", Sort::BitVec(64)).unwrap();
    e.assert(&op("LOGICAL_NOT", &[neg], &opts(false)).value)
        .unwrap();
    e.assert(&op("LT", &[x, bv(64, -10)], &opts(true)).value)
        .unwrap();
    assert_eq!(
        solver.check(&e.finish(), &SolverBudget::default()),
        CheckResult::Unsat
    );
    let mut e = Encoder::new();
    let x = e.declare("x", Sort::BitVec(8)).unwrap();
    let plus = op("ADD", &[x.clone(), bv(8, 1)], &opts(true)).value;
    e.assert(&op("LT", &[plus, x], &opts(true)).value).unwrap();
    let CheckResult::Sat { model: Some(model) } =
        solver.check(&e.finish(), &SolverBudget::default())
    else {
        panic!("expected SAT model")
    };
    assert_eq!(
        model["x"],
        ModelValue::BitVec {
            width: 8,
            bits: 127
        }
    );
}

#[test]
fn z3_limits_replay_errors_and_fresh_instance() {
    let mut solver = Z3Solver::new();
    let text = "(declare-const x (_ BitVec 64)) (assert (= (bvmul x x) (_ bv17 64))) (check-sat)";
    let budget = SolverBudget {
        rlimit: 1,
        ..Default::default()
    };
    assert_eq!(
        solver.check_smtlib(text, &budget),
        CheckResult::Timeout {
            kind: TimeoutKind::Rlimit
        }
    );
    assert_eq!(
        solver.check_smtlib("(assert false)", &SolverBudget::default()),
        CheckResult::Unsat
    );
    assert!(matches!(
        solver.check_smtlib("(check-sat)", &SolverBudget::default()),
        CheckResult::Sat { .. }
    ));
    assert!(matches!(
        solver.check_smtlib("(assert missing)", &SolverBudget::default()),
        CheckResult::Unknown { .. }
    ));
    assert!(matches!(
        solver.check_smtlib("\0", &budget),
        CheckResult::Unknown { .. }
    ));
    assert!(matches!(
        solver.check_smtlib(
            "",
            &SolverBudget {
                rlimit: u64::MAX,
                ..budget
            }
        ),
        CheckResult::Unknown { .. }
    ));
    for (reason, want) in [
        ("timeout", TimeoutKind::WallClock),
        ("max. resource limit exceeded", TimeoutKind::Rlimit),
        ("canceled", TimeoutKind::Budget),
    ] {
        assert_eq!(
            map_unknown_reason(reason),
            CheckResult::Timeout { kind: want }
        );
    }
    assert_eq!(
        map_unknown_reason("incomplete"),
        CheckResult::Unknown {
            reason: "incomplete".into()
        }
    );
}
#[test]
fn scripted_order_and_text_preserved() {
    let expected = vec![
        CheckResult::Unknown {
            reason: "incomplete".into(),
        },
        CheckResult::Timeout {
            kind: TimeoutKind::Rlimit,
        },
        CheckResult::Sat { model: None },
        CheckResult::Unsat,
    ];
    let mut solver = ScriptedSolver::new(expected.clone());
    for want in expected {
        assert_eq!(
            solver.check_smtlib("(assert true)\n(check-sat)\n", &SolverBudget::default()),
            want
        );
    }
    assert_eq!(solver.remaining(), 0);
    assert_eq!(solver.queries.len(), 4);
    assert!(solver
        .queries
        .iter()
        .all(|q| q.smtlib == "(assert true)\n(check-sat)\n"));
    assert!(matches!(
        solver.check_smtlib("", &SolverBudget::default()),
        CheckResult::Unknown { .. }
    ));
}

#[test]
fn encoding_matches_bitvec_normative_vectors() {
    use ske_cir::bitvec::BitVec;
    let b = |w, n| BitVec::new(w, n as u128).unwrap();
    for (name, a, c, s) in [
        ("ADD", 127, 1, true),
        ("ADD", 200, 100, false),
        ("SUB", 0, 1, false),
        ("MUL", 64, 2, true),
        ("MUL", 16, 16, false),
        ("DIV", -128, -1, true),
        ("REM", -128, -1, true),
        ("DIV", -7, 2, true),
        ("REM", -7, 2, true),
        ("DIV", 7, -2, true),
        ("REM", 7, -2, true),
        ("BIT_AND", 0x55, 0x3c, false),
        ("BIT_OR", 0x55, 0x3c, false),
        ("XOR", 0x55, 0x3c, false),
        ("DIV", 255, 2, false),
        ("REM", 255, 2, false),
    ] {
        let want = b(8, a).binary(name, b(8, c), s).unwrap();
        let got = op(name, &[bv(8, a), bv(8, c)], &opts(s));
        prove_equal(&got.value, &Term::bitvec(8, want.bits()).unwrap());
        prove_equal(&got.normal_guard, &Term::bool(true));
    }
    prove_equal(
        &op("NEG", &[bv(8, -128)], &opts(true)).value,
        &Term::bitvec(8, (-b(8, -128)).bits()).unwrap(),
    );
    prove_equal(
        &op("BIT_NOT", &[bv(8, 0x55)], &opts(false)).value,
        &Term::bitvec(8, b(8, 0x55).bit_not().bits()).unwrap(),
    );
    for (a, c, cw, right, arith) in [
        (1, 7, 8, false, false),
        (1, 8, 8, false, false),
        (-128, 7, 8, true, true),
        (-128, 100, 8, true, true),
        (128, 7, 8, true, false),
        (128, 8, 8, true, false),
        (1, 256, 16, false, false),
        (1, 1, 1, false, false),
    ] {
        let got = op(
            if right { "SHR" } else { "SHL" },
            &[bv(8, a), bv(cw, c)],
            &opts(arith),
        );
        let want = b(8, a).shift(b(cw, c), false, right, arith).unwrap();
        prove_equal(&got.value, &Term::bitvec(8, want.bits()).unwrap());
    }
    for s in [true, false] {
        let want = b(8, -1).lt(b(8, 1), s).unwrap();
        prove_equal(
            &op("LT", &[bv(8, -1), bv(8, 1)], &opts(s)).value,
            &Term::bool(want),
        );
    }
    for (name, from, to, a, s) in [
        ("SIGN_EXTEND", 8, 16, -1, true),
        ("ZERO_EXTEND", 8, 16, 255, false),
        ("TRUNCATE", 16, 8, 300, false),
        ("BITCAST", 8, 8, -1, false),
    ] {
        let got = op(
            name,
            &[bv(from, a)],
            &OpOptions {
                result_sort: Some(Sort::BitVec(to)),
                ..opts(s)
            },
        );
        prove_equal(
            &got.value,
            &Term::bitvec(to, b(from, a).resize(to, s).unwrap().bits()).unwrap(),
        );
    }
    // Errors are guards, not SMT-LIB's totalized division/shift result.
    assert_eq!(
        b(8, 5).binary("DIV", b(8, 0), true),
        Err(ske_cir::bitvec::ArithmeticError::DivByZero)
    );
    for name in ["DIV", "REM"] {
        prove_equal(
            &op(name, &[bv(8, 5), bv(8, 0)], &opts(true)).normal_guard,
            &Term::bool(false),
        );
    }
    assert_eq!(
        b(8, 1).shift(b(8, -1), true, false, false),
        Err(ske_cir::bitvec::ArithmeticError::NegativeShift)
    );
    for name in ["SHL", "SHR"] {
        let options = OpOptions {
            count_signed: true,
            ..opts(true)
        };
        prove_equal(
            &op(name, &[bv(8, 1), bv(8, -1)], &options).normal_guard,
            &Term::bool(false),
        );
    }
}

#[test]
fn encoding_matches_bitvec_remaining_supported_opcodes_and_widths() {
    for width in [1, 7, 8, 16, 64, 127, 128] {
        let a = Term::bitvec(width, u128::MAX).unwrap();
        let b = Term::bitvec(width, 0).unwrap();
        prove_equal(
            &op("COPY", std::slice::from_ref(&a), &opts(false)).value,
            &a,
        );
        let inputs = vec![("left".into(), a.clone()), ("right".into(), b.clone())];
        prove_equal(&Encoder::phi("left", &inputs).unwrap().value, &a);
        prove_equal(&Encoder::phi("right", &inputs).unwrap().value, &b);
        prove_equal(
            &op(
                "SELECT",
                &[Term::bool(true), a.clone(), b.clone()],
                &opts(false),
            )
            .value,
            &a,
        );
        for s in [false, true] {
            let av = ske_cir::bitvec::BitVec::new(width, u128::MAX).unwrap();
            let bv = ske_cir::bitvec::BitVec::new(width, 0).unwrap();
            for name in ["EQ", "NE", "LT", "LE", "GT", "GE"] {
                let lt = av.lt(bv, s).unwrap();
                let gt = bv.lt(av, s).unwrap();
                let eq = av == bv;
                let want = match name {
                    "EQ" => eq,
                    "NE" => !eq,
                    "LT" => lt,
                    "LE" => lt || eq,
                    "GT" => gt,
                    _ => gt || eq,
                };
                prove_equal(
                    &op(name, &[a.clone(), b.clone()], &opts(s)).value,
                    &Term::bool(want),
                );
            }
        }
    }
    prove_equal(
        &op("LOGICAL_NOT", &[Term::bool(false)], &opts(false)).value,
        &Term::bool(true),
    );
    prove_equal(
        &op("EQ", &[Term::bool(true), Term::bool(false)], &opts(false)).value,
        &Term::bool(false),
    );
}

#[test]
fn encoding_matches_bitvec_boundary_arithmetic_and_shift_counts() {
    use ske_cir::bitvec::BitVec;
    for width in [1, 7, 64, 128] {
        for bits in [0, 1, 1u128 << (width - 1), u128::MAX] {
            let a = BitVec::new(width, bits).unwrap();
            for signed in [false, true] {
                for rhs in [1, u128::MAX] {
                    let b = BitVec::new(width, rhs).unwrap();
                    for name in [
                        "ADD", "SUB", "MUL", "DIV", "REM", "BIT_AND", "BIT_OR", "XOR",
                    ] {
                        let got = op(
                            name,
                            &[
                                Term::bitvec(width, a.bits()).unwrap(),
                                Term::bitvec(width, b.bits()).unwrap(),
                            ],
                            &opts(signed),
                        );
                        let want = a.binary(name, b, signed).unwrap();
                        prove_equal(&got.value, &Term::bitvec(width, want.bits()).unwrap());
                    }
                }
                for cw in [1, 8, 128] {
                    for count in [0, 1, width as u128, 256, u128::MAX] {
                        let c = BitVec::new(cw, count).unwrap();
                        for name in ["SHL", "SHR"] {
                            let got = op(
                                name,
                                &[
                                    Term::bitvec(width, a.bits()).unwrap(),
                                    Term::bitvec(cw, c.bits()).unwrap(),
                                ],
                                &opts(signed),
                            );
                            let want = a.shift(c, false, name == "SHR", signed).unwrap();
                            prove_equal(&got.value, &Term::bitvec(width, want.bits()).unwrap());
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn z3_uninterpreted_nil_and_typed_models() {
    let mut e = Encoder::new();
    let sort = Sort::Uninterpreted("go:*Thing/日本".into());
    let x = e.declare("x", sort.clone()).unwrap();
    let nil = e.nil(sort).unwrap();
    e.assert(&op("EQ", &[x.clone(), nil.clone()], &opts(false)).value)
        .unwrap();
    e.assert(&op("NE", &[x, nil], &opts(false)).value).unwrap();
    assert_eq!(
        Z3Solver::new().check(&e.finish(), &SolverBudget::default()),
        CheckResult::Unsat
    );
    let mut e = Encoder::new();
    let x = e.declare("wide", Sort::BitVec(128)).unwrap();
    let b = e.declare("boolean", Sort::Bool).unwrap();
    e.assert(
        &op(
            "EQ",
            &[x, Term::bitvec(128, u128::MAX).unwrap()],
            &opts(false),
        )
        .value,
    )
    .unwrap();
    e.assert(&b).unwrap();
    let CheckResult::Sat { model: Some(m) } =
        Z3Solver::new().check(&e.finish(), &SolverBudget::default())
    else {
        panic!("SAT")
    };
    assert_eq!(
        m["wide"],
        ModelValue::BitVec {
            width: 128,
            bits: u128::MAX
        }
    );
    assert_eq!(m["boolean"], ModelValue::Bool(true));
}

#[test]
fn encoding_negative_arity_types_options_identifiers() {
    for name in [
        "COPY",
        "SELECT",
        "ADD",
        "SUB",
        "MUL",
        "DIV",
        "REM",
        "NEG",
        "EQ",
        "NE",
        "LT",
        "LE",
        "GT",
        "GE",
        "BIT_AND",
        "BIT_OR",
        "XOR",
        "BIT_NOT",
        "SHL",
        "SHR",
        "LOGICAL_NOT",
        "SIGN_EXTEND",
        "ZERO_EXTEND",
        "TRUNCATE",
        "BITCAST",
    ] {
        assert!(Encoder::op(name, &[], &opts(false)).is_err(), "{name}");
        assert!(
            Encoder::op(name, &vec![Term::bool(true); 4], &opts(false)).is_err(),
            "{name}"
        );
    }
    for name in ["ADD", "DIV", "LT", "BIT_AND", "SHL"] {
        assert!(Encoder::op(name, &[Term::bool(true), Term::bool(false)], &opts(false)).is_err());
    }
    assert!(Encoder::op("ADD", &[bv(8, 1), bv(16, 1)], &opts(false)).is_err());
    assert!(Encoder::op(
        "ADD",
        &[bv(8, 1), bv(8, 1)],
        &OpOptions {
            overflow_wrap: false,
            ..Default::default()
        }
    )
    .is_err());
    assert!(Encoder::op("CAST", &[bv(8, 1)], &opts(false)).is_err());
    assert!(Encoder::phi("missing", &[("a".into(), bv(8, 1))]).is_err());
    assert!(Encoder::phi("a", &[("a".into(), bv(8, 1)), ("a".into(), bv(8, 2))]).is_err());
    assert!(Term::bitvec(0, 0).is_err());
    assert!(Term::bitvec(129, 0).is_err());
    let mut e = Encoder::new();
    assert!(e.declare("bad|name", Sort::Bool).is_err());
    assert!(e.declare("nil_S_61", Sort::Bool).is_err());
    e.declare("same", Sort::Bool).unwrap();
    assert!(e.declare("same", Sort::BitVec(8)).is_err());
    assert!(e.assert(&bv(8, 1)).is_err());
}

#[test]
fn fingerprint_exact_normative_concatenation() {
    use sha2::{Digest, Sha256};
    let budget = SolverBudget::default();
    let want: String = Sha256::digest(b"(assert true)|seed=0|rlimit=5000000")
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(query_fingerprint("(assert true)", &budget), want);
    assert_eq!(
        query_fingerprint(
            "(assert true)",
            &SolverBudget {
                wall_timeout_ms: 1,
                ..budget
            }
        ),
        want
    );
}
