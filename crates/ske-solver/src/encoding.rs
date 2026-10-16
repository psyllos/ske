//! Typed SMT-LIB fragments, with explicit normal/panic guards.
//! No expression arena or backend objects cross this API.
use crate::Query;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Sort {
    Bool,
    BitVec(u32),
    Uninterpreted(String),
}
impl Sort {
    pub fn validate(&self) -> Result<(), EncodeError> {
        match self {
            Self::BitVec(w) if !(1..=128).contains(w) => {
                Err(EncodeError("bit width must be 1..=128".into()))
            }
            Self::Uninterpreted(n) if n.is_empty() => {
                Err(EncodeError("empty type identifier".into()))
            }
            _ => Ok(()),
        }
    }
    /// Injective cleaning: hex-encoded UTF-8 avoids collisions between type IDs.
    pub fn smt(&self) -> String {
        match self {
            Self::Bool => "Bool".into(),
            Self::BitVec(w) => format!("(_ BitVec {w})"),
            Self::Uninterpreted(id) => format!(
                "S_{}",
                id.as_bytes()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            ),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variable {
    pub name: String,
    pub sort: Sort,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Term {
    pub smt: String,
    pub sort: Sort,
}
impl Term {
    pub fn bool(value: bool) -> Self {
        Self {
            smt: value.to_string(),
            sort: Sort::Bool,
        }
    }
    pub fn bitvec(width: u32, bits: u128) -> Result<Self, EncodeError> {
        Sort::BitVec(width).validate()?;
        let bits = bits & (u128::MAX >> (128 - width));
        Ok(Self {
            smt: format!("(_ bv{bits} {width})"),
            sort: Sort::BitVec(width),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodeError(pub String);
impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for EncodeError {}
#[derive(Clone, Debug)]
pub struct OpOptions {
    pub signed: bool,
    pub count_signed: bool,
    pub overflow_wrap: bool,
    pub result_sort: Option<Sort>,
}
impl Default for OpOptions {
    fn default() -> Self {
        Self {
            signed: false,
            count_signed: false,
            overflow_wrap: true,
            result_sort: None,
        }
    }
}
#[derive(Clone, Debug)]
pub struct EncodedOp {
    pub value: Term,
    /// Assert for a normal execution edge; its negation guards PANIC.
    pub normal_guard: Term,
}
#[derive(Default)]
pub struct Encoder {
    variables: BTreeMap<String, Sort>,
    nil_sorts: BTreeMap<String, Sort>,
    assertions: Vec<String>,
}
/// Quote every identifier; reject characters SMT-LIB quoted symbols cannot contain.
pub fn symbol(name: &str) -> Result<String, EncodeError> {
    if name.is_empty()
        || name
            .chars()
            .any(|c| c == '|' || c == '\\' || c.is_control())
    {
        return Err(EncodeError("invalid SMT identifier".into()));
    }
    Ok(format!("|{name}|"))
}
impl Encoder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn declare(&mut self, name: &str, sort: Sort) -> Result<Term, EncodeError> {
        sort.validate()?;
        let smt = symbol(name)?;
        if name.starts_with("nil_") {
            return Err(EncodeError("nil_ is a reserved namespace".into()));
        }
        if let Some(old) = self.variables.get(name) {
            if old != &sort {
                return Err(EncodeError("conflicting variable sort".into()));
            }
        }
        self.variables.insert(name.into(), sort.clone());
        Ok(Term { smt, sort })
    }
    pub fn nil(&mut self, sort: Sort) -> Result<Term, EncodeError> {
        sort.validate()?;
        if !matches!(sort, Sort::Uninterpreted(_)) {
            return Err(EncodeError("nil requires uninterpreted sort".into()));
        }
        let name = format!("nil_{}", sort.smt());
        self.nil_sorts.insert(name.clone(), sort.clone());
        Ok(Term {
            smt: symbol(&name)?,
            sort,
        })
    }
    pub fn assert(&mut self, term: &Term) -> Result<(), EncodeError> {
        if term.sort != Sort::Bool {
            return Err(EncodeError("assert requires Bool".into()));
        }
        self.assertions.push(term.smt.clone());
        Ok(())
    }
    pub fn finish(self) -> Query {
        let mut text = String::from("(set-logic QF_UFBV)\n");
        let mut sorts = std::collections::BTreeSet::new();
        for s in self.variables.values().chain(self.nil_sorts.values()) {
            if matches!(s, Sort::Uninterpreted(_)) {
                sorts.insert(s.smt());
            }
        }
        for s in sorts {
            text.push_str(&format!("(declare-sort {s} 0)\n"));
        }
        // nil is a distinct symbol namespace from user variables.
        for (n, s) in &self.nil_sorts {
            text.push_str(&format!(
                "(declare-const {} {})\n",
                symbol(n).unwrap(),
                s.smt()
            ));
        }
        for (n, s) in &self.variables {
            text.push_str(&format!(
                "(declare-const {} {})\n",
                symbol(n).unwrap(),
                s.smt()
            ));
        }
        for a in &self.assertions {
            text.push_str(&format!("(assert {a})\n"));
        }
        text.push_str("(check-sat)\n");
        Query {
            smtlib: text,
            variables: self
                .variables
                .into_iter()
                .map(|(name, sort)| Variable { name, sort })
                .collect(),
        }
    }
    /// PHI must be selected by predecessor before calling this: `phi` below.
    pub fn op(op: &str, args: &[Term], opts: &OpOptions) -> Result<EncodedOp, EncodeError> {
        let err = || EncodeError(format!("invalid arity/type/options for {op}"));
        for a in args {
            a.sort.validate()?;
        }
        let n = match op {
            "COPY" | "NEG" | "BIT_NOT" | "LOGICAL_NOT" | "SIGN_EXTEND" | "ZERO_EXTEND"
            | "TRUNCATE" | "BITCAST" => 1,
            "SELECT" => 3,
            "ADD" | "SUB" | "MUL" | "DIV" | "REM" | "EQ" | "NE" | "LT" | "LE" | "GT" | "GE"
            | "BIT_AND" | "BIT_OR" | "XOR" | "SHL" | "SHR" => 2,
            _ => return Err(EncodeError(format!("unsupported opcode {op}"))),
        };
        if args.len() != n {
            return Err(err());
        }
        let a = &args[0];
        let mut guard = Term::bool(true);
        let (smt, sort) = if op == "COPY" {
            (a.smt.clone(), a.sort.clone())
        } else if op == "SELECT" {
            if a.sort != Sort::Bool || args[1].sort != args[2].sort {
                return Err(err());
            }
            (
                format!("(ite {} {} {})", a.smt, args[1].smt, args[2].smt),
                args[1].sort.clone(),
            )
        } else if op == "LOGICAL_NOT" {
            if a.sort != Sort::Bool {
                return Err(err());
            }
            (format!("(not {})", a.smt), Sort::Bool)
        } else if op == "EQ" || op == "NE" {
            if a.sort != args[1].sort {
                return Err(err());
            }
            let eq = format!("(= {} {})", a.smt, args[1].smt);
            (
                if op == "NE" {
                    format!("(not {eq})")
                } else {
                    eq
                },
                Sort::Bool,
            )
        } else {
            let Sort::BitVec(w) = a.sort else {
                return Err(err());
            };
            if ["SIGN_EXTEND", "ZERO_EXTEND", "TRUNCATE", "BITCAST"].contains(&op) {
                let Some(Sort::BitVec(to)) = opts.result_sort else {
                    return Err(err());
                };
                Sort::BitVec(to).validate()?;
                let x = match op {
                    "SIGN_EXTEND" | "ZERO_EXTEND" if to > w => format!(
                        "((_ {} {}) {})",
                        if op == "SIGN_EXTEND" {
                            "sign_extend"
                        } else {
                            "zero_extend"
                        },
                        to - w,
                        a.smt
                    ),
                    "TRUNCATE" if to < w => format!("((_ extract {} 0) {})", to - 1, a.smt),
                    "BITCAST" if to == w => a.smt.clone(),
                    _ => return Err(err()),
                };
                (x, Sort::BitVec(to))
            } else if op == "NEG" || op == "BIT_NOT" {
                if op == "NEG" && !opts.overflow_wrap {
                    return Err(EncodeError("UNSUPPORTED_OVERFLOW_MODE".into()));
                }
                (
                    format!(
                        "({} {})",
                        if op == "NEG" { "bvneg" } else { "bvnot" },
                        a.smt
                    ),
                    a.sort.clone(),
                )
            } else if op == "SHL" || op == "SHR" {
                let Sort::BitVec(cw) = args[1].sort else {
                    return Err(err());
                };
                let c = &args[1].smt;
                if opts.count_signed {
                    guard = Term {
                        sort: Sort::Bool,
                        smt: format!("(bvsge {c} (_ bv0 {cw}))"),
                    };
                }
                // Widen both to max(widths, bits_needed(width)): width itself
                // must be representable even when count is narrower (e.g. u1).
                let common = w.max(cw).max(32 - w.leading_zeros());
                let wide = if cw < common {
                    format!("((_ zero_extend {}) {c})", common - cw)
                } else {
                    c.clone()
                };
                let count = if cw > w {
                    format!("((_ extract {} 0) {c})", w - 1)
                } else if cw < w {
                    format!("((_ zero_extend {}) {c})", w - cw)
                } else {
                    c.clone()
                };
                let shift = if op == "SHL" {
                    "bvshl"
                } else if opts.signed {
                    "bvashr"
                } else {
                    "bvlshr"
                };
                let fill = if op == "SHR" && opts.signed {
                    format!("(bvashr {} (_ bv{} {w}))", a.smt, w - 1)
                } else {
                    format!("(_ bv0 {w})")
                };
                (
                    format!(
                        "(ite (bvuge {wide} (_ bv{w} {common})) {fill} ({shift} {} {count}))",
                        a.smt
                    ),
                    a.sort.clone(),
                )
            } else {
                if a.sort != args[1].sort {
                    return Err(err());
                }
                if ["ADD", "SUB", "MUL"].contains(&op) && !opts.overflow_wrap {
                    return Err(EncodeError("UNSUPPORTED_OVERFLOW_MODE".into()));
                }
                let smtop = match op {
                    "ADD" => "bvadd",
                    "SUB" => "bvsub",
                    "MUL" => "bvmul",
                    "DIV" => {
                        if opts.signed {
                            "bvsdiv"
                        } else {
                            "bvudiv"
                        }
                    }
                    "REM" => {
                        if opts.signed {
                            "bvsrem"
                        } else {
                            "bvurem"
                        }
                    }
                    "BIT_AND" => "bvand",
                    "BIT_OR" => "bvor",
                    "XOR" => "bvxor",
                    "LT" => {
                        if opts.signed {
                            "bvslt"
                        } else {
                            "bvult"
                        }
                    }
                    "LE" => {
                        if opts.signed {
                            "bvsle"
                        } else {
                            "bvule"
                        }
                    }
                    "GT" => {
                        if opts.signed {
                            "bvsgt"
                        } else {
                            "bvugt"
                        }
                    }
                    "GE" => {
                        if opts.signed {
                            "bvsge"
                        } else {
                            "bvuge"
                        }
                    }
                    _ => return Err(err()),
                };
                if op == "DIV" || op == "REM" {
                    guard = Term {
                        sort: Sort::Bool,
                        smt: format!("(not (= {} (_ bv0 {w})))", args[1].smt),
                    };
                }
                (
                    format!("({smtop} {} {})", a.smt, args[1].smt),
                    if ["LT", "LE", "GT", "GE"].contains(&op) {
                        Sort::Bool
                    } else {
                        a.sort.clone()
                    },
                )
            }
        };
        if opts.result_sort.as_ref().is_some_and(|s| s != &sort) {
            return Err(err());
        }
        Ok(EncodedOp {
            value: Term { smt, sort },
            normal_guard: guard,
        })
    }
    /// PHI's operand list is empty in CIR; use its predecessor-indexed inputs.
    pub fn phi(predecessor: &str, inputs: &[(String, Term)]) -> Result<EncodedOp, EncodeError> {
        let Some((_, first)) = inputs.first() else {
            return Err(EncodeError("empty PHI".into()));
        };
        let mut seen = std::collections::BTreeSet::new();
        for (p, v) in inputs {
            v.sort.validate()?;
            if !seen.insert(p) || v.sort != first.sort {
                return Err(EncodeError("invalid PHI inputs".into()));
            }
        }
        let value = inputs
            .iter()
            .find(|(p, _)| p == predecessor)
            .ok_or_else(|| EncodeError("missing PHI predecessor".into()))?
            .1
            .clone();
        Ok(EncodedOp {
            value,
            normal_guard: Term::bool(true),
        })
    }
}
