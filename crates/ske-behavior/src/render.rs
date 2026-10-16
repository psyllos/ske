//! Deterministic guard/value rendering. Rules R1–R6 of §7.2 (normative).
//!
//! - R1: guard = conjunction of constraints in path order; no algebraic
//!   simplification, including repeated constraints.
//! - R2: negation is pushed onto comparisons and bare booleans.
//! - R3: integer constants render in decimal per type signedness.
//! - R4: parameters render as their ValueDef.name (else `p<index>`); globals as
//!   their short name; otherwise `<expr:N>`.
//! - R5: ` AND `/` OR ` with parentheses around conjunctions inside disjunctions.
//! - R6: infix arithmetic with parentheses around every nested `BINARY`.
use ske_symbolic::{BinaryOp, Constant, ExprArena, ExprId, ExprKind, Sort, UnaryOp};
use std::collections::BTreeMap;
type Memo = BTreeMap<(ExprId, bool), String>;

/// Symbol-name resolution. The caller maps full persisted symbol names to
/// display names (parameter ValueDef.name, global short name). Symbols left
/// unresolved follow the R4 fallback rules.
#[derive(Clone, Debug, Default)]
pub struct Names {
    by_symbol: BTreeMap<String, String>,
}

impl Names {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, symbol: impl Into<String>, display: impl Into<String>) -> &mut Self {
        self.by_symbol.insert(symbol.into(), display.into());
        self
    }
    pub fn get(&self, symbol: &str) -> Option<&str> {
        self.by_symbol.get(symbol).map(String::as_str)
    }
}

/// Render a guard: the conjunction of terms in path order (R1).
pub fn render_guard(arena: &ExprArena, terms: &[ExprId], names: &Names) -> String {
    if terms.is_empty() {
        return "true".into();
    }
    let mut parts = Vec::new();
    for term in terms {
        let mut text = render_top(arena, *term, names, true);
        if terms.len() > 1
            && matches!(
                arena.get(*term).map(|n| &n.kind),
                Some(ExprKind::Binary {
                    op: BinaryOp::Or,
                    ..
                })
            )
        {
            text = format!("({text})");
        }
        parts.push(text);
    }
    parts.join(" AND ")
}

/// Render a single value expression (a `RETURN`/`PANIC` operand).
pub fn render_value(arena: &ExprArena, id: ExprId, names: &Names) -> String {
    render_top(arena, id, names, false)
}

/// A guard root that is itself a bare boolean renders `b == true` (R2); a value
/// root renders the parameter/name plainly.
fn render_top(arena: &ExprArena, id: ExprId, names: &Names, guard: bool) -> String {
    let mut counter = 0;
    render_expr(arena, id, names, guard, &mut counter, &mut BTreeMap::new())
}

fn render_expr(
    arena: &ExprArena,
    id: ExprId,
    names: &Names,
    root: bool,
    counter: &mut u64,
    memo: &mut Memo,
) -> String {
    if let Some(text) = memo.get(&(id, root)) {
        return text.clone();
    }
    let node = match arena.get(id) {
        Some(node) => node,
        None => return String::from("<expr:invalid>"),
    };
    let rendered = match &node.kind {
        ExprKind::Const(Constant::Bool(b)) => b.to_string(),
        ExprKind::Const(Constant::Null) => "nil".to_owned(),
        ExprKind::Const(Constant::Bits(bits)) => {
            let (width, signed) = match node.ty.sort {
                Sort::BitVec { width, signed } => (width, signed),
                _ => (0, false),
            };
            render_bits(*bits, width, signed)
        }
        ExprKind::Symbol(name) => render_symbol(name, node, names, root, counter),
        ExprKind::Havoc(n) => render_unresolved(*n, counter),
        ExprKind::Unary { op, arg } => render_unary(arena, *op, *arg, node, names, counter, memo),
        ExprKind::Binary { op, left, right } => {
            render_binary(arena, *op, *left, *right, names, counter, memo)
        }
        ExprKind::Ite {
            condition,
            then_expr,
            else_expr,
        } => render_ite(
            arena, *condition, *then_expr, *else_expr, names, counter, memo,
        ),
        ExprKind::Field { .. } => render_unresolved(id.0 as u64, counter),
        ExprKind::Uninterpreted { .. } => render_unresolved(id.0 as u64, counter),
    };
    memo.insert((id, root), rendered.clone());
    rendered
}

fn render_bits(bits: u128, width: u32, signed: bool) -> String {
    if signed {
        if width < 128 && bits & (1_u128 << (width - 1)) != 0 {
            return ((bits as i128) - (1_i128 << width)).to_string();
        }
        if width == 128 && bits >> 127 != 0 {
            return (bits as i128).to_string();
        }
    }
    bits.to_string()
}

fn render_symbol(
    name: &str,
    node: &ske_symbolic::Node,
    names: &Names,
    bare_bool_as_guard: bool,
    counter: &mut u64,
) -> String {
    let display = names.get(name).map(str::to_owned);
    let base = match display {
        Some(display) => display,
        None => {
            if let Some(rest) = name.strip_prefix("param:") {
                if let Some(index) = rest.rsplit(':').next() {
                    format!("p{index}")
                } else {
                    name.to_owned()
                }
            } else {
                return render_unresolved_counter(counter);
            }
        }
    };
    if bare_bool_as_guard && node.ty.sort == Sort::Bool {
        format!("{base} == true")
    } else {
        base
    }
}

