//! v0.8 scenarios (causal link/show) and functional documentation fiches,
//! exercised end-to-end on the real Go lab through the public Query façade.
mod go_support;
use go_support::{adapter, profile, Lab};
use serde_json::Value;

fn open(lab: &Lab) -> ske_query::Query {
    ske_query::Query::open(&lab.db).unwrap()
}

fn reconstruct(q: &ske_query::Query, entry: &str) {
    q.reconstruct_behaviors("latest", None, entry, 100, 0)
        .unwrap();
}

#[test]
fn scenario_link_is_confirmed_and_idempotent() {
    let lab = Lab::new();
    let q = open(&lab);
    reconstruct(&q, "example.Entry");
    reconstruct(&q, "example.helper");
    let first = q.scenarios_link("latest", None, "example.Entry").unwrap();
    let links = first["links"].as_array().unwrap();
    assert_eq!(links.len(), 1, "Entry calls exactly helper");
    assert_eq!(links[0]["callee"], "example.helper");
    assert_eq!(links[0]["confidence"], "CONFIRMED");
    assert_eq!(
        first["not_reconstructed"].as_array().unwrap().len(),
        0,
        "helper was reconstructed, so it must link, not fall back"
    );
    let second = q.scenarios_link("latest", None, "example.Entry").unwrap();
    assert_eq!(
        second["links"].as_array().unwrap().len(),
        1,
        "linking again must not duplicate the CALL edge"
    );
    for r in q.list_scenario_links("latest", None).unwrap()["links"]
        .as_array()
        .unwrap()
    {
        if r["from_scenario_id"] == first["scenario_id"] {
            assert_eq!(r["relation_kind"], "CALL");
            assert_eq!(r["confidence_kind"], "CONFIRMED");
        }
    }
    assert_eq!(q.db_check().unwrap()["ok"], true);
}

#[test]
fn scenario_link_reports_unreconstructed_callee() {
    let lab = Lab::new();
    let q = open(&lab);
    reconstruct(&q, "example.Entry");
    // helper is NOT reconstructed here.
    let result = q.scenarios_link("latest", None, "example.Entry").unwrap();
    assert_eq!(
        result["links"].as_array().unwrap().len(),
        0,
        "no scenario for helper => no invented CALL link"
    );
    assert_eq!(
        result["not_reconstructed"].as_array().unwrap().as_slice(),
        &[Value::String("example.helper".to_owned())]
    );
    assert_eq!(result["total_callees"], 1);
}

