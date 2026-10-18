//! Structural comparison belongs in the query layer.
use ske_common::Result;
use ske_query::Query;
pub fn compare(query: &Query, repository: &str, from: &str, to: &str) -> Result<serde_json::Value> {
    query.compare_snapshots(repository, from, to)
}
