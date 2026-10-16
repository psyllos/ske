mod go_support;
use go_support::*;

fn width(c: &rusqlite::Connection, name: &str, bits: u64) {
    let fid = function(c, name);
    let payload: Vec<u8> = c.query_row("SELECT t.payload FROM values_ir v JOIN cir_types t ON t.id=v.type_id WHERE v.function_id=? AND v.value_role='PARAM' AND v.param_index=0", [fid], |r| r.get(0)).unwrap();
    let fields = wire_fields(&payload);
    assert_eq!(fields.get(&2).unwrap(), b"BITVECTOR", "{name}");
    assert_eq!(scalar(&fields, 5), bits, "{name}");
    assert_eq!(scalar(&fields, 6), 1, "{name}: signed");
}

#[test]
fn all_normative_ir_properties() {
    let lab = Lab::new();
    let c = lab.conn();
    assert_links(&c);
    assert_activation(&c);
    assert_dominance(&c);
    for g in golden() {
        let name = g["function"].as_str().unwrap();
        let fid = function(&c, name);
        let n = count(&c, &format!("SELECT COUNT(*) FROM ir_decisions d JOIN instructions i ON i.id=d.terminator_instruction_id JOIN basic_blocks b ON b.id=i.block_id WHERE b.function_id={fid}"));
        if let Some(expected) = g["ir_decisions"].as_i64() {
            assert_eq!(n, expected, "P-IRDEC-n {name}");
        } else if ["example.SumRange", "example.Pipe", "example.DaySwitch"].contains(&name) {
            assert!(n >= 1, "P-IRDEC-n {name}");
        }
    }
    let check = decisions(&c, function(&c, "example.Check"));
    for (o, edge) in outcomes(&c, &check[1][0])
        .iter()
        .zip(["BRANCH_FALSE", "BRANCH_TRUE"])
    {
        assert_eq!(rows(&c,&format!("SELECT e.edge_kind,l.inverted FROM source_ir_links l JOIN ir_outcomes o ON o.id=l.ir_outcome_id JOIN control_edges e ON e.id=o.control_edge_id WHERE l.source_outcome_id={}",o[0])),vec![vec![edge,"1"]],"P-INVERT-Check");
    }
    for (name, expected, mapping) in [
        ("example.And", vec![1, 2], "SHORT_CIRCUIT"),
        ("example.Day", vec![1, 2, 1], "SWITCH_CASE"),
    ] {
        let ds = decisions(&c, function(&c, name));
        for (i, o) in outcomes(&c, &ds[0][0]).iter().enumerate() {
            let links = rows(
                &c,
                &format!(
                    "SELECT mapping_kind FROM source_ir_links WHERE source_outcome_id={}",
                    o[0]
                ),
            );
            if name == "example.Day" && i != 1 {
                assert!(links.len() >= expected[i]);
            } else {
                assert_eq!(links.len(), expected[i], "{name}/o{i}");
            }
            assert!(links.iter().all(|l| l[0] == mapping), "{name}: mapping");
        }
    }
    let div = function(&c, "example.DivMod");
    let returns=rows(&c,&format!("SELECT i.id,COUNT(o.operand_index) FROM instructions i JOIN basic_blocks b ON b.id=i.block_id LEFT JOIN instruction_operands o ON o.instruction_id=i.id WHERE b.function_id={div} AND i.opcode='RETURN' GROUP BY i.id"));
    assert_eq!(returns.len(), 2, "P-TWORET");
    assert!(returns.iter().all(|r| r[1] == "2"), "P-TWORET");
    let use_div = function(&c, "example.UseDivMod");
    let mut statement = c.prepare("SELECT i.payload FROM instructions i JOIN basic_blocks b ON b.id=i.block_id WHERE b.function_id=? AND i.opcode='EXTRACT_FIELD'").unwrap();
    let indices: std::collections::BTreeSet<_> = statement
        .query_map([use_div], |r| r.get::<_, Vec<u8>>(0))
        .unwrap()
        .map(|r| {
            let fields = wire_fields(&r.unwrap());
            scalar(
                &wire_fields(fields.get(&7).expect("EXTRACT_FIELD field payload")),
                1,
            )
        })
        .collect();
    assert!(
        indices.contains(&0) && indices.contains(&1),
        "P-TWORET field indices"
    );
    let total = function(&c, "example.Total");
    assert_eq!(rows(&c,&format!("SELECT cs.dispatch_kind,cs.resolution_status,cs.world_closed FROM call_sites cs JOIN instructions i ON i.id=cs.instruction_id JOIN basic_blocks b ON b.id=i.block_id WHERE b.function_id={total} AND i.opcode='CALL'")),vec![vec!["INTERFACE","RESOLVED_POSSIBLE","0"]]);
    let targets=rows(&c,&format!("SELECT s.qualified_name FROM call_targets t JOIN functions f ON f.id=t.target_function_id JOIN symbols s ON s.id=f.symbol_id JOIN call_sites cs ON cs.id=t.call_site_id JOIN instructions i ON i.id=cs.instruction_id JOIN basic_blocks b ON b.id=i.block_id WHERE b.function_id={total}"));
    for target in ["example.Square.Area", "example.Rect.Area"] {
        assert!(
            targets.contains(&vec![target.to_owned()]),
            "P-INTERFACE-Total {target}"
        );
    }
    let hasa = function(&c, "example.HasA");
    assert_eq!(rows(&c,&format!("SELECT cs.dispatch_kind,cs.resolution_status,s.qualified_name,f.is_external FROM call_sites cs JOIN instructions i ON i.id=cs.instruction_id JOIN basic_blocks b ON b.id=i.block_id JOIN call_targets t ON t.call_site_id=cs.id JOIN functions f ON f.id=t.target_function_id JOIN symbols s ON s.id=f.symbol_id WHERE b.function_id={hasa} AND i.opcode='CALL'")),vec![vec!["STATIC","EXTERNAL","strings.HasPrefix","1"]],"P-EXT-HasA");
    width(&c, "example.AddOne", 8);
    width(&c, "example.Big", 64);
    for (name, opcode) in [
        ("example.Pipe", "SPAWN"),
        ("example.Pipe$1", "CHANNEL_SEND"),
    ] {
        let fid = function(&c, name);
        assert!(count(&c,&format!("SELECT COUNT(*) FROM instructions i JOIN basic_blocks b ON b.id=i.block_id WHERE b.function_id={fid} AND i.opcode='{opcode}'"))>0,"P-CONC-Pipe");
    }
    let listed = lab.ok(&["functions", "list"]);
    for name in ["example.Max", "example.Max[int]", "example.Max[string]"] {
        assert!(
            listed["functions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["qualified_name"] == name),
            "P-GEN {name}"
        );
    }
    let generic = function(&c, "example.Max");
    assert!(count(&c,&format!("SELECT COUNT(*) FROM diagnostics WHERE function_id={generic} AND code='GENERIC_ORIGIN_OPAQUE'"))>0);
    assert_negative_activation(&lab);
    let other = Lab::empty();
    other.ingest("linux", "386");
    width(&other.conn(), "example.Big", 32);
}