#[test]
fn scenario_show_builds_causal_chain() {
    let lab = Lab::new();
    let q = open(&lab);
    reconstruct(&q, "example.Entry");
    reconstruct(&q, "example.helper");
    let tree = q
        .scenarios_show("latest", None, "example.Entry", 3)
        .unwrap();
    assert_eq!(tree["function"], "example.Entry");
    assert_eq!(tree["not_reconstructed"], false);
    let cases = tree["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    // The composed entry behavior matches helper's body (a>100 => 1).
    assert!(cases
        .iter()
        .any(|c| c["guard_text"] == "a > 100" && c["result_text"] == "return 1"));
    let callees = tree["callees"].as_array().unwrap();
    assert_eq!(callees.len(), 1);
    assert_eq!(callees[0]["function"], "example.helper");
    assert!(!callees[0]["scenario_id"].is_null());
    assert_eq!(callees[0]["cases"].as_array().unwrap().len(), 2);
}

#[test]
fn docs_render_writes_fiches_with_authoritative_dedup() {
    let lab = Lab::new();
    let mut q = open(&lab);
    reconstruct(&q, "example.Check");
    reconstruct(&q, "example.Decide");
    // Reconstruct Check again: two behavior runs reference the same summary.
    reconstruct(&q, "example.Check");
    // A branch analysis on Decide proves `x < -10` MODEL_UNREACHABLE via Z3;
    // the fiche must render that proof.
    let budget = ske_query::analysis_config::Budget::default();
    let context = ske_query::analysis_config::Context::default();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    q.analyze("latest", None, "example.Decide", &budget, &context, &cancel)
        .unwrap();
    let out = tempfile::tempdir().unwrap();
    let dto = q.docs_render("latest", None, out.path()).unwrap();
    assert!(dto["written"].as_i64().unwrap() >= 2);
    let index = std::fs::read_to_string(out.path().join("INDEX.md")).unwrap();
    assert!(index.contains("example.Check"));
    assert!(index.contains("example.Decide"));
    let check = std::fs::read_to_string(out.path().join("example_Check.md")).unwrap();
    // Authoritative-run dedup: 3 cases, not 6.
    let table_rows = check
        .lines()
        .filter(|l| l.starts_with('|') && l.contains('`') && !l.contains("WHEN"))
        .collect::<Vec<_>>();
    assert_eq!(
        table_rows.len(),
        3,
        "fiche must carry the authoritative run only"
    );
    assert!(check.contains("age >= 18 AND enabled == true"));
    let decide = std::fs::read_to_string(out.path().join("example_Decide.md")).unwrap();
    assert!(decide.contains("x < 0"));
    assert!(
        decide.contains("MODEL_UNREACHABLE"),
        "proved-unreachable branch must be documented"
    );
}

#[test]
fn docs_render_skips_functions_without_behavior() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    std::fs::write(repo.join("go.mod"), "module nobehav\n\ngo 1.22\n").unwrap();
    // A source function that stays UNANALYZED (no summary): the lab's
    // example.Pipe already has no behavior reconstructed unless asked.
    let lab = Lab::new();
    let q = open(&lab);
    // Do NOT reconstruct anything. docs render on the lab snapshot: only
    // functions with a reconstructed behavior get a fiche.
    let out = lab.dir.path().join("docs");
    std::fs::create_dir(&out).unwrap();
    let dto = q.docs_render("latest", None, &out).unwrap();
    assert_eq!(
        dto["written"].as_i64().unwrap(),
        0,
        "no fiche without behavior"
    );
}

// ---- Cycle 2 : capability nomination + top-down view (v0.8) ----

#[test]
fn capability_nominate_and_reject_update_status() {
    let lab = Lab::new();
    let q = open(&lab);
    reconstruct(&q, "example.Check");
    let caps0 = q.capabilities("latest", None).unwrap();
    let check = caps0["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["canonical_key"]
                .as_str()
                .unwrap()
                .ends_with("example.Check")
        })
        .cloned()
        .expect("capability for Check");
    assert_eq!(check["title"], serde_json::Value::Null);
    assert_eq!(check["validation_status"], "UNVALIDATED");

    let n = q
        .capability_nominate("latest", None, "example.Check", "Éligibilité")
        .unwrap();
    assert_eq!(n["validation_status"], "HUMAN_VALIDATED");
    assert_eq!(n["title"], "Éligibilité");

    let caps1 = q.capabilities("latest", None).unwrap();
    let check1 = caps1["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["canonical_key"]
                .as_str()
                .unwrap()
                .ends_with("example.Check")
        })
        .cloned()
        .unwrap();
    assert_eq!(check1["validation_status"], "HUMAN_VALIDATED");

    let r = q
        .capability_reject("latest", None, "example.Check")
        .unwrap();
    assert_eq!(r["validation_status"], "REJECTED");
    let caps2 = q.capabilities("latest", None).unwrap();
    let check2 = caps2["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["canonical_key"]
                .as_str()
                .unwrap()
                .ends_with("example.Check")
        })
        .cloned()
        .unwrap();
    assert_eq!(check2["validation_status"], "REJECTED");
    assert_eq!(q.db_check().unwrap()["ok"], true);
}

