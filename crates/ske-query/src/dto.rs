//! SQLite-independent typed DTOs (§11.7, AC-0.7-01).
//!
//! These serde structs describe the JSON surface consumed by `ske-api` and
//! `ske-mcp` without any `rusqlite` type leaking in. The `dto_*` tests verify
//! the shapes (required keys, pagination, truncation) against hand-written
//! expected JSON, never against program output.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON object helper: require that `value` carry every key in `keys`.
pub fn require_keys(value: &Value, keys: &[&str]) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| "DTO is not an object".to_string())?;
    for key in keys {
        if !object.contains_key(*key) {
            return Err(format!("missing DTO key {key}"));
        }
    }
    Ok(())
}

/// Page descriptor (normative §11.6 and AC-0.7-02).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub limit: usize,
    pub offset: usize,
    pub total: usize,
    pub truncated: bool,
}

/// A bounded (paginated) response envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Paged<T> {
    #[serde(flatten)]
    pub items: T,
    pub page: Page,
}

/// Exploration truncation report (AC-0.7-02). Never presented as complete.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Truncation {
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visited_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontier_count: Option<u64>,
}

/// The §11.7 DTO shape is a subset of the JSON documents; the typed structs
/// below only cover the keys the API layer must validate. The full documents
/// remain the JSON `Value` produced by `ske-query` (see §11.5).
///
/// `inspect_snapshot` essentials.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InspectSnapshotDto {
    pub snapshot: Value,
    pub units: Vec<Value>,
    pub analysis_defaults: Value,
    pub diagnostics_summary: Value,
}

/// `list_functions` / `list_branches` / `list_rules` / `reconstruct_behaviors`
/// all share the paged envelope; the item shape is validated by `require_keys`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PagedListDto {
    pub page: Page,
    #[serde(default)]
    pub functions: Option<Vec<Value>>,
    #[serde(default)]
    pub source_decisions: Option<Vec<Value>>,
    #[serde(default)]
    pub ir_outcomes: Option<Vec<Value>>,
    #[serde(default)]
    pub rules: Option<Vec<Value>>,
    #[serde(default)]
    pub cases: Option<Vec<Value>>,
}

/// The `api_limits` contract: any listing response must carry a `page` and any
/// truncated exploration must expose `truncated=true`. These functions turn a
/// raw JSON DTO into the typed envelope, failing on missing keys (never
/// silently dropping a paging field).
pub fn page_from_json(value: &Value) -> Result<Page, String> {
    let page = value
        .get("page")
        .ok_or_else(|| "response has no page".to_string())?;
    serde_json::from_value(page.clone()).map_err(|e| format!("invalid page: {e}"))
}

pub fn truncation_from_json(value: &Value) -> Result<Option<Truncation>, String> {
    if value.get("truncated").is_none() {
        return Ok(None);
    }
    serde_json::from_value(value.clone()).map_err(|e| format!("invalid truncation: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dto_page_round_trips_with_required_keys() {
        let value = json!({"page": {"limit": 100, "offset": 0, "total": 4, "truncated": false}});
        let page = page_from_json(&value).unwrap();
        assert_eq!(
            page,
            Page {
                limit: 100,
                offset: 0,
                total: 4,
                truncated: false
            }
        );
        assert_eq!(serde_json::to_value(&page).unwrap(), value["page"]);
    }

    #[test]
    fn dto_missing_page_is_rejected() {
        let value = json!({"rules": []});
        assert!(page_from_json(&value).is_err());
    }

    #[test]
    fn dto_truncated_exploration_is_never_complete() {
        let value = json!({
            "truncated": true,
            "truncation_reason": "max_nodes",
            "visited_count": 10,
            "frontier_count": 3
        });
        let truncation = truncation_from_json(&value).unwrap().unwrap();
        assert!(truncation.truncated);
        assert_eq!(truncation.truncation_reason.as_deref(), Some("max_nodes"));
        assert_eq!(truncation.visited_count, Some(10));
        assert_eq!(truncation.frontier_count, Some(3));
        // A plain object without `truncated` is not an exploration report.
        assert!(truncation_from_json(&json!({"rules": []}))
            .unwrap()
            .is_none());
    }

    #[test]
    fn dto_inspect_snapshot_shape() {
        let value = json!({
            "snapshot": {"id": 1},
            "units": [],
            "analysis_defaults": {},
            "diagnostics_summary": {"INFO": 0}
        });
        let dto: InspectSnapshotDto = serde_json::from_value(value).unwrap();
        assert_eq!(dto.units.len(), 0);
    }

    #[test]
    fn dto_require_keys_detects_missing_field() {
        let value = json!({"functions": []});
        assert!(require_keys(&value, &["functions", "page"]).is_err());
    }
}
