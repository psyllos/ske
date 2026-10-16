mod go_support;
use go_support::*;
use serde_json::Value;

const P: &str = "POSSIBLE";
const PU: &str = "PROVEN_UNREACHABLE";

fn analyze(lab: &Lab, name: &str, unroll: u32, partial: bool, expected: &[&str]) -> Value {
    let budget = lab.dir.path().join("budget.toml");
    std::fs::write(
        &budget,
        format!("[budget]\nloop_unroll = {unroll}\nsolver_seed = 0\n"),
    )
    .unwrap();
    let before = lab.branches(name);
    let summary = lab.ok(&[
        "branches",
        "analyze",
        "--snapshot",
        "latest",
        "--entry",
        name,
        "--budget",
        budget.to_str().unwrap(),
    ]);
    assert_eq!(
        summary["run_status"],
        if partial { "PARTIAL" } else { "COMPLETED" }
    );
    assert_eq!(summary["exploration_closed"], !partial);
    let cuts = summary["cut_count"].as_u64().expect("numeric cut_count");
    assert!(if partial { cuts >= 1 } else { cuts == 0 }, "{summary}");
    for assumption in ["SEQUENTIAL_EXECUTION", "CLOSED_ENTRY_CONTEXT"] {
        assert!(summary["assumptions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a == assumption));
    }
    let result = lab.ok(&[
        "branches",
        "list",
        "--snapshot",
        "latest",
        "--function",
        name,
        "--run",
        "latest",
    ]);
    let outcomes = result["ir_outcomes"].as_array().unwrap();
    assert_eq!(
        outcomes
            .iter()
            .map(|o| o["status"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        before["inventory"], result["inventory"],
        "prop_inventory_never_shrinks"
    );
    assert!(
        outcomes.iter().all(|o| o["status"] != "WITNESSED"),
        "prop_json_possible_vs_witnessed"
    );
    if name == "example.MemOpaque" {
        for (o, soundness) in outcomes.iter().zip(["EXACT", "EXACT", "OPAQUE", "OPAQUE"]) {
            assert_eq!(o["soundness"], soundness);
        }
    }
    assert_eq!(
        count(
            &lab.conn(),
            "SELECT COUNT(*) FROM branch_analysis_results r JOIN analysis_runs a ON a.id=r.analysis_run_id WHERE a.exploration_closed=0 AND r.status='PROVEN_UNREACHABLE'"
        ),
        0,
        "prop_no_proof_when_not_closed"
    );
    if partial {
        assert!(outcomes.iter().all(|o| o["status"] != PU));
    }
    lab.ok(&["db", "check"]);
    result
}

fn row(name: &str, fixture: Option<&str>, unroll: u32, partial: bool, expected: &[&str]) {
    let name = format!("example.{name}");
    let go = Lab::new();
    let actual = analyze(&go, &name, unroll, partial, expected);
    if let Some(fixture) = fixture {
        let cir = Lab::empty();
        cir.ok(&[
            "ingest",
            "cir",
            "--input",
            root()
                .join(format!("tests/fixtures/cir/{fixture}.cir.pb"))
                .to_str()
                .unwrap(),
            "--source-root",
            root().join("examples/go-branch-lab").to_str().unwrap(),
        ]);
        let other = analyze(&cir, &name, unroll, partial, expected);
        let statuses = |v: &Value| {
            v["ir_outcomes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|o| o["status"].clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            statuses(&actual),
            statuses(&other),
            "Go/CIR semantic parity"
        );
    }
}

#[test]
fn analysis_decide() {
    row("Decide", Some("decide"), 3, false, &[P, P, PU, P]);
}
#[test]
fn analysis_abs() {
    row("Abs", None, 3, false, &[P, P]);
}
#[test]
fn analysis_adjusted() {
    row("Adjusted", None, 3, false, &[P, P]);
}
#[test]
fn analysis_classify() {
    row("Classify", None, 3, false, &[P, P, P, P]);
}
#[test]
fn analysis_check() {
    row("Check", Some("check"), 3, false, &[P, P, P, P]);
}
#[test]
fn analysis_addone() {
    let lab = Lab::new();
    analyze(&lab, "example.AddOne", 3, false, &[P, P]);
    let witnesses = rows(
        &lab.conn(),
        "SELECT c.witness_json,c.validation_method FROM branch_cases c JOIN branch_analysis_results r ON r.id=c.result_id JOIN ir_outcomes o ON o.id=r.ir_outcome_id WHERE o.ordinal=0 ORDER BY c.id",
    );
    assert!(
        witnesses
            .iter()
            .any(|w| serde_json::from_str::<Value>(&w[0]).unwrap()
                == serde_json::json!({"x":"127"})
                && w[1] == "SMT_MODEL"),
        "{witnesses:?}"
    );
}
#[test]
fn analysis_day() {
    row("Day", None, 3, false, &[P, P, P, P, P, P]);
}
#[test]
fn analysis_count3_u3() {
    row("Count3", None, 3, false, &[P, P]);
}
#[test]
fn analysis_count3_u2() {
    let lab = Lab::new();
    let result = analyze(&lab, "example.Count3", 2, true, &[P, "UNKNOWN"]);
    assert!(result["ir_outcomes"][1]["explanation"]
        .as_str()
        .unwrap()
        .contains("EXPLORATION_NOT_CLOSED"));
}
#[test]
fn analysis_sum_u3() {
    let lab = Lab::new();
    analyze(&lab, "example.Sum", 3, true, &[P, P]);
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ske"))
        .env_remove("SKE_DB")
        .args([
            "branches",
            "analyze",
            "--entry",
            "example.Sum",
            "--require-complete",
            "--format",
            "json",
            "--db",
        ])
        .arg(&lab.db)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["run_status"], "PARTIAL");
    assert_eq!(v["exploration_closed"], false);
}
#[test]
fn analysis_mem_nocall() {
    row("MemNoCall", Some("mem_nocall"), 3, false, &[P, P, PU, P]);
}
#[test]
fn analysis_mem_opaque() {
    row("MemOpaque", Some("mem_opaque"), 3, false, &[P, P, P, P]);
    let lab = Lab::new();
    let result = analyze(&lab, "example.MemOpaque", 3, false, &[P, P, P, P]);
    for (o, soundness) in result["ir_outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .zip(["EXACT", "EXACT", "OPAQUE", "OPAQUE"])
    {
        assert_eq!(o["soundness"], soundness);
    }
}
#[test]
fn analysis_mem_alias() {
    row("MemAlias", None, 3, false, &[P, P]);
}
#[test]
fn analysis_mem_alloc() {
    row("MemAlloc", None, 3, false, &[P, PU]);
}
#[test]
fn analysis_hasa() {
    let lab = Lab::new();
    let result = analyze(&lab, "example.HasA", 3, false, &[P, P]);
    for o in result["ir_outcomes"].as_array().unwrap() {
        assert_eq!(o["soundness"], "OPAQUE");
    }
}
#[test]
fn analysis_parse() {
    row("Parse", None, 3, false, &[P, P]);
}
#[test]
fn analysis_divmod() {
    row("DivMod", None, 3, false, &[P, P]);
}