#[test]
fn capability_nominate_requires_existing_capability() {
    let lab = Lab::new();
    let q = open(&lab);
    // No reconstruction => no capability => E_NOT_FOUND.
    let err = q
        .capability_nominate("latest", None, "example.Check", "X")
        .unwrap_err();
    assert_eq!(err.code, "E_NOT_FOUND");
}

#[test]
fn capability_show_renders_scenario_chain_with_nomination() {
    let lab = Lab::new();
    let q = open(&lab);
    reconstruct(&q, "example.Check");
    q.capability_nominate("latest", None, "example.Check", "Éligibilité")
        .unwrap();
    let view = q
        .capability_show("latest", None, "example.Check", 2)
        .unwrap();
    assert_eq!(view["title"], "Éligibilité");
    assert_eq!(view["validation_status"], "HUMAN_VALIDATED");
    let chains = view["chains"].as_array().unwrap();
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0]["function"], "example.Check");
    let cases = chains[0]["cases"].as_array().unwrap();
    let guards: Vec<&str> = cases
        .iter()
        .map(|c| c["guard_text"].as_str().unwrap())
        .collect();
    assert!(guards.contains(&"age < 18"));
    assert!(guards.contains(&"age >= 18 AND enabled == true"));
}

#[test]
fn docs_render_capabilities_writes_top_down_view() {
    let lab = Lab::new();
    let q = open(&lab);
    reconstruct(&q, "example.Check");
    q.capability_nominate("latest", None, "example.Check", "Éligibilité")
        .unwrap();
    reconstruct(&q, "example.Decide");
    let out = tempfile::tempdir().unwrap();
    let dto = q
        .docs_render_capabilities("latest", None, out.path())
        .unwrap();
    assert_eq!(dto["written"].as_i64().unwrap(), 2, "two capabilities");
    let body = std::fs::read_to_string(out.path().join("CAPABILITIES.md")).unwrap();
    assert!(body.contains("Éligibilité"));
    assert!(body.contains("age < 18"));
    assert!(body.contains("HUMAN_VALIDATED"));
    assert!(body.contains("(non nommée)"));
}

// ---- Cycle 3 : capability stale-check + interservice contracts (v0.8) ----

#[test]
fn capability_stale_check_marks_changed_capability_stale() {
    // Two snapshots of the same repository; change check.go between them.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    std::fs::write(repo.join("go.mod"), "module example\n\ngo 1.22\n").unwrap();
    std::fs::write(
        repo.join("check.go"),
        "package example\nfunc Check(age int, enabled bool) error {\n if age < 18 { return nil }\n if !enabled { return nil }\n return nil\n}\n",
    )
    .unwrap();
    let cfg = dir.path().join("p.json");
    std::fs::write(&cfg, profile("linux", "amd64").to_string()).unwrap();
    let profile_json = std::fs::read_to_string(&cfg).unwrap();
    let lab = Lab::empty();
    let q = open(&lab);
    let limits = ske_query::RunLimits::default();
    let ingest = |repo: &std::path::Path, name: &str| -> i64 {
        q.ingest_go(&ske_query::GoOptions {
            repo,
            adapter: adapter(),
            profile_json: &profile_json,
            limits: &limits,

            repository: Some(name),
            redact: false,
        })
        .unwrap()["snapshot_id"]
            .as_i64()
            .unwrap()
    };
    let a = ingest(&repo, "cap-stale");
    q.reconstruct_behaviors(&a.to_string(), None, "example.Check", 100, 0)
        .unwrap();
    q.capability_nominate(&a.to_string(), None, "example.Check", "Éligibilité")
        .unwrap();
    // Change the function body.
    let text = std::fs::read_to_string(repo.join("check.go")).unwrap();
    std::fs::write(repo.join("check.go"), text.replace("age < 18", "age < 21")).unwrap();
    let b = ingest(&repo, "cap-stale");
    let result = q
        .capability_stale_check("cap-stale", &a.to_string(), &b.to_string())
        .unwrap();
    assert_eq!(result["capabilities_marked_stale"], 1);
    let caps = q.capabilities(&a.to_string(), None).unwrap();
    let check = caps["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["canonical_key"]
                .as_str()
                .unwrap()
                .ends_with("example.Check")
        })
        .cloned()
        .unwrap();
    assert_eq!(check["validation_status"], "STALE");
    assert_eq!(q.db_check().unwrap()["ok"], true);
}

