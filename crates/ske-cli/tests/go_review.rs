mod go_support;
use go_support::*;

fn project(lab: &Lab, source: &str) {
    let repo = lab.dir.path().join("source");
    std::fs::create_dir(&repo).unwrap();
    std::fs::write(repo.join("go.mod"), "module review\n\ngo 1.22\n").unwrap();
    std::fs::write(repo.join("review.go"), source).unwrap();
    let p = lab.dir.path().join("profile.json");
    std::fs::write(&p, profile("linux", "amd64").to_string()).unwrap();
    lab.ok(&[
        "ingest",
        "go",
        "--repo",
        repo.to_str().unwrap(),
        "--adapter",
        adapter().to_str().unwrap(),
        "--yes-run-adapter",
        "--profile",
        p.to_str().unwrap(),
    ]);
    let report = lab.ok(&["db", "check"]);
    assert_eq!(report["ok"], true);
}

#[test]
fn go_nil_outside_core_literal_contract_is_explicitly_opaque() {
    let lab = Lab::empty();
    project(&lab, "package review\nfunc Nil() ([]int,func(),map[int]int,chan int,*int,interface{}) { return nil,nil,nil,nil,nil,nil }\n");
    let c = lab.conn();
    let fid = function(&c, "review.Nil");
    assert_eq!(
        count(
            &c,
            &format!(
                "SELECT count(*) FROM values_ir v JOIN cir_types t ON t.id=v.type_id
         WHERE v.function_id={fid} AND v.value_role='CONST'
         AND t.kind NOT IN ('POINTER','REFERENCE','INTERFACE')"
            )
        ),
        0
    );
    assert_eq!(
        count(
            &c,
            &format!(
                "SELECT count(*) FROM instructions i JOIN values_ir v ON v.id=i.result_value_id
         JOIN cir_types t ON t.id=v.type_id WHERE v.function_id={fid}
         AND i.opcode='OPAQUE_OP' AND v.value_role='SSA'
         AND t.kind IN ('SLICE','FUNCTION','OPAQUE')"
            )
        ),
        4
    );
    assert_eq!(
        count(
            &c,
            &format!(
                "SELECT count(*) FROM diagnostics WHERE function_id={fid}
         AND code='OPAQUE_OP_EMITTED' AND severity='INFO' AND impact='SYMBOLIC'"
            )
        ),
        1
    );
}

#[test]
fn go_activation_limit_is_persisted_without_shrinking_inventory() {
    let lab = Lab::empty();
    let params = (0..13)
        .map(|i| format!("a{i},b{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let condition = (0..13)
        .map(|i| format!("(a{i} || b{i})"))
        .collect::<Vec<_>>()
        .join(" && ");
    project(&lab, &format!("package review\nfunc E({params} bool) bool {{ if {condition} {{ return true }}; return false }}\n"));
    let c = lab.conn();
    let fid = function(&c, "review.E");
    assert_eq!(
        count(
            &c,
            &format!("SELECT count(*) FROM source_decisions WHERE function_id={fid}")
        ),
        27
    );
    assert_eq!(count(&c, &format!(
        "SELECT count(*) FROM source_outcomes o JOIN source_decisions d ON d.id=o.decision_id WHERE d.function_id={fid}"
    )), 54);
    assert_eq!(
        rows(
            &c,
            &format!(
                "SELECT o.activation_mapping_complete, count(a.id)
         FROM source_outcomes o JOIN source_decisions d ON d.id=o.decision_id
         JOIN source_outcome_activation_cases a ON a.source_outcome_id=o.id
         WHERE d.function_id={fid} AND d.kind='IF' GROUP BY o.id ORDER BY o.ordinal"
            )
        ),
        vec![vec!["0", "4096"], vec!["0", "4096"]]
    );
    assert!(
        count(
            &c,
            &format!(
                "SELECT count(*) FROM diagnostics WHERE function_id={fid}
         AND code='SOURCE_ACTIVATION_INCOMPLETE' AND severity='INFO' AND impact='SYMBOLIC'"
            )
        ) > 0
    );
}
