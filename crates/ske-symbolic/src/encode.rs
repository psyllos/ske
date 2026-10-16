//! Deterministic SMT-LIB generation independent of the solver backend.
use crate::expression::{BinaryOp, Constant, ExprArena, ExprId, ExprKind, Sort, UnaryOp};
use std::collections::{BTreeMap, BTreeSet};

fn quoted(s: &str) -> String {
    // Encode bytes rather than stripping punctuation: local keys must never collide.
    format!(
        "s{}",
        s.as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

pub fn symbol_name(s: &str) -> String {
    quoted(s)
}

pub fn encode(arena: &ExprArena, constraints: &[ExprId]) -> Result<String, String> {
    fn visit(
        arena: &ExprArena,
        id: ExprId,
        seen: &mut BTreeSet<usize>,
        order: &mut Vec<ExprId>,
    ) -> Result<(), String> {
        if !seen.insert(id.0) {
            return Ok(());
        }
        let node = arena.get(id).ok_or("invalid expression ID")?;
        let children = match &node.kind {
            ExprKind::Unary { arg, .. } => vec![*arg],
            ExprKind::Binary { left, right, .. } => vec![*left, *right],
            ExprKind::Ite {
                condition,
                then_expr,
                else_expr,
            } => vec![*condition, *then_expr, *else_expr],
            ExprKind::Field { base, .. } => vec![*base],
            ExprKind::Uninterpreted { args, .. } => args.clone(),
            _ => vec![],
        };
        for child in children {
            visit(arena, child, seen, order)?;
        }
        order.push(id);
        Ok(())
    }
    let mut order = Vec::new();
    let mut seen = BTreeSet::new();
    for id in constraints {
        visit(arena, *id, &mut seen, &mut order)?;
    }
    let sort = |id: ExprId| -> Result<String, String> {
        let n = arena.get(id).ok_or("invalid expression ID")?;
        Ok(match n.ty.sort {
            Sort::Bool => "Bool".into(),
            Sort::BitVec { width, .. } => format!("(_ BitVec {width})"),
            Sort::Uninterpreted => format!("S_{}", &quoted(&n.ty.local_key)[1..]),
        })
    };
    let mut sorts = BTreeSet::new();
    let mut declarations = BTreeMap::new();
    let mut definitions = String::new();
    for id in order {
        let n = arena.get(id).ok_or("invalid expression ID")?;
        let s = sort(id)?;
        if n.ty.sort == Sort::Uninterpreted {
            sorts.insert(s.clone());
        }
        let e = |x: ExprId| format!("e{}", x.0);
        let text = match &n.kind {
            ExprKind::Const(Constant::Bool(b)) => b.to_string(),
            ExprKind::Const(Constant::Bits(bits)) => {
                let Sort::BitVec { width, .. } = n.ty.sort else {
                    return Err("bits without bitvector type".into());
                };
                format!("(_ bv{bits} {width})")
            }
            ExprKind::Const(Constant::Null) => {
                let name = format!("nil_{s}");
                declarations.insert(name.clone(), format!("(declare-fun {name} () {s})\n"));
                name
            }
            ExprKind::Symbol(name) => {
                let name = quoted(name);
                declarations.insert(name.clone(), format!("(declare-fun {name} () {s})\n"));
                name
            }
            ExprKind::Havoc(number) => {
                let name = format!("havoc_{number}");
                declarations.insert(name.clone(), format!("(declare-fun {name} () {s})\n"));
                name
            }
            ExprKind::Unary { op, arg } => {
                let a = arena.get(*arg).ok_or("invalid unary argument")?;
                match op {
                    UnaryOp::Not => format!("(not {})", e(*arg)),
                    UnaryOp::Neg => format!("(bvneg {})", e(*arg)),
                    UnaryOp::BitNot => format!("(bvnot {})", e(*arg)),
                    UnaryOp::Cast => {
                        let (Sort::BitVec { width: src, signed }, Sort::BitVec { width: dst, .. }) =
                            (&a.ty.sort, &n.ty.sort)
                        else {
                            return Err("non-bitvector cast".into());
                        };
                        if src == dst {
                            e(*arg)
                        } else if src > dst {
                            format!("((_ extract {} 0) {})", dst - 1, e(*arg))
                        } else {
                            format!(
                                "((_ {} {}) {})",
                                if *signed {
                                    "sign_extend"
                                } else {
                                    "zero_extend"
                                },
                                dst - src,
                                e(*arg)
                            )
                        }
                    }
                }
            }
            ExprKind::Binary { op, left, right } => {
                let left_node = arena.get(*left).ok_or("invalid binary argument")?;
                let signed = matches!(left_node.ty.sort, Sort::BitVec { signed: true, .. });
                let operator = match op {
                    BinaryOp::Add => "bvadd",
                    BinaryOp::Sub => "bvsub",
                    BinaryOp::Mul => "bvmul",
                    BinaryOp::Div => {
                        if signed {
                            "bvsdiv"
                        } else {
                            "bvudiv"
                        }
                    }
                    BinaryOp::Rem => {
                        if signed {
                            "bvsrem"
                        } else {
                            "bvurem"
                        }
                    }
                    BinaryOp::BitAnd => "bvand",
                    BinaryOp::BitOr => "bvor",
                    BinaryOp::Xor => "bvxor",
                    BinaryOp::Eq | BinaryOp::Ne => "=",
                    BinaryOp::And => "and",
                    BinaryOp::Or => "or",
                    BinaryOp::Lt => {
                        if signed {
                            "bvslt"
                        } else {
                            "bvult"
                        }
                    }
                    BinaryOp::Le => {
                        if signed {
                            "bvsle"
                        } else {
                            "bvule"
                        }
                    }
                    BinaryOp::Gt => {
                        if signed {
                            "bvsgt"
                        } else {
                            "bvugt"
                        }
                    }
                    BinaryOp::Ge => {
                        if signed {
                            "bvsge"
                        } else {
                            "bvuge"
                        }
                    }
                    BinaryOp::Shl => "bvshl",
                    BinaryOp::Shr => {
                        if signed {
                            "bvashr"
                        } else {
                            "bvlshr"
                        }
                    }
                };
                if matches!(op, BinaryOp::Shl | BinaryOp::Shr) {
                    let right_node = arena.get(*right).ok_or("invalid shift count")?;
                    let (
                        Sort::BitVec { width, .. },
                        Sort::BitVec {
                            width: count_width, ..
                        },
                    ) = (&left_node.ty.sort, &right_node.ty.sort)
                    else {
                        return Err("non-bitvector shift".into());
                    };
                    // Compare untruncated count before adapting its width.
                    let count = if count_width == width {
                        e(*right)
                    } else if count_width > width {
                        format!("((_ extract {} 0) {})", width - 1, e(*right))
                    } else {
                        format!("((_ zero_extend {}) {})", width - count_width, e(*right))
                    };
                    let shifted = format!("({operator} {} {count})", e(*left));
                    if *count_width < 32 && (1u64 << count_width) <= u64::from(*width) {
                        shifted
                    } else {
                        let fill = if matches!(op, BinaryOp::Shr) && signed {
                            format!("(bvashr {} (_ bv{} {width}))", e(*left), width - 1)
                        } else {
                            format!("(_ bv0 {width})")
                        };
                        format!(
                            "(ite (bvuge {} (_ bv{width} {count_width})) {fill} {shifted})",
                            e(*right)
                        )
                    }
                } else {
                    let expression = format!("({operator} {} {})", e(*left), e(*right));
                    if matches!(op, BinaryOp::Ne) {
                        format!("(not {expression})")
                    } else {
                        expression
                    }
                }
            }
            ExprKind::Ite {
                condition,
                then_expr,
                else_expr,
            } => format!(
                "(ite {} {} {})",
                e(*condition),
                e(*then_expr),
                e(*else_expr)
            ),
            ExprKind::Field { base, index } => {
                let name = format!(
                    "field_{}_{}",
                    quoted(&arena.get(*base).ok_or("invalid field")?.ty.local_key),
                    index
                );
                declarations.insert(
                    name.clone(),
                    format!("(declare-fun {name} ({}) {s})\n", sort(*base)?),
                );
                format!("({name} {})", e(*base))
            }
            ExprKind::Uninterpreted { name, args } => {
                let name = format!("uf_{}", quoted(name));
                let types = args
                    .iter()
                    .map(|a| sort(*a))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(" ");
                declarations.insert(
                    name.clone(),
                    format!("(declare-fun {name} ({types}) {s})\n"),
                );
                if args.is_empty() {
                    name
                } else {
                    format!(
                        "({name} {})",
                        args.iter().map(|a| e(*a)).collect::<Vec<_>>().join(" ")
                    )
                }
            }
        };
        definitions.push_str(&format!("(define-fun e{} () {s} {text})\n", id.0));
    }
    let mut text = "(set-logic ALL)\n(set-option :produce-models true)\n".to_owned();
    for s in sorts {
        text.push_str(&format!("(declare-sort {s} 0)\n"));
    }
    for declaration in declarations.values() {
        text.push_str(declaration);
    }
    text.push_str(&definitions);
    for id in constraints {
        text.push_str(&format!("(assert e{})\n", id.0));
    }
    text.push_str("(check-sat)\n");
    Ok(text)
}

pub fn query_fingerprint(text: &str, seed: u32, rlimit: u64) -> String {
    ske_common::sha256_hex(format!("{text}|seed={seed}|rlimit={rlimit}").as_bytes())
}
