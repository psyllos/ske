mod go_support;
use go_support::*;

/// AC-0.7-02: every listing response carries `page`; a bounded exploration
/// reports truncation explicitly and never hides it as complete.
#[test]
fn api_limits_every_listing_has_page() {
    let lab = Lab::new();
    let api = ske_api::ApiService::open(&lab.db).unwrap();
    let functions = api.list_functions("latest", None, false, 3, 0).unwrap();
    assert!(functions["page"].is_object());
    assert_eq!(functions["page"]["limit"], 3);
    assert_eq!(functions["page"]["offset"], 0);
    assert!(functions["page"]["total"].is_u64());
    assert!(functions["page"]["truncated"].is_boolean());

    let db = lab.db.clone();
    drop(api);
    // Branches listing carries page and never shows completeness without a run.
    let q = ske_query::Query::open(&db).unwrap();
    let branches = q
        .branches_page("latest", None, "example.Decide", 2, 0)
        .unwrap();
    assert!(branches["page"].is_object());
    assert_eq!(branches["page"]["limit"], 2);
}

#[test]
fn api_limits_clamps_over_large_limit() {
    let lab = Lab::new();
    let api = ske_api::ApiService::open(&lab.db).unwrap();
    // The service clamps to 10000 instead of overflowing page buffers.
    let oversized = api
        .list_functions("latest", None, false, 100_000, 0)
        .unwrap();
    assert_eq!(oversized["page"]["limit"], 10_000);
}