fn render_unresolved(cooked: u64, counter: &mut u64) -> String {
    let _ = cooked;
    render_unresolved_counter(counter)
}

fn render_unresolved_counter(counter: &mut u64) -> String {
    let text = format!("<expr:{counter}>");
    *counter += 1;
    text
}

#[allow(clippy::too_many_arguments)]
fn render_unary(
    arena: &ExprArena,
    op: UnaryOp,
    arg: ExprId,
    node: &ske_symbolic::Node,
    names: &Names,
    counter: &mut u64,
    memo: &mut Memo,
) -> String {
    if op == UnaryOp::Not {
        // R2: NOT on a bare boolean -> `b == false`; NOT on a comparison pushes
        // the negated comparison; anything else keeps NOT(...).
        let arg_node = match arena.get(arg) {
            Some(n) => n,
            None => return "<expr:invalid>".to_owned(),
        };
        if arg_node.ty.sort == Sort::Bool {
            if let ExprKind::Symbol(name) = &arg_node.kind {
                let inner = render_symbol(name, arg_node, names, false, counter);
                return format!("{inner} == false");
            }
        }
        if let ExprKind::Binary {
            op: bop,
            left,
            right,
        } = &arg_node.kind
        {
            if matches!(
                bop,
                BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
            ) {
                let l = render_operand(arena, *left, names, counter, memo);
                let r = render_operand(arena, *right, names, counter, memo);
                return format!("{l} {} {r}", negate_op(*bop));
            }
        }
        let inner = render_expr(arena, arg, names, false, counter, memo);
        return format!("NOT({inner})");
    }
    let inner = render_operand(arena, arg, names, counter, memo);
    match op {
        UnaryOp::Not => unreachable!(),
        UnaryOp::Neg => {
            let _ = node;
            format!("-{inner}")
        }
        UnaryOp::BitNot => format!("~{inner}"),
        UnaryOp::Cast => inner,
    }
}

fn negate_op(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Lt => ">=",
        BinaryOp::Le => ">",
        BinaryOp::Gt => "<=",
        BinaryOp::Ge => "<",
        BinaryOp::Eq => "!=",
        BinaryOp::Ne => "==",
        _ => "??",
    }
}

/// Render an operand. Per R6 a `BINARY` operand is parenthesized.
fn render_operand(
    arena: &ExprArena,
    id: ExprId,
    names: &Names,
    counter: &mut u64,
    memo: &mut Memo,
) -> String {
    let text = render_expr(arena, id, names, false, counter, memo);
    let node = arena.get(id);
    let parenthesize = node.is_some_and(|n| {
        matches!(n.kind, ExprKind::Binary { .. })
            || matches!(
                n.kind,
                ExprKind::Unary {
                    op: UnaryOp::Neg,
                    ..
                }
            )
    });
    if parenthesize {
        format!("({text})")
    } else {
        text
    }
}

fn render_binary(
    arena: &ExprArena,
    op: BinaryOp,
    left: ExprId,
    right: ExprId,
    names: &Names,
    counter: &mut u64,
    memo: &mut Memo,
) -> String {
    let operator = match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Rem => "%",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::Xor => "^",
        BinaryOp::Shl => "<<",
        BinaryOp::Shr => ">>",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::And => " AND ",
        BinaryOp::Or => " OR ",
    };
    if matches!(op, BinaryOp::And | BinaryOp::Or) {
        let l = render_logical_operand(arena, left, names, counter, memo, op);
        let r = render_logical_operand(arena, right, names, counter, memo, op);
        return format!("{l}{operator}{r}");
    }
    let l = render_operand(arena, left, names, counter, memo);
    let r = render_operand(arena, right, names, counter, memo);
    format!("{l} {operator} {r}")
}

/// R5: a conjunction operand inside a disjunction is parenthesized, and vice
/// versa only when that produces the required grouping.
fn render_logical_operand(
    arena: &ExprArena,
    id: ExprId,
    names: &Names,
    counter: &mut u64,
    memo: &mut Memo,
    parent_op: BinaryOp,
) -> String {
    let text = render_expr(arena, id, names, true, counter, memo);
    let is_conj = matches!(
        arena.get(id).map(|n| &n.kind),
        Some(ExprKind::Binary {
            op: BinaryOp::And,
            ..
        })
    );
    let is_disj = matches!(
        arena.get(id).map(|n| &n.kind),
        Some(ExprKind::Binary {
            op: BinaryOp::Or,
            ..
        })
    );
    // A disjunction inside a conjunction needs parens; a conjunction inside a
    // disjunction with more than one atom also needs parens. We conservatively
    // parenthesize the cross-boundary case.
    if (parent_op == BinaryOp::And && is_disj) || (parent_op == BinaryOp::Or && is_conj) {
        format!("({text})")
    } else {
        text
    }
}

fn render_ite(
    _arena: &ExprArena,
    condition: ExprId,
    then_expr: ExprId,
    else_expr: ExprId,
    _names: &Names,
    counter: &mut u64,
    _memo: &mut Memo,
) -> String {
    let _ = condition;
    let _ = then_expr;
    let _ = else_expr;
    // §7.2 rendering rules do not define ITE (added by interval/loop abstraction
    // only). Render an opaque deterministic placeholder rather than inventing a
    // business interpretation.
    render_unresolved_counter(counter)
}
