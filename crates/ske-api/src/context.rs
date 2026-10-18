//! Context Builder (AC-0.7-06): selection by structural links, then a token
//! budget; relevant unknowns are included first; `truncated` is always shown.
//! Token estimate: ceil(chars/4).
use serde::Serialize;
use ske_common::{Error, Result};
use std::collections::{BTreeMap, BTreeSet};

/// Estimated token count for a text (AC-0.7-06 rule): ceil(chars/4).
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

/// Knowledge status of a structural element (selection candidate).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Status {
    Unknown,
    Timeout,
    Unsupported,
    Possible,
    Witnessed,
    ProvenUnreachable,
    Unanalyzed,
}

impl Status {
    /// A relevant unknown (included first, AC-0.7-06).
    pub fn is_unknown(self) -> bool {
        matches!(
            self,
            Status::Unknown | Status::Timeout | Status::Unsupported
        )
    }
}

/// One structured information item (rule, case, issue, proof...).
#[derive(Clone)]
pub struct Section {
    pub kind: &'static str,
    pub text: String,
    pub status: Option<Status>,
    pub links: Vec<String>,
}

impl Section {
    pub fn new(kind: &'static str, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            status: None,
            links: Vec::new(),
        }
    }
}

/// Output of the Context Builder.
#[derive(Debug, Serialize)]
pub struct Context {
    pub text: String,
    pub truncated: bool,
    pub max_tokens: usize,
    pub tokens_used: usize,
    pub selected: usize,
    pub total: usize,
}

/// Compatibility entry for an already selected flat list. Links require
/// `build_from_roots`, because a flat section has no identity.
/// Impossible budgets are explicit errors, including on this legacy entry.
pub fn build(sections: Vec<Section>, max_tokens: usize) -> Result<Context> {
    try_build(sections, max_tokens)
}

/// Select only nodes reachable from explicit roots. Keys are caller-owned
/// structural identities, not text matches. Root/link ordering is canonical;
/// cycles are visited once. Missing reachable references fail closed.
pub fn build_from_roots(
    graph: &BTreeMap<String, Section>,
    roots: &[String],
    max_tokens: usize,
) -> Result<Context> {
    let mut pending: BTreeSet<String> = roots.iter().cloned().collect();
    let mut seen = BTreeSet::new();
    let mut sections = Vec::new();
    while let Some(key) = pending.pop_first() {
        if !seen.insert(key.clone()) {
            continue;
        }
        let section = graph.get(&key).ok_or_else(|| {
            Error::new(
                "E_CONTEXT_REFERENCE",
                format!("missing structural node {key}"),
            )
        })?;
        pending.extend(section.links.iter().cloned());
        sections.push((key, section.clone()));
    }
    // Follow links first, then order the reachable facts by structural layer.
    sections.sort_by_key(|(key, section)| (rank(section.kind), key.clone()));
    try_build(
        sections.into_iter().map(|(_, section)| section).collect(),
        max_tokens,
    )
}

fn rank(kind: &str) -> u8 {
    match kind {
        "rule" => 0,
        "case" => 1,
        "issue" | "outcome" => 2,
        "proof" => 3,
        _ => 4,
    }
}

