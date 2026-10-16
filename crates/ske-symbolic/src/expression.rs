//! Structural interning and only the simplifications authorized by §6.2.
use ske_cir::bitvec::BitVec;
use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Sort {
    Bool,
    BitVec { width: u32, signed: bool },
    Uninterpreted,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Type {
    pub local_key: String,
    pub sort: Sort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExprId(pub usize);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Constant {
    Bool(bool),
    Bits(u128),
    Null,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Not,
    Neg,
    BitNot,
    /// Resize, sign-extending according to the operand's type.
    Cast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    BitAnd,
    BitOr,
    Xor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExprKind {
    Const(Constant),
    Symbol(String),
    Unary {
        op: UnaryOp,
        arg: ExprId,
    },
    Binary {
        op: BinaryOp,
        left: ExprId,
        right: ExprId,
    },
    Ite {
        condition: ExprId,
        then_expr: ExprId,
        else_expr: ExprId,
    },
    Field {
        base: ExprId,
        index: u32,
    },
    Uninterpreted {
        name: String,
        args: Vec<ExprId>,
    },
    Havoc(u64),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Node {
    pub ty: Type,
    pub kind: ExprKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprError {
    InvalidOperand(ExprId),
    InvalidType,
}

/// IDs are arena-local. Operands must already exist; nodes are immutable.
/// The entire Node is the map key, never its hash alone.
pub struct ExprArena<S = RandomState> {
    nodes: Vec<Node>,
    ids: HashMap<Node, ExprId, S>,
}

impl Default for ExprArena {
    fn default() -> Self {
        Self::new()
    }
}

impl ExprArena {
    pub fn new() -> Self {
        Self::with_hasher(RandomState::new())
    }
}

impl<S: BuildHasher> ExprArena<S> {
    pub fn with_hasher(hasher: S) -> Self {
        Self {
            nodes: Vec::new(),
            ids: HashMap::with_hasher(hasher),
        }
    }
    pub fn get(&self, id: ExprId) -> Option<&Node> {
        self.nodes.get(id.0)
    }
    pub fn iter(&self) -> impl Iterator<Item = (ExprId, &Node)> {
        self.nodes.iter().enumerate().map(|(i, n)| (ExprId(i), n))
    }
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn symbol(&mut self, ty: Type, name: impl Into<String>) -> Result<ExprId, ExprError> {
        self.intern(Node {
            ty,
            kind: ExprKind::Symbol(name.into()),
        })
    }
    pub fn constant(&mut self, ty: Type, value: Constant) -> Result<ExprId, ExprError> {
        self.intern(Node {
            ty,
            kind: ExprKind::Const(value),
        })
    }
    pub fn unary(&mut self, ty: Type, op: UnaryOp, arg: ExprId) -> Result<ExprId, ExprError> {
        self.intern(Node {
            ty,
            kind: ExprKind::Unary { op, arg },
        })
    }
    pub fn binary(
        &mut self,
        ty: Type,
        op: BinaryOp,
        left: ExprId,
        right: ExprId,
    ) -> Result<ExprId, ExprError> {
        self.intern(Node {
            ty,
            kind: ExprKind::Binary { op, left, right },
        })
    }
    pub fn ite(
        &mut self,
        condition: ExprId,
        then_expr: ExprId,
        else_expr: ExprId,
    ) -> Result<ExprId, ExprError> {
        let ty = self.operand(then_expr)?.ty.clone();
        self.intern(Node {
            ty,
            kind: ExprKind::Ite {
                condition,
                then_expr,
                else_expr,
            },
        })
    }
    /// The run owns the monotonically increasing counter (starting at zero).
    pub fn havoc(&mut self, ty: Type, n: u64) -> Result<ExprId, ExprError> {
        self.intern(Node {
            ty,
            kind: ExprKind::Havoc(n),
        })
    }

    pub fn intern(&mut self, node: Node) -> Result<ExprId, ExprError> {
        self.validate(&node)?;
        if let Some(id) = self.simplify(&node) {
            return Ok(id);
        }
        if let Some(value) = self.fold(&node) {
            return self.intern(Node {
                ty: node.ty,
                kind: ExprKind::Const(value),
            });
        }
        if let Some(id) = self.ids.get(&node) {
            return Ok(*id);
        }
        let id = ExprId(self.nodes.len());
        self.nodes.push(node.clone());
        self.ids.insert(node, id);
        Ok(id)
    }

    fn operand(&self, id: ExprId) -> Result<&Node, ExprError> {
        self.get(id).ok_or(ExprError::InvalidOperand(id))
    }

    fn validate(&self, n: &Node) -> Result<(), ExprError> {
        use ExprKind::*;
        let invalid = || Err(ExprError::InvalidType);
        if matches!(n.ty.sort, Sort::BitVec { width, .. } if !(1..=128).contains(&width)) {
            return invalid();
        }
        match &n.kind {
            Const(c) => match (&n.ty.sort, c) {
                (Sort::Bool, Constant::Bool(_)) | (Sort::Uninterpreted, Constant::Null) => (),
                (Sort::BitVec { width, .. }, Constant::Bits(bits))
                    if *bits <= u128::MAX >> (128 - width) => {}
                _ => return invalid(),
            },
            Unary { op, arg } => {
                let a = self.operand(*arg)?;
                match op {
                    UnaryOp::Not if a.ty == n.ty && n.ty.sort == Sort::Bool => (),
                    UnaryOp::Neg | UnaryOp::BitNot
                        if a.ty == n.ty && matches!(n.ty.sort, Sort::BitVec { .. }) => {}
                    UnaryOp::Cast
                        if matches!(a.ty.sort, Sort::BitVec { .. })
                            && matches!(n.ty.sort, Sort::BitVec { .. }) => {}
                    _ => return invalid(),
                }
            }
            Binary { op, left, right } => {
                let a = self.operand(*left)?;
                let b = self.operand(*right)?;
                use BinaryOp::*;
                let ok = match op {
                    And | Or => a.ty == b.ty && a.ty == n.ty && n.ty.sort == Sort::Bool,
                    Eq | Ne => a.ty == b.ty && n.ty.sort == Sort::Bool,
                    Lt | Le | Gt | Ge => {
                        a.ty == b.ty
                            && n.ty.sort == Sort::Bool
                            && matches!(a.ty.sort, Sort::BitVec { .. })
                    }
                    Shl | Shr => {
                        a.ty == n.ty
                            && matches!(a.ty.sort, Sort::BitVec { .. })
                            && matches!(b.ty.sort, Sort::BitVec { .. })
                    }
                    _ => a.ty == b.ty && a.ty == n.ty && matches!(n.ty.sort, Sort::BitVec { .. }),
                };
                if !ok {
                    return invalid();
                }
            }
            Ite {
                condition,
                then_expr,
                else_expr,
            } => {
                if self.operand(*condition)?.ty.sort != Sort::Bool
                    || self.operand(*then_expr)?.ty != n.ty
                    || self.operand(*else_expr)?.ty != n.ty
                {
                    return invalid();
                }
            }
            Field { base, .. } => {
                self.operand(*base)?;
            }
            Uninterpreted { args, .. } => {
                for id in args {
                    self.operand(*id)?;
                }
            }
            Symbol(_) | Havoc(_) => (),
        }
        Ok(())
    }

    fn boolean(&self, id: ExprId) -> Option<bool> {
        match self.get(id)?.kind {
            ExprKind::Const(Constant::Bool(b)) => Some(b),
            _ => None,
        }
    }

    fn simplify(&self, n: &Node) -> Option<ExprId> {
        match n.kind {
            ExprKind::Unary {
                op: UnaryOp::Not,
                arg,
            } => match self.get(arg)?.kind {
                ExprKind::Unary {
                    op: UnaryOp::Not,
                    arg: inner,
                } => Some(inner),
                _ => None,
            },
            ExprKind::Ite {
                condition,
                then_expr,
                else_expr,
            } => match self.boolean(condition) {
                Some(true) => Some(then_expr),
                Some(false) => Some(else_expr),
                None if then_expr == else_expr => Some(then_expr),
                None => None,
            },
            ExprKind::Binary {
                op: BinaryOp::And,
                left,
                right,
            } => match (self.boolean(left), self.boolean(right)) {
                (Some(false), _) | (Some(true), _) => {
                    if self.boolean(left)? {
                        Some(right)
                    } else {
                        Some(left)
                    }
                }
                (_, Some(b)) => Some(if b { left } else { right }),
                _ => None,
            },
            ExprKind::Binary {
                op: BinaryOp::Or,
                left,
                right,
            } => match (self.boolean(left), self.boolean(right)) {
                (Some(b), _) => Some(if b { left } else { right }),
                (_, Some(b)) => Some(if b { right } else { left }),
                _ => None,
            },
            _ => None,
        }
    }

    fn bitvec(&self, id: ExprId) -> Option<(BitVec, bool)> {
        let n = self.get(id)?;
        match (&n.ty.sort, &n.kind) {
            (Sort::BitVec { width, signed }, ExprKind::Const(Constant::Bits(bits))) => {
                Some((BitVec::new(*width, *bits).ok()?, *signed))
            }
            _ => None,
        }
    }

    fn fold(&self, n: &Node) -> Option<Constant> {
        use BinaryOp::*;
        use Constant::*;
        match n.kind {
            ExprKind::Unary { op, arg } => {
                if op == UnaryOp::Not {
                    return Some(Bool(!self.boolean(arg)?));
                }
                let (a, signed) = self.bitvec(arg)?;
                let value = match op {
                    UnaryOp::Neg => -a,
                    UnaryOp::BitNot => a.bit_not(),
                    UnaryOp::Cast => {
                        let Sort::BitVec { width, .. } = n.ty.sort else {
                            return None;
                        };
                        a.resize(width, signed).ok()?
                    }
                    _ => return None,
                };
                Some(Bits(value.bits()))
            }
            ExprKind::Binary { op, left, right } => {
                if let (Some(a), Some(b)) = (self.boolean(left), self.boolean(right)) {
                    return Some(Bool(match op {
                        Eq => a == b,
                        Ne => a != b,
                        And => a && b,
                        Or => a || b,
                        _ => return None,
                    }));
                }
                if matches!(op, Eq | Ne)
                    && matches!(self.get(left)?.kind, ExprKind::Const(Null))
                    && matches!(self.get(right)?.kind, ExprKind::Const(Null))
                {
                    return Some(Bool(op == Eq));
                }
                let (a, signed) = self.bitvec(left)?;
                let (b, count_signed) = self.bitvec(right)?;
                let arithmetic = match op {
                    Add => "ADD",
                    Sub => "SUB",
                    Mul => "MUL",
                    Div => "DIV",
                    Rem => "REM",
                    BitAnd => "BIT_AND",
                    BitOr => "BIT_OR",
                    Xor => "XOR",
                    Shl | Shr => {
                        return Some(Bits(
                            a.shift(b, count_signed, op == Shr, op == Shr && signed)
                                .ok()?
                                .bits(),
                        ))
                    }
                    Eq => return Some(Bool(a == b)),
                    Ne => return Some(Bool(a != b)),
                    Lt => return Some(Bool(a.lt(b, signed).ok()?)),
                    Le => return Some(Bool(!b.lt(a, signed).ok()?)),
                    Gt => return Some(Bool(b.lt(a, signed).ok()?)),
                    Ge => return Some(Bool(!a.lt(b, signed).ok()?)),
                    _ => return None,
                };
                Some(Bits(a.binary(arithmetic, b, signed).ok()?.bits()))
            }
            _ => None,
        }
    }
}
