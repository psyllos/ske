use super::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{BufReader, Read};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fixtures() -> Vec<(&'static str, Vec<Record>)> {
    let lab = root().join("examples/go-branch-lab");
    vec![
        ("decide", decide(&lab, false, false).unwrap()),
        ("check", check(&lab).unwrap()),
        ("adjusted", adjusted(&lab).unwrap()),
        ("switch3", switch3(&lab).unwrap()),
        ("invoke", invoke()),
        ("mem_nocall", decide(&lab, true, false).unwrap()),
        ("mem_opaque", decide(&lab, true, true).unwrap()),
    ]
}
fn bytes(records: &[Record]) -> Vec<u8> {
    let mut bytes = vec![];
    for r in records {
        write_record(&mut bytes, r).unwrap();
    }
    bytes
}

#[test]
fn fixture_big_exact_size_and_framing() {
    let path = std::env::temp_dir().join(format!("ske-big-test-{}.cir.pb", std::process::id()));
    write_big(&path).unwrap();
    let mut input = BufReader::new(fs::File::open(&path).unwrap());
    let mut functions = 0;
    let mut blocks = 0;
    let mut instructions = 0;
    let mut values = 0;
    let mut headers = 0;
    let mut ends = 0;
    loop {
        let mut prefix = [0; 4];
        if input.read(&mut prefix[..1]).unwrap() == 0 {
            break;
        }
        input.read_exact(&mut prefix[1..]).unwrap();
        let size = u32::from_be_bytes(prefix) as usize;
        assert!((1..4096).contains(&size));
        let mut bytes = vec![0; size];
        input.read_exact(&mut bytes).unwrap();
        let r = Record::decode(bytes.as_slice()).unwrap();
        assert_eq!(ends, 0, "record after EndUnit");
        match r.content.unwrap() {
            Content::Header(h) => {
                assert_eq!(headers, 0);
                assert_eq!(h.cir_major, 1);
                headers += 1;
            }
            Content::FunctionDef(f) => {
                assert!(!f.is_external);
                assert_eq!(f.parameter_value_ids, vec![format!("{}/v0", f.id)]);
                functions += 1;
            }
            Content::BlockDef(b) => {
                assert_eq!(b.ordinal, 0);
                blocks += 1;
            }
            Content::ValueDef(v) => {
                assert_eq!(v.type_id, INT);
                values += 1;
            }
            Content::InstructionDef(i) => {
                let ordinal = instructions % 100;
                assert_eq!(i.ordinal, ordinal);
                let function = i.block_id.strip_suffix("/b0").unwrap();
                assert_eq!(i.operand_value_ids, vec![format!("{function}/v{ordinal}")]);
                assert_eq!(i.is_terminator, ordinal == 99);
                assert_eq!(i.opcode, if ordinal == 99 { "RETURN" } else { "NEG" });
                if ordinal != 99 {
                    assert_eq!(i.result_value_id, format!("{function}/v{}", ordinal + 1));
                }
                instructions += 1;
            }
            Content::EndUnit(e) => {
                assert_eq!(e.emitted_ir_edges, 0);
                assert_eq!(e.emitted_source_decisions, 0);
                ends += 1;
            }
            _ => {}
        }
    }
    assert_eq!(
        (headers, ends, functions, blocks, values, instructions),
        (1, 1, 2000, 2000, 200_000, 200_000)
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn fixture_positive_local_contract() {
    for (name, records) in fixtures() {
        let types: HashMap<_, _> = records
            .iter()
            .filter_map(|r| match &r.content {
                Some(Content::TypeDef(d)) => Some((d.id.clone(), d.clone())),
                _ => None,
            })
            .collect();
        let values: HashMap<_, _> = records
            .iter()
            .filter_map(|r| match &r.content {
                Some(Content::ValueDef(d)) => Some((d.id.clone(), d.clone())),
                _ => None,
            })
            .collect();
        for r in &records {
            match &r.content {
                Some(Content::TypeDef(d)) => {
                    ske_cir::validate_type(d).unwrap_or_else(|e| panic!("{name} {}: {e}", d.id))
                }
                Some(Content::ValueDef(d)) => ske_cir::validate_value(d, &types)
                    .unwrap_or_else(|e| panic!("{name} {}: {e}", d.id)),
                Some(Content::InstructionDef(d)) => {
                    ske_cir::validate_instruction(d, &types, &values)
                        .unwrap_or_else(|e| panic!("{name} {}: {e}", d.id));
                    let edges = records
                        .iter()
                        .filter_map(|r| match &r.content {
                            Some(Content::ControlEdgeDef(e))
                                if e.terminator_instruction_id == d.id =>
                            {
                                Some(e.clone())
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    ske_cir::validate_edges(d, &edges, &values)
                        .unwrap_or_else(|e| panic!("{name} {}: {e}", d.id));
                }
                _ => {}
            }
        }
    }
}

fn counts(records: &[Record]) -> BTreeMap<&'static str, u64> {
    let names = [
        "source_files",
        "source_spans",
        "cir_types",
        "symbols",
        "functions",
        "values_ir",
        "basic_blocks",
        "instructions",
        "instruction_operands",
        "phi_inputs",
        "control_edges",
        "control_edge_case_values",
        "source_decisions",
        "source_outcomes",
        "ir_decisions",
        "ir_outcomes",
        "source_ir_links",
        "source_outcome_activation_cases",
        "source_outcome_activation_terms",
        "call_sites",
        "call_targets",
        "diagnostics",
    ];
    let mut counts: BTreeMap<_, _> = names.into_iter().map(|s| (s, 0)).collect();
    let mut spans = BTreeSet::new();
    for r in records {
        let mut add_spans = |ss: &[Span]| {
            for s in ss {
                spans.insert((
                    s.path.clone(),
                    s.start_line,
                    s.start_column,
                    s.end_line,
                    s.end_column,
                ));
            }
        };
        match r.content.as_ref().unwrap() {
            Content::SourceFileDef(_) => *counts.get_mut("source_files").unwrap() += 1,
            Content::TypeDef(_) => *counts.get_mut("cir_types").unwrap() += 1,
            Content::SymbolDef(d) => {
                *counts.get_mut("symbols").unwrap() += 1;
                add_spans(&d.spans);
            }
            Content::FunctionDef(_) => *counts.get_mut("functions").unwrap() += 1,
            Content::ValueDef(_) => *counts.get_mut("values_ir").unwrap() += 1,
            Content::BlockDef(_) => *counts.get_mut("basic_blocks").unwrap() += 1,
            Content::InstructionDef(d) => {
                *counts.get_mut("instructions").unwrap() += 1;
                *counts.get_mut("instruction_operands").unwrap() +=
                    d.operand_value_ids.len() as u64;
                *counts.get_mut("phi_inputs").unwrap() +=
                    d.payload.as_ref().map_or(0, |p| p.phi_inputs.len()) as u64;
                if matches!(d.opcode.as_str(), "BRANCH" | "SWITCH" | "INVOKE") {
                    *counts.get_mut("ir_decisions").unwrap() += 1;
                }
                if matches!(
                    d.opcode.as_str(),
                    "CALL" | "INVOKE" | "SPAWN" | "DEFER_PUSH"
                ) {
                    *counts.get_mut("call_sites").unwrap() += 1;
                }
                add_spans(&d.spans);
            }
            Content::ControlEdgeDef(d) => {
                *counts.get_mut("control_edges").unwrap() += 1;
                *counts.get_mut("control_edge_case_values").unwrap() +=
                    d.case_value_ids.len() as u64;
                if d.edge_kind != "UNCONDITIONAL" {
                    *counts.get_mut("ir_outcomes").unwrap() += 1;
                }
            }
            Content::SourceDecisionDef(d) => {
                *counts.get_mut("source_decisions").unwrap() += 1;
                *counts.get_mut("source_outcomes").unwrap() += d.outcomes.len() as u64;
                add_spans(&d.span.clone().into_iter().collect::<Vec<_>>());
            }
            Content::SourceIrLink(_) => *counts.get_mut("source_ir_links").unwrap() += 1,
            Content::SourceOutcomeActivationCase(d) => {
                *counts.get_mut("source_outcome_activation_cases").unwrap() += 1;
                *counts.get_mut("source_outcome_activation_terms").unwrap() +=
                    d.ordered_control_edge_ids.len() as u64;
            }
            Content::CallTargetDef(_) => *counts.get_mut("call_targets").unwrap() += 1,
            Content::Diagnostic(_) => *counts.get_mut("diagnostics").unwrap() += 1,
            _ => {}
        }
    }
    counts.insert("source_spans", spans.len() as u64);
    counts
}

#[test]
fn fixture_counts_and_source_goldens() {
    for (name, records) in fixtures() {
        // These goldens are literal manual transcriptions, never generated by SKE.
        let golden: serde_json::Value = serde_json::from_slice(
            &fs::read(root().join(format!("tests/golden/cir/{name}.json"))).unwrap(),
        )
        .unwrap();
        let got = counts(&records);
        assert_eq!(
            serde_json::to_value(got).unwrap(),
            golden["counts"],
            "{name}"
        );
        let decisions = records
            .iter()
            .filter_map(|r| match &r.content {
                Some(Content::SourceDecisionDef(d)) => Some(d),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            decisions.len(),
            golden["source_decisions"].as_array().unwrap().len()
        );
        for (d, want) in decisions
            .into_iter()
            .zip(golden["source_decisions"].as_array().unwrap())
        {
            assert_eq!(d.id, want["id"].as_str().unwrap());
            assert_eq!(d.decision_kind, want["kind"].as_str().unwrap());
            let s = d.span.as_ref().unwrap();
            let source =
                fs::read_to_string(root().join("examples/go-branch-lab").join(&s.path)).unwrap();
            assert_eq!(s.start_line, s.end_line);
            let line = source.lines().nth(s.start_line as usize - 1).unwrap();
            assert_eq!(
                &line[s.start_column as usize - 1..s.end_column as usize - 1],
                want["cond_text"].as_str().unwrap()
            );
            assert_eq!(
                serde_json::to_value(d.outcomes.iter().map(|o| &o.label).collect::<Vec<_>>())
                    .unwrap(),
                want["outcomes"]
            );
        }
        assert_eq!(
            bytes(&records),
            fs::read(root().join(format!("tests/fixtures/cir/{name}.cir.pb"))).unwrap()
        );
    }
}

#[test]
fn fixture_negative_set_and_reproducibility() {
    assert_eq!(NEGATIVES.len(), 33);
    let temporary = std::env::temp_dir().join(format!("ske-fixtures-test-{}", std::process::id()));
    fs::create_dir_all(&temporary).unwrap();
    let all = fixtures();
    write_negatives(&temporary, &all[0].1, &all[2].1).unwrap();
    for code in NEGATIVES {
        let name = format!("bad_{code}.cir.pb");
        assert_eq!(
            fs::read(temporary.join(&name)).unwrap(),
            fs::read(root().join("tests/fixtures/cir").join(&name)).unwrap(),
            "{code}"
        );
    }
    fs::remove_dir_all(temporary).unwrap();
}
