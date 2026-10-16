mod go_support;
use go_support::*;
use serde_json::{json, Value};

fn projection(lab: &Lab, name: &str) -> Value {
    let c = lab.conn();
    let ds = decisions(&c, function(&c, name));
    let source: Vec<_>=ds.iter().map(|d| {
        let span=rows(&c,&format!("SELECT f.path,s.start_line,s.start_column,s.end_line,s.end_column FROM source_spans s JOIN source_files f ON f.id=s.source_file_id WHERE s.id={}",d[3]));
        let outcomes:Vec<_>=outcomes(&c,&d[0]).iter().map(|o| {
            // Project each link to its semantic edge, not SSA instruction ids.
            let mut links=rows(&c,&format!("SELECT e.edge_kind,l.mapping_kind,l.quality,l.inverted FROM source_ir_links l JOIN ir_outcomes io ON io.id=l.ir_outcome_id JOIN control_edges e ON e.id=io.control_edge_id WHERE l.source_outcome_id={} ORDER BY e.edge_kind,l.mapping_kind,l.inverted",o[0]));
            links.sort();
            json!({"ordinal":o[2],"label":o[3],"elided":o[4],"links":links})
        }).collect();
        json!({"kind":d[2],"span":span,"text":span_text(&c,d[3].parse().unwrap()),"outcomes":outcomes})
    }).collect();
    let fid = function(&c, name);
    let ir=rows(&c,&format!("SELECT e.edge_kind FROM ir_outcomes o JOIN control_edges e ON e.id=o.control_edge_id JOIN basic_blocks b ON b.id=e.source_block_id WHERE b.function_id={fid} ORDER BY e.edge_kind"));
    assert_eq!(ir.len(), 4, "independent four-IR-outcome oracle");
    json!({"source":source,"ir_edges":ir})
}

#[test]
fn go_matches_independently_handwritten_cir_fixtures() {
    let go = Lab::new();
    for fixture in ["decide", "check"] {
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
        let name = if fixture == "decide" {
            "example.Decide"
        } else {
            "example.Check"
        };
        assert_eq!(
            projection(&go, name),
            projection(&cir, name),
            "{name}: semantic Go/CIR projection"
        );
    }
}
