//! SQLite-independent knowledge API service (v0.7, §11.7).
//!
//! Wraps [`ske_query::Query`] (the application DTO façade) and enforces the
//! pagination/truncation contract (AC-0.7-02): every listing carries `page`,
//! every bounded exploration reports `truncated`/`truncation_reason`/
//! `visited_count`/`frontier_count` and is never presented as complete.
use ske_common::Result;
use ske_query::Query;

/// Default and maximum page size (§11.1: default 100, max 10000).
pub const DEFAULT_LIMIT: usize = 100;
pub const MAX_LIMIT: usize = 10_000;

/// Validate and clamp a user-supplied limit/offset. An explicit offset beyond
/// the row count simply yields an empty page (never an error).
pub fn clamp_page(limit: usize, offset: usize) -> (usize, usize) {
    (limit.min(MAX_LIMIT), offset)
}

/// The knowledge API: a thin, SQLite-independent veneer over `ske-query`.
pub struct ApiService {
    query: Query,
}

impl ApiService {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        Ok(Self {
            query: Query::open(path)?,
        })
    }

    pub fn into_query(self) -> Query {
        self.query
    }

    /// §11.7 DTO `inspect_snapshot`.
    pub fn inspect_snapshot(
        &self,
        snapshot: &str,
        repository: Option<&str>,
    ) -> Result<serde_json::Value> {
        let snapshot = self.query.closed_snapshot(snapshot, repository)?;
        let mut value = self.query.inspect(&snapshot, repository)?;
        paginate(&mut value, "units", DEFAULT_LIMIT, 0);
        Ok(value)
    }

    /// §11.7 DTO `list_functions` (paginated).
    pub fn list_functions(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        include_external: bool,
        limit: usize,
        offset: usize,
    ) -> Result<serde_json::Value> {
        let (limit, offset) = clamp_page(limit, offset);
        let snapshot = self.query.closed_snapshot(snapshot, repository)?;
        self.query
            .functions(&snapshot, repository, include_external, limit, offset)
    }

    /// §11.7 DTO `list_branches` (paginated).
    pub fn list_branches(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        function: &str,
        limit: usize,
        offset: usize,
    ) -> Result<serde_json::Value> {
        let (limit, offset) = clamp_page(limit, offset);
        let snapshot = self.query.closed_snapshot(snapshot, repository)?;
        self.query
            .branches_page(&snapshot, repository, function, limit, offset)
    }

    /// §11.7 DTO `get_function_cfg`.
    pub fn get_function_cfg(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        function: &str,
    ) -> Result<serde_json::Value> {
        let snapshot = self.query.closed_snapshot(snapshot, repository)?;
        let mut value = self.query.cfg(&snapshot, repository, function)?;
        paginate(&mut value, "blocks", DEFAULT_LIMIT, 0);
        Ok(value)
    }

    /// §11.7 DTO `explore_paths` with explicit truncation reporting.
    #[allow(clippy::too_many_arguments)]
    pub fn explore_paths(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        source: &str,
        target: &str,
        max_depth: Option<usize>,
        max_paths: Option<usize>,
        max_nodes: Option<usize>,
        limit: usize,
        offset: usize,
    ) -> Result<serde_json::Value> {
        let (limit, offset) = clamp_page(limit, offset);
        let snapshot = self.query.closed_snapshot(snapshot, repository)?;
        self.query.paths_explore(
            &snapshot,
            repository,
            source,
            target,
            Some(max_depth.unwrap_or(32).min(128)),
            Some(max_paths.unwrap_or(1000).min(MAX_LIMIT)),
            Some(max_nodes.unwrap_or(MAX_LIMIT).min(MAX_LIMIT)),
            limit,
            offset,
        )
    }

    /// §11.7 DTO `get_function_summary`: wraps `summarize_scalar`.
    pub fn get_function_summary(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        entry: &str,
    ) -> Result<serde_json::Value> {
        self.query.get_function_summary(snapshot, repository, entry)
    }

    /// §11.7 DTO `reconstruct_behaviors` (paginated).
    pub fn reconstruct_behaviors(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        entry: &str,
        limit: usize,
        offset: usize,
    ) -> Result<serde_json::Value> {
        let (limit, offset) = clamp_page(limit, offset);
        let snapshot = self.query.closed_snapshot(snapshot, repository)?;
        self.query
            .reconstruct_behaviors(&snapshot, repository, entry, limit, offset)
    }

    /// §11.7 DTO `list_rules` (paginated).
    pub fn list_rules(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        with_evidence: bool,
        limit: usize,
        offset: usize,
    ) -> Result<serde_json::Value> {
        let (limit, offset) = clamp_page(limit, offset);
        let snapshot = self.query.closed_snapshot(snapshot, repository)?;
        self.query
            .list_rules(&snapshot, repository, with_evidence, limit, offset)
    }

    /// §11.7 DTO `get_rule_evidence`: one rule plus its evidence anchors.
    pub fn get_rule_evidence(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        rule_key: &str,
    ) -> Result<serde_json::Value> {
        self.query.get_rule_evidence(snapshot, repository, rule_key)
    }

    /// §11.7 DTO `compare_snapshots`: structural diff of two snapshots of the
    /// same repository (inventory counts, functions, rules). Conservative:
    /// reports deltas, never a blanket equality claim.
    pub fn compare_snapshots(
        &self,
        repository: &str,
        from: &str,
        to: &str,
    ) -> Result<serde_json::Value> {
        crate::compare::compare(&self.query, repository, from, to)
    }
}

pub(crate) fn paginate(value: &mut serde_json::Value, key: &str, limit: usize, offset: usize) {
    if let Some(items) = value[key].as_array_mut() {
        let total = items.len();
        *items = std::mem::take(items)
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();
        value["page"] = serde_json::json!({"limit":limit,"offset":offset,"total":total,"truncated":offset.saturating_add(limit)<total});
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_limits_defaults_and_cap() {
        // No paging request may exceed the 10 000 cap (§11.1) or panic.
        assert_eq!(clamp_page(0, 0), (0, 0));
        assert_eq!(clamp_page(100, 0), (100, 0));
        assert_eq!(clamp_page(10_000, 7), (10_000, 7));
        assert_eq!(clamp_page(100_000, 0), (10_000, 0));
        assert_eq!(clamp_page(usize::MAX, 0), (10_000, 0));
        assert_eq!(DEFAULT_LIMIT, 100);
        assert_eq!(MAX_LIMIT, 10_000);
    }

    #[test]
    fn api_limits_listing_envelope_has_page() {
        // A listing DTO must always carry `page`; empty lists are still paged
        // and reported as not truncated.
        let body = serde_json::json!({"page": {"limit": 100, "offset": 0, "total": 0, "truncated": false}});
        let page = ske_query::dto::page_from_json(&body).unwrap();
        assert_eq!(page.limit, 100);
        assert!(!page.truncated);
    }
}
