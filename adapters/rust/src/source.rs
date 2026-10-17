//! Source decisions are collected before MIR lowering, using byte-exact spans.
use proc_macro2::Span as TokenSpan;
use ske_cir::proto::{SourceDecisionDef, SourceOutcomeDef, Span};
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
};

pub struct Function {
    pub name: String,
    pub span: Span,
    pub decisions: Vec<SourceDecisionDef>,
    pub parameters: Vec<String>,
    pub direct_ifs: Vec<String>,
}

pub struct Inventory {
    pub functions: Vec<Function>,
    pub expected: u64,
}

pub fn byte_column(text: &str, line: usize, char_column: usize) -> u32 {
    let line = text.split('\n').nth(line - 1).unwrap_or("");
    (line
        .char_indices()
        .nth(char_column)
        .map(|(i, _)| i)
        .unwrap_or(line.len())
        + 1) as u32
}

pub fn span(path: &str, text: &str, s: TokenSpan) -> Span {
    let a = s.start();
    let b = s.end();
    Span {
        path: path.into(),
        start_line: a.line as u32,
        start_column: byte_column(text, a.line, a.column),
        end_line: b.line as u32,
        end_column: byte_column(text, b.line, b.column),
    }
}

/// Refuse constructs requiring code loading or untracked source discovery before
/// invoking rustc. This is deliberately stricter than rustc's own syntax parser.
pub fn inventory(path: &str, text: &str) -> Result<Inventory, String> {
    let file = syn::parse_file(text).map_err(|e| e.to_string())?;
    struct Gate {
        reason: Option<String>,
    }
    impl<'ast> Visit<'ast> for Gate {
        fn visit_macro(&mut self, _: &'ast syn::Macro) {
            self.reason = Some(
                "macros (including include!) require expansion provenance; unsupported".into(),
            );
        }
        fn visit_attribute(&mut self, _: &'ast syn::Attribute) {
            // Reject custom attributes, cfg/cfg_attr and derives. They may alter
            // the source inventory or require a proc macro.
            self.reason = Some("attributes require cfg/expansion provenance; unsupported".into());
        }
        fn visit_item_mod(&mut self, _: &'ast syn::ItemMod) {
            self.reason = Some("modules require multi-file provenance; unsupported".into());
        }
        fn visit_item_extern_crate(&mut self, _: &'ast syn::ItemExternCrate) {
            self.reason = Some("extern crates are not loaded by the isolated adapter".into());
        }
        fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {
            self.reason = Some("closures require separate function provenance; unsupported".into());
        }
        fn visit_expr_await(&mut self, _: &'ast syn::ExprAwait) {
            self.reason = Some("await requires coroutine provenance; unsupported".into());
        }
    }
    let mut gate = Gate { reason: None };
    gate.visit_file(&file);
    if let Some(reason) = gate.reason {
        return Err(reason);
    }
    let mut functions = vec![];
    let mut expected = 0;
    for item in &file.items {
        let syn::Item::Fn(f) = item else {
            // Type declarations are safe but may create implicit MIR functions.
            if matches!(
                item,
                syn::Item::Struct(_) | syn::Item::Enum(_) | syn::Item::Type(_) | syn::Item::Use(_)
            ) {
                continue;
            }
            return Err("only free functions and type/use declarations are supported".into());
        };
        if f.sig.asyncness.is_some() || !f.sig.generics.params.is_empty() || f.sig.abi.is_some() {
            return Err(
                "async, generic definitions and FFI require dedicated provenance; unsupported"
                    .into(),
            );
        }
        let name = f.sig.ident.to_string();
        let id = format!("rust:fn:{name}");
        let mut collector = Decisions {
            path,
            text,
            function: &id,
            decisions: vec![],
            direct_ifs: vec![],
        };
        collector.visit_block(&f.block);
        // Count separately from emission, with a distinct visitor.
        struct Count(u64);
        impl<'ast> Visit<'ast> for Count {
            fn visit_expr(&mut self, e: &'ast syn::Expr) {
                if matches!(
                    e,
                    syn::Expr::If(_)
                        | syn::Expr::Match(_)
                        | syn::Expr::Try(_)
                        | syn::Expr::While(_)
                        | syn::Expr::ForLoop(_)
                        | syn::Expr::Loop(_)
                ) {
                    self.0 += 1;
                }
                visit::visit_expr(self, e);
            }
        }
        let mut count = Count(0);
        count.visit_block(&f.block);
        expected += count.0;
        let parameters = f
            .sig
            .inputs
            .iter()
            .map(|arg| match arg {
                syn::FnArg::Typed(t) => match t.pat.as_ref() {
                    syn::Pat::Ident(i) => i.ident.to_string(),
                    _ => String::new(),
                },
                _ => String::new(),
            })
            .collect();
        functions.push(Function {
            name,
            span: span(path, text, f.span()),
            decisions: collector.decisions,
            parameters,
            direct_ifs: collector.direct_ifs,
        });
    }
    Ok(Inventory {
        functions,
        expected,
    })
}