#[test]
fn interservice_link_is_declared_inferred_and_idempotent() {
    // Two services, two repositories in the SAME database.
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("svca");
    let b = dir.path().join("svcb");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    std::fs::write(a.join("go.mod"), "module svca\n\ngo 1.22\n").unwrap();
    std::fs::write(b.join("go.mod"), "module svcb\n\ngo 1.22\n").unwrap();
    std::fs::write(
        a.join("charge.go"),
        "package svca\nvar Hook func(string) int\nfunc ChargeMoney(amount int) int {\n if amount < 0 { return -1 }\n r := Hook(\"charge\")\n if r > 0 { return 1 }\n return 0\n}\n",
    )
    .unwrap();
    std::fs::write(
        b.join("billing.go"),
        "package svcb\nfunc Charge(amount int) int {\n if amount > 1000 { return 2 }\n if amount < 0 { return -1 }\n return 0\n}\n",
    )
    .unwrap();
    let cfg = dir.path().join("p.json");
    std::fs::write(&cfg, profile("linux", "amd64").to_string()).unwrap();
    let profile_json = std::fs::read_to_string(&cfg).unwrap();
    let lab = Lab::empty();
    let q = open(&lab);
    let limits = ske_query::RunLimits::default();
    let ingest = |repo: &std::path::Path, name: &str| -> i64 {
        q.ingest_go(&ske_query::GoOptions {
            repo,
            adapter: adapter(),
            profile_json: &profile_json,
            limits: &limits,

            repository: Some(name),
            redact: false,
        })
        .unwrap()["snapshot_id"]
            .as_i64()
            .unwrap()
    };
    let snap_a = ingest(&a, "svca");
    let snap_b = ingest(&b, "svcb");
    q.reconstruct_behaviors(&snap_b.to_string(), None, "svcb.Charge", 100, 0)
        .unwrap();
    q.reconstruct_behaviors(&snap_a.to_string(), None, "svca.ChargeMoney", 100, 0)
        .unwrap();
    let contracts = vec![ske_query::scenarios::InterContract {
        target_descriptor: None,
        match_any: true,
        relation_kind: "HTTP".to_owned(),
        service_function: "svcb.Charge".to_owned(),
        assumptions: vec!["transport=HTTP 1.1 sync".to_owned()],
    }];
    let first = q
        .scenarios_link_inter(
            &snap_a.to_string(),
            None,
            "svca.ChargeMoney",
            contracts.clone(),
        )
        .unwrap();
    assert_eq!(first["links"].as_array().unwrap().len(), 1);
    assert_eq!(first["links"][0]["confidence"], "INFERRED");
    assert_eq!(first["links"][0]["relation_kind"], "HTTP");
    // Idempotent.
    let second = q
        .scenarios_link_inter(&snap_a.to_string(), None, "svca.ChargeMoney", contracts)
        .unwrap();
    assert_eq!(second["links"].as_array().unwrap().len(), 1);
    // The declared link never marks the scenario closed: show still reports the
    // EXTERNAL_UNKNOWN boundary (UNKNOWN effects remain, no invented proof).
    let view = q
        .scenarios_show(&snap_a.to_string(), None, "svca.ChargeMoney", 2)
        .unwrap();
    assert_eq!(view["external_boundary"], "EXTERNAL_UNKNOWN");
    assert_eq!(
        q.list_scenario_links(&snap_a.to_string(), None).unwrap()["links"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| l["relation_kind"] == "HTTP")
            .count(),
        1
    );
    assert_eq!(q.db_check().unwrap()["ok"], true);
}
