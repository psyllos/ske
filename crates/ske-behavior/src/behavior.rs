//! Conservative behavior-case grouping and ordering (§7.2 steps 3–4, normatif).
//!
//! Inputs are the rendered rows produced by the caller (ske-query) after it
//! renders each summary case. No SQL here; pure, deterministic values only.

/// A rendered behavior case row produced by the persistence layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BehaviorCaseRowInput {
    /// Rendered guard text (already the R1–R6 conjunction).
    pub guard: String,
    /// exit_kind: RETURN | PANIC | THROW | UNREACHABLE_EXIT.
    pub exit_kind: String,
    /// Rendered result text (e.g. `0, ErrZero`), empty for PANIC.
    pub result: String,
    /// (line, col) of the primary exit span, if any.
    pub span: Option<(u64, u64)>,
    /// Ordered IR-outcome path labels, e.g. `["0/1", "1/0"]` — decision
    /// ordinal within the function then outcome ordinal, as produced by the
    /// caller from `summary_case_outcomes`.
    pub ir_path: Vec<String>,
    /// Memory delta text (structural, for dedup).
    pub memory_delta: String,
    /// Effects text (structural, for dedup).
    pub effects: String,
    /// Precision text (EXACT/OVERAPPROX/PARTIAL/UNKNOWN).
    pub precision: String,
    /// A stable fingerprint for tie-breaking only.
    pub fingerprint: String,
    /// Origin summary case id kept for the DERIVED_FROM evidence anchor.
    pub summary_case_id: Option<i64>,
}

/// A grouped behavior case: dedup merges rows conservatively (§7.2 step 3).
#[derive(Clone, Debug)]
pub struct BehaviorCaseRow {
    pub guard: String,
    pub exit_kind: String,
    pub result: String,
    pub span: Option<(u64, u64)>,
    pub ir_path: Vec<String>,
    pub memory_delta: String,
    pub effects: String,
    pub precision: String,
    pub fingerprint: String,
    /// Union of referenced IR-outcome path labels after dedup.
    pub referenced_outcomes: Vec<String>,
    /// Origin summary case id (first source; kept for DERIVED_FROM evidence).
    pub summary_case_id: Option<i64>,
}

/// §7.2 step 4 ordering comparator key: `(span, ir_path, fingerprint)`, where
/// cases without a span sort last. Returns an ordering vector usable by `sort`.
pub fn order_key(row: &BehaviorCaseRow) -> (Option<(u64, u64)>, Vec<String>, String) {
    (row.span, row.ir_path.clone(), row.fingerprint.clone())
}

/// Sort rows per §7.2 step 4 (span asc, then IR path lexicographic, then
/// fingerprint; no-span last). Returns the row order; the caller assigns the
/// new `case_ordinal` from 0 upward.
pub fn order_cases(mut rows: Vec<BehaviorCaseRow>) -> Vec<BehaviorCaseRow> {
    rows.sort_by(|a, b| {
        let (aspan, apath, afp) = order_key(a);
        let (bspan, bpath, bfp) = order_key(b);
        // No-span last.
        let a_none = a.span.is_none();
        let b_none = b.span.is_none();
        a_none
            .cmp(&b_none)
            .then_with(|| aspan.cmp(&bspan))
            .then_with(|| apath.cmp(&bpath))
            .then_with(|| afp.cmp(&bfp))
    });
    rows
}