struct Decisions<'a> {
    path: &'a str,
    text: &'a str,
    function: &'a str,
    decisions: Vec<SourceDecisionDef>,
    direct_ifs: Vec<String>,
}

impl Decisions<'_> {
    fn push(&mut self, kind: &str, s: TokenSpan, labels: Vec<String>) {
        let id = format!("{}/sd{}", self.function, self.decisions.len());
        let outcomes = labels
            .into_iter()
            .enumerate()
            .map(|(i, label)| SourceOutcomeDef {
                id: format!("{id}/o{i}"),
                label,
                elided: false,
                // No attestation until MIR path mapping has actually succeeded.
                activation_mapping_complete: false,
            })
            .collect();
        self.decisions.push(SourceDecisionDef {
            id,
            decision_kind: kind.into(),
            function_id: self.function.into(),
            span: Some(span(self.path, self.text, s)),
            outcomes,
            ..Default::default()
        });
    }
}

impl<'ast> Visit<'ast> for Decisions<'_> {
    fn visit_expr_if(&mut self, e: &'ast syn::ExprIf) {
        self.push("IF", e.cond.span(), vec!["TRUE".into(), "FALSE".into()]);
        // A complete DIRECT mapping is only justified for one evaluated atom.
        // General short-circuit expressions require ordered activation cases.
        let atom = match e.cond.as_ref() {
            syn::Expr::Path(_) | syn::Expr::Lit(_) => true,
            syn::Expr::Binary(b) => {
                matches!(
                    b.op,
                    syn::BinOp::Lt(_)
                        | syn::BinOp::Le(_)
                        | syn::BinOp::Gt(_)
                        | syn::BinOp::Ge(_)
                        | syn::BinOp::Eq(_)
                        | syn::BinOp::Ne(_)
                ) && matches!(b.left.as_ref(), syn::Expr::Path(_) | syn::Expr::Lit(_))
                    && matches!(
                        b.right.as_ref(),
                        syn::Expr::Path(_) | syn::Expr::Lit(_) | syn::Expr::Unary(_)
                    )
            }
            _ => false,
        };
        if atom {
            self.direct_ifs
                .push(self.decisions.last().unwrap().id.clone());
        }
        visit::visit_expr_if(self, e);
    }
    fn visit_expr_match(&mut self, e: &'ast syn::ExprMatch) {
        let mut case = 0;
        let labels = e
            .arms
            .iter()
            .map(|arm| {
                if matches!(arm.pat, syn::Pat::Wild(_)) {
                    "DEFAULT".into()
                } else {
                    let label = format!("CASE:{case}");
                    case += 1;
                    label
                }
            })
            .collect();
        self.push("MATCH", e.expr.span(), labels);
        visit::visit_expr_match(self, e);
    }
    fn visit_expr_try(&mut self, e: &'ast syn::ExprTry) {
        self.push(
            "ERROR_PROPAGATION",
            e.span(),
            vec!["SUCCESS".into(), "ERROR".into()],
        );
        visit::visit_expr_try(self, e);
    }
    fn visit_expr_while(&mut self, e: &'ast syn::ExprWhile) {
        self.push(
            "LOOP_CONDITION",
            e.cond.span(),
            vec!["ENTER_BODY".into(), "EXIT".into()],
        );
        visit::visit_expr_while(self, e);
    }
    fn visit_expr_for_loop(&mut self, e: &'ast syn::ExprForLoop) {
        self.push(
            "RANGE_ITERATION",
            e.expr.span(),
            vec!["ENTER_BODY".into(), "EXIT".into()],
        );
        visit::visit_expr_for_loop(self, e);
    }
    fn visit_expr_loop(&mut self, e: &'ast syn::ExprLoop) {
        self.push(
            "X_RUST_LOOP",
            e.loop_token.span,
            vec!["ENTER_BODY".into(), "EXIT".into()],
        );
        visit::visit_expr_loop(self, e);
    }
    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {
        // Gate below in lower rejects additional MIR functions; no attribution
        // of a closure's decisions to its containing function.
    }
}