/// Budget the complete compact JSON output, including metadata, JSON escaping,
/// separators and `tokens_used` itself. All relevant unknowns are mandatory.
pub fn try_build(mut sections: Vec<Section>, max_tokens: usize) -> Result<Context> {
    sections.sort_by_key(|s| !s.status.is_some_and(Status::is_unknown));
    let required = sections
        .iter()
        .take_while(|s| s.status.is_some_and(Status::is_unknown))
        .count();
    let total = sections.len();
    let mut selected: Vec<&Section> = sections.iter().take(required).collect();
    let render = |items: &[&Section]| {
        let mut context = Context {
            text: items
                .iter()
                .map(|s| match s.status {
                    Some(status) => format!(
                        "{} [{}]: {}",
                        s.kind,
                        serde_json::to_value(status).unwrap().as_str().unwrap(),
                        s.text
                    ),
                    None => format!("{}: {}", s.kind, s.text),
                })
                .collect::<Vec<_>>()
                .join("\n"),
            truncated: items.len() < total,
            max_tokens,
            tokens_used: 0,
            selected: items.len(),
            total,
        };
        loop {
            let cost =
                estimate_tokens(&serde_json::to_string(&context).expect("serializable context"));
            if cost == context.tokens_used {
                break;
            }
            context.tokens_used = cost;
        }
        context
    };
    let mut context = render(&selected);
    if context.tokens_used > max_tokens {
        // The budget is smaller than the output envelope itself. Explicitly
        // fail only when relevant unknowns exist and cannot fit; with no
        // mandatory facts there is nothing to silently drop, so return the
        // empty truncated context instead of a misleading hard error.
        if required > 0 {
            return Err(Error::new(
                "E_CONTEXT_BUDGET",
                "budget cannot contain metadata and all relevant unknowns",
            ));
        }
        return Ok(context);
    }
    for section in sections.iter().skip(required) {
        selected.push(section);
        let candidate = render(&selected);
        if candidate.tokens_used <= max_tokens {
            context = candidate;
        } else {
            selected.pop();
        }
    }
    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph_node(kind: &'static str, text: &str, links: &[&str]) -> Section {
        let mut section = Section::new(kind, text);
        section.links = links.iter().map(|s| s.to_string()).collect();
        section
    }

    #[test]
    fn context_builder_budget_structural_selection_cycles_and_missing_refs() {
        let mut graph = BTreeMap::from([
            ("r".into(), graph_node("rule", "RULE", &["c"])),
            ("c".into(), graph_node("case", "CASE", &["i"])),
            ("i".into(), graph_node("issue", "ISSUE", &["p"])),
            ("p".into(), graph_node("proof", "PROOF", &["r"])),
            (
                "unrelated".into(),
                graph_node("issue", "UNRELATED", &["absent"]),
            ),
        ]);
        graph.get_mut("unrelated").unwrap().status = Some(Status::Unknown);
        let roots = vec!["r".into(), "r".into()];
        let context = build_from_roots(&graph, &roots, 500).unwrap();
        assert_eq!(
            context.text,
            "rule: RULE\ncase: CASE\nissue: ISSUE\nproof: PROOF"
        );
        assert_eq!(context.total, 4);
        assert_eq!(context.selected, 4);
        assert!(!context.truncated);
        assert_eq!(
            context.tokens_used,
            estimate_tokens(&serde_json::to_string(&context).unwrap())
        );
        graph.get_mut("i").unwrap().links.push("missing".into());
        assert!(build_from_roots(&graph, &roots, 500).is_err());
        assert!(build_from_roots(&graph, &["no-root".into()], 500).is_err());
    }

    #[test]
    fn context_builder_budget_unknowns_mandatory_and_deterministic() {
        let mut graph = BTreeMap::new();
        graph.insert(
            "r".into(),
            graph_node("rule", &"ordinary".repeat(1000), &["u", "t", "s"]),
        );
        for (id, status) in [
            ("u", Status::Unknown),
            ("t", Status::Timeout),
            ("s", Status::Unsupported),
        ] {
            let mut section = graph_node("issue", "quoted \"text\"\n反復🦀", &[]);
            section.status = Some(status);
            graph.insert(id.into(), section);
        }
        let context = build_from_roots(&graph, &["r".into()], 500).unwrap();
        assert!(context.truncated);
        assert_eq!(context.selected, 3);
        for status in ["UNKNOWN", "TIMEOUT", "UNSUPPORTED"] {
            assert!(context.text.contains(status));
        }
        assert!(context.tokens_used <= 500);
        assert_eq!(
            context.tokens_used,
            estimate_tokens(&serde_json::to_string(&context).unwrap())
        );
        graph.get_mut("r").unwrap().links.reverse();
        let repeated = build_from_roots(&graph, &["r".into(), "r".into()], 500).unwrap();
        assert_eq!(
            serde_json::to_string(&context).unwrap(),
            serde_json::to_string(&repeated).unwrap()
        );
        graph.get_mut("u").unwrap().text = "unknown".repeat(1000);
        assert!(build_from_roots(&graph, &["r".into()], 500).is_err());
        // With no mandatory facts, a sub-metadata budget is an empty truncated
        // context, not a hard error (nothing was silently dropped).
        let empty = try_build(Vec::new(), 0).unwrap();
        assert_eq!(empty.selected, 0);
        assert!(!empty.truncated);
    }

    #[test]
    fn estimate_tokens_ceiling() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
    }

    #[test]
    fn context_builder_budget_counts_rendered_separators() {
        let sections = (0..500).map(|_| Section::new("issue", "abcd")).collect();
        let ctx = build(sections, 500).unwrap();
        assert!(ctx.truncated);
        assert_eq!(
            ctx.tokens_used,
            estimate_tokens(&serde_json::to_string(&ctx).unwrap())
        );
        assert!(ctx.tokens_used <= 500);
    }

    #[test]
    fn context_builder_budget_respects_limit() {
        let sections = vec![
            Section::new("rule", "age < 18 -> return ErrUnderage"),
            Section::new("case", "case 0: underage"),
            Section::new("issue", "sd0/o0 PROVEN_UNREACHABLE"),
        ];
        let ctx = build(sections.clone(), 500).unwrap();
        assert!(!ctx.truncated);
        assert!(ctx.tokens_used <= 500);
        assert_eq!(ctx.selected, 3);
        // No unknowns here: a one-token budget yields an empty truncated
        // context (explicit fail is reserved for mandatory unknowns). The
        // serialized envelope alone costs more than one token, so `tokens_used`
        // reports that real cost (truncated=false would be a silent lie).
        let tiny = try_build(sections, 1).unwrap();
        assert!(tiny.truncated);
        assert_eq!(tiny.selected, 0);
        assert_eq!(
            tiny.tokens_used,
            estimate_tokens(&serde_json::to_string(&tiny).unwrap())
        );
        // With a mandatory unknown that cannot fit, the budget fails closed.
        let mut mandatory = vec![
            Section::new("rule", "age < 18 -> return ErrUnderage"),
            Section {
                kind: "issue",
                text: "unknown outcome (UNKNOWN)".to_string(),
                status: Some(Status::Unknown),
                links: vec![],
            },
        ];
        mandatory.push(Section::new("case", "case 0: underage"));
        assert!(try_build(mandatory, 1).is_err());
    }

    #[test]
    fn context_builder_unknowns_prioritized() {
        let sections = vec![
            Section {
                kind: "issue",
                text: "a very long ordinary outcome text that would fill the whole budget"
                    .to_string(),
                status: Some(Status::Possible),
                links: vec![],
            },
            Section {
                kind: "issue",
                text: "unknown outcome (UNKNOWN)".to_string(),
                status: Some(Status::Unknown),
                links: vec![],
            },
        ];
        let ctx = build(sections, 50).unwrap();
        assert!(ctx.truncated);
        assert!(ctx.text.contains("UNKNOWN"));
        assert!(!ctx.text.contains("ordinary outcome"));
    }
}