/// §7.2 step 3: conservatively merge a group of rows into behavior cases.
/// Two rows merge iff same `exit_kind`, structurally equal `result`, same
/// `memory_delta`, same `effects`; a RETURN never merges with a PANIC. The
/// merged guard is the disjunction of the two guards rendered with ` OR `,
/// referenced outcomes = union.
pub fn dedup_cases(rows: Vec<BehaviorCaseRowInput>) -> Vec<BehaviorCaseRow> {
    let mut merged: Vec<BehavGroup> = Vec::new();
    for input in rows {
        let candidate = BehavGroup::from(input);
        if let Some(existing) = merged.iter_mut().find(|g| {
            g.exit_kind == candidate.exit_kind
                && g.result == candidate.result
                && g.memory_delta == candidate.memory_delta
                && g.effects == candidate.effects
        }) {
            existing.merge(candidate);
        } else {
            merged.push(candidate);
        }
    }
    let mut out: Vec<BehaviorCaseRow> = merged.into_iter().map(BehavGroup::finish).collect();
    // §7.2 step 4 ordering applies to the deduplicated cases.
    out.sort_by(|a, b| {
        let (aspan, apath, afp) = order_key(a);
        let (bspan, bpath, bfp) = order_key(b);
        let a_none = a.span.is_none();
        let b_none = b.span.is_none();
        a_none
            .cmp(&b_none)
            .then_with(|| aspan.cmp(&bspan))
            .then_with(|| apath.cmp(&bpath))
            .then_with(|| afp.cmp(&bfp))
    });
    out
}

struct BehavGroup {
    guard: String,
    exit_kind: String,
    result: String,
    span: Option<(u64, u64)>,
    ir_path: Vec<String>,
    memory_delta: String,
    effects: String,
    precision: String,
    fingerprint: String,
    referenced_outcomes: std::collections::BTreeSet<String>,
    summary_case_id: Option<i64>,
}

impl BehavGroup {
    fn from(input: BehaviorCaseRowInput) -> Self {
        let mut referenced_outcomes = std::collections::BTreeSet::new();
        referenced_outcomes.extend(input.ir_path.iter().cloned());
        Self {
            guard: input.guard,
            exit_kind: input.exit_kind,
            result: input.result,
            span: input.span,
            ir_path: input.ir_path,
            memory_delta: input.memory_delta,
            effects: input.effects,
            precision: input.precision,
            fingerprint: input.fingerprint,
            referenced_outcomes,
            summary_case_id: input.summary_case_id,
        }
    }
    fn merge(&mut self, other: BehavGroup) {
        // Guard = disjunction; keep the R1 order and parenthesize each side if
        // it contains a conjunction (R5).
        let left = wrap_disjunct(&self.guard);
        let right = wrap_disjunct(&other.guard);
        self.guard = format!("{left} OR {right}");
        self.referenced_outcomes.extend(other.referenced_outcomes);
        self.fingerprint = format!("{}+{}", self.fingerprint, other.fingerprint);
        if self.span.is_none() {
            self.span = other.span;
        }
        self.ir_path.extend(other.ir_path);
        self.ir_path.sort();
        self.ir_path.dedup();
        if self.summary_case_id.is_none() {
            self.summary_case_id = other.summary_case_id;
        }
    }
    fn finish(self) -> BehaviorCaseRow {
        BehaviorCaseRow {
            guard: self.guard,
            exit_kind: self.exit_kind,
            result: self.result,
            span: self.span,
            ir_path: self.ir_path,
            memory_delta: self.memory_delta,
            effects: self.effects,
            precision: self.precision,
            fingerprint: self.fingerprint,
            referenced_outcomes: self.referenced_outcomes.into_iter().collect(),
            summary_case_id: self.summary_case_id,
        }
    }
}

/// A guard containing a conjunction is parenthesized when becoming a disjunct
/// (R5).
fn wrap_disjunct(guard: &str) -> String {
    if guard.contains(" AND ") {
        format!("({guard})")
    } else {
        guard.to_owned()
    }
}

/// AC-0.5-08: rule description is exactly `WHEN <garde> THEN <résultat>`, with
/// `return` prefix on RETURN results and `panic` for PANIC/THROW. No free text.
pub fn rule_description(guard: &str, exit_kind: &str, result: &str) -> String {
    let result = match exit_kind {
        "RETURN" => {
            if result.is_empty() {
                "return".to_owned()
            } else {
                format!("return {result}")
            }
        }
        "PANIC" | "THROW" => "panic".to_owned(),
        _ => result.to_owned(),
    };
    format!("WHEN {guard} THEN {result}")
}
