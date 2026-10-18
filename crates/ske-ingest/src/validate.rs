use super::*;
use std::collections::{HashMap, HashSet};

pub(crate) fn known(value: &str, allowed: &str) -> Result<()> {
    if allowed.split_whitespace().any(|s| s == value) {
        Ok(())
    } else {
        Err(err("E_UNKNOWN_KIND", value))
    }
}
fn path(p: &str) -> Result<()> {
    if p.is_empty()
        || p.starts_with('/')
        || p.contains(['\\', '\0'])
        || p.split('/').any(|s| s.is_empty() || s == "." || s == "..")
    {
        Err(err("E_PATH_INVALID", p))
    } else {
        Ok(())
    }
}
fn span(s: &Span) -> Result<()> {
    path(&s.path)?;
    if s.start_line == 0
        || s.start_column == 0
        || s.end_column == 0
        || s.end_line < s.start_line
        || (s.end_line == s.start_line && s.end_column < s.start_column)
    {
        Err(err("E_SPAN_INVALID", format!("{s:?}")))
    } else {
        Ok(())
    }
}
pub(crate) fn local(c: &Content) -> Result<()> {
    let kind = match c {
        Content::Header(_) => "Header",
        Content::UnitDef(_) => "UnitDef",
        Content::TypeDef(_) => "TypeDef",
        Content::SymbolDef(_) => "SymbolDef",
        Content::ValueDef(_) => "ValueDef",
        Content::FunctionDef(_) => "FunctionDef",
        Content::BlockDef(_) => "BlockDef",
        Content::InstructionDef(_) => "InstructionDef",
        Content::ControlEdgeDef(_) => "ControlEdgeDef",
        Content::SourceFileDef(_) => "SourceFileDef",
        Content::SourceDecisionDef(_) => "SourceDecisionDef",
        Content::SourceIrLink(_) => "SourceIrLink",
        Content::SourceOutcomeActivationCase(_) => "SourceOutcomeActivationCase",
        Content::ImplicitExitSite(_) => "ImplicitExitSite",
        Content::CallTargetDef(_) => "CallTargetDef",
        Content::Diagnostic(_) => "Diagnostic",
        Content::EndUnit(_) => "EndUnit",
    };
    let result = (|| -> Result<()> {
        match c {
        Content::TypeDef(v) => ske_cir::validate_type(v)?,
        Content::SymbolDef(v) => {
            known(
                &v.kind,
                "PACKAGE FUNCTION METHOD GLOBAL TYPE CONSTANT OTHER",
            )?;
            for s in &v.spans {
                span(s)?;
            }
        }
        Content::ValueDef(v) => {
            known(&v.role, "PARAM CONST SSA GLOBAL MEMORY")?;
            if (v.role == "PARAM") != v.param_index.is_some() {
                return Err(err("E_PARAM_ORDER", &v.id));
            }
            if (v.role == "CONST") != v.literal.is_some()
                || (v.role == "GLOBAL") != !v.symbol_id.is_empty()
            {
                return Err(err("E_TYPE_MISMATCH", &v.id));
            }
        }
        Content::InstructionDef(v) => {
            if ske_cir::registry::lookup(&v.opcode).is_none() {
                return Err(err("E_UNKNOWN_OPCODE", &v.opcode));
            }
            for s in &v.spans {
                span(s)?;
            }
        }
        Content::ControlEdgeDef(v) => known(
            &v.edge_kind,
            "UNCONDITIONAL BRANCH_TRUE BRANCH_FALSE SWITCH_CASE SWITCH_DEFAULT NORMAL EXCEPTION",
        )?,
        Content::SourceFileDef(v) => path(&v.path)?,
        Content::SourceDecisionDef(v) => {
            if !v.decision_kind.starts_with("X_") {
                known(&v.decision_kind,"IF LOGICAL_OPERAND SWITCH TYPE_SWITCH SELECT LOOP_CONDITION RANGE_ITERATION MATCH TERNARY TRY_CATCH ERROR_PROPAGATION")?;
            }
            if let Some(s) = &v.span {
                span(s)?;
            }
        }
        Content::SourceIrLink(v) => {
            known(
                &v.mapping_kind,
                "DIRECT SHORT_CIRCUIT SWITCH_CASE LOOP_CONDITION IMPLICIT",
            )?;
            known(&v.quality, "EXACT APPROX HEURISTIC")?;
        }
        Content::SourceOutcomeActivationCase(v) => known(&v.quality, "EXACT APPROX HEURISTIC")?,
        Content::ImplicitExitSite(v) => {
            known(&v.modeling_status, "MODELED UNMODELED UNKNOWN")?;
            if let Some(s) = &v.span {
                span(s)?;
            }
        }
        Content::Diagnostic(v) => {
            known(&v.severity, "INFO WARNING ERROR")?;
            known(
                &v.impact,
                "INVENTORY SYMBOLIC EFFECTS CALLGRAPH TYPES CONCURRENCY OTHER",
            )?;
            if let Some(s) = &v.span {
                span(s)?;
            }
        }
        _ => (),
    }
        Ok(())
    })();
    result.map_err(|e| {
        let id = match c {
            Content::TypeDef(v) => v.id.clone(),
            Content::ValueDef(v) => v.id.clone(),
            Content::InstructionDef(v) => format!("{}:{}", v.id, v.opcode),
            _ => String::new(),
        };
        err(e.code.as_str(), format!("[{kind} {id}] {}", e.message))
    })
}
pub(crate) fn references(u: &Unit<'_>, c: &Content) -> Result<()> {
    let check = |space: &str, id: &str| u.get(space, id).map(|_| ());
    let opt = |space: &str, id: &str| u.optional(space, id).map(|_| ());
    let sp = |s: &Span| check("file", &s.path);
    match c {
        Content::TypeDef(v) => {
            for t in v.param_type_ids.iter().chain(&v.result_type_ids) {
                check("type", t)?;
            }
        }
        Content::SymbolDef(v) => {
            opt("symbol", &v.owner_id)?;
            opt("type", &v.type_id)?;
            for s in &v.spans {
                sp(s)?;
            }
        }
        Content::ValueDef(v) => {
            opt("function", &v.function_id)?;
            check("type", &v.type_id)?;
            opt("symbol", &v.symbol_id)?;
        }
        Content::FunctionDef(v) => {
            check("symbol", &v.symbol_id)?;
            for p in &v.parameter_value_ids {
                check("value", p)?;
            }
            for t in &v.return_type_ids {
                check("type", t)?;
            }
        }
        Content::BlockDef(v) => check("function", &v.function_id)?,
        Content::InstructionDef(v) => {
            check("block", &v.block_id)?;
            opt("value", &v.result_value_id)?;
            for o in &v.operand_value_ids {
                check("value", o)?;
            }
            for s in &v.spans {
                sp(s)?;
            }
            if let Some(p) = &v.payload {
                for phi in &p.phi_inputs {
                    check("block", &phi.predecessor_block_id)?;
                    check("value", &phi.value_id)?;
                }
                if let Some(operation_payload::Details::TypeTest(t)) = &p.details {
                    check("type", &t.target_type_id)?;
                }
            }
        }
        Content::ControlEdgeDef(v) => {
            check("block", &v.source_block_id)?;
            check("block", &v.target_block_id)?;
            check("instruction", &v.terminator_instruction_id)?;
            opt("value", &v.guard_value_id)?;
            opt("type", &v.exception_filter_type_id)?;
            for t in &v.case_value_ids {
                check("value", t)?;
            }
        }
        Content::SourceDecisionDef(v) => {
            opt("function", &v.function_id)?;
            opt("decision", &v.parent_decision_id)?;
            if let Some(s) = &v.span {
                sp(s)?;
            } else {
                return Err(err("E_SPAN_INVALID", "source decision requires span"));
            }
        }
        Content::SourceIrLink(v) => {
            check("outcome", &v.source_outcome_id)?;
            check("edge", &v.control_edge_id)?;
        }
        Content::SourceOutcomeActivationCase(v) => {
            check("outcome", &v.source_outcome_id)?;
            for e in &v.ordered_control_edge_ids {
                check("edge", e)?;
            }
        }
        Content::CallTargetDef(v) => {
            check("instruction", &v.call_instruction_id)?;
            check("function", &v.target_function_id)?;
        }
        Content::ImplicitExitSite(v) => {
            check("function", &v.function_id)?;
            if let Some(s) = &v.span {
                sp(s)?;
            }
        }
        Content::Diagnostic(v) => {
            opt("function", &v.affected_function_id)?;
            if let Some(s) = &v.span {
                sp(s)?;
            }
        }
        _ => (),
    }
    Ok(())
}

fn load(u: &Unit<'_>, query: &str, id: i64) -> Result<Vec<Content>> {
    let mut stmt = u.db.prepare(query).map_err(sql)?;
    let rows = stmt
        .query_map([id], |r| r.get::<_, Vec<u8>>(0))
        .map_err(sql)?;
    rows.map(|r| {
        Record::decode(r.map_err(sql)?.as_slice())
            .map_err(|e| err("E_PROTO_DECODE", e.to_string()))?
            .content
            .ok_or_else(|| err("E_PROTO_DECODE", "empty record"))
    })
    .collect()
}
fn count(u: &Unit<'_>, q: &str) -> Result<i64> {
    u.db.query_row(q, [u.id], |r| r.get(0)).map_err(sql)
}
pub(crate) fn unit(u: &Unit<'_>) -> Result<()> {
    // Literal/type checks apply even to unused values and external functions.
    // Decode one value and its type at a time, before structural validation.
    u.records(|content| {
        if let Content::ValueDef(value) = content {
            let bytes: Vec<u8> =
                u.db.query_row(
                    "SELECT payload FROM cir_types WHERE id=?1",
                    [u.get("type", &value.type_id)?],
                    |r| r.get(0),
                )
                .map_err(sql)?;
            let ty = TypeDef::decode(bytes.as_slice())
                .map_err(|e| err("E_PROTO_DECODE", e.to_string()))?;
            ske_cir::validate_value(&value, &HashMap::from([(ty.id.clone(), ty)]))?;
        }
        Ok(())
    })?;
    let mut statement=u.db.prepare("SELECT id FROM functions WHERE symbol_id IN (SELECT id FROM symbols WHERE unit_id=?1) ORDER BY id").map_err(sql)?;
    let ids: Vec<i64> = statement
        .query_map([u.id], |r| r.get(0))
        .map_err(sql)?
        .collect::<std::result::Result<_, _>>()
        .map_err(sql)?;
    // Structural checks run globally before opcode/type/SSA validation.
    for &(q,code) in &[
        ("SELECT COUNT(*) FROM basic_blocks b JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=?1 AND NOT EXISTS(SELECT 1 FROM instructions i WHERE i.block_id=b.id AND i.is_terminator=1)","E_NO_TERMINATOR"),
        ("SELECT COUNT(*) FROM instructions i JOIN basic_blocks b ON b.id=i.block_id JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=?1 AND i.is_terminator=1 AND EXISTS(SELECT 1 FROM instructions j WHERE j.block_id=i.block_id AND j.ordinal>i.ordinal)","E_TERMINATOR_NOT_LAST"),
        ("SELECT COUNT(*) FROM control_edges e JOIN instructions i ON i.id=e.terminator_instruction_id JOIN basic_blocks b ON b.id=i.block_id JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=?1 AND e.source_block_id<>i.block_id","E_EDGE_SOURCE_MISMATCH"),
        ("SELECT COUNT(*) FROM control_edges e JOIN basic_blocks a ON a.id=e.source_block_id JOIN basic_blocks b ON b.id=e.target_block_id JOIN functions f ON f.id=a.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=?1 AND a.function_id<>b.function_id","E_EDGE_CROSS_FUNCTION"),
    ] {if count(u,q)?>0{return Err(err(code,"structural invariant violated"));}}
    for id in ids {
        function(u, id)?;
    }
    Ok(())
}

fn function(u: &Unit<'_>, id: i64) -> Result<()> {
    let def = load(
        u,
        "SELECT body FROM temp.ske_records WHERE space='function' AND object_id=?1",
        id,
    )?
    .pop()
    .unwrap();
    let Content::FunctionDef(def) = def else {
        unreachable!()
    };
    let mut stmt =
        u.db.prepare("SELECT id,local_key FROM basic_blocks WHERE function_id=?1 ORDER BY ordinal")
            .map_err(sql)?;
    let blocks: Vec<(i64, String)> = stmt
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(sql)?
        .collect::<std::result::Result<_, _>>()
        .map_err(sql)?;
    let block_names: HashMap<String, usize> = blocks
        .iter()
        .enumerate()
        .map(|(n, (_, k))| (k.split_once('|').unwrap().1.to_owned(), n))
        .collect();
    let failed:bool=u.db.query_row("SELECT EXISTS(SELECT 1 FROM diagnostics WHERE function_id=?1 AND code='FUNCTION_TRANSLATION_FAILED')",[id],|r|r.get(0)).map_err(sql)?;
    if (def.is_external && (!blocks.is_empty() || !def.entry_block_id.is_empty()))
        || (!def.is_external
            && !block_names.contains_key(&def.entry_block_id)
            && !(failed && blocks.is_empty() && def.entry_block_id.is_empty()))
    {
        return Err(err("E_ENTRY_BLOCK", &def.id));
    }
    if def.is_external || failed {
        return Ok(());
    }
    let mut values = HashMap::new();
    for c in load(u,"SELECT body FROM temp.ske_records r WHERE space='value' AND object_id IN (
        SELECT id FROM values_ir WHERE function_id=?1 OR function_id IS NULL
        UNION SELECT i.result_value_id FROM instructions i JOIN basic_blocks b ON b.id=i.block_id WHERE b.function_id=?1 AND i.result_value_id IS NOT NULL
        UNION SELECT o.value_id FROM instruction_operands o JOIN instructions i ON i.id=o.instruction_id JOIN basic_blocks b ON b.id=i.block_id WHERE b.function_id=?1
        UNION SELECT p.value_id FROM phi_inputs p JOIN instructions i ON i.id=p.instruction_id JOIN basic_blocks b ON b.id=i.block_id WHERE b.function_id=?1
        UNION SELECT e.guard_value_id FROM control_edges e JOIN basic_blocks b ON b.id=e.source_block_id WHERE b.function_id=?1 AND e.guard_value_id IS NOT NULL
        UNION SELECT c.value_id FROM control_edge_case_values c JOIN control_edges e ON e.id=c.edge_id JOIN basic_blocks b ON b.id=e.source_block_id WHERE b.function_id=?1
    )",id)?{
        if let Content::ValueDef(v)=c{values.insert(v.id.clone(),v);}
    }
    let mut types = HashMap::new();
    // Types are the unit's compact schema, not function/record bodies.
    for c in load(
        u,
        "SELECT body FROM temp.ske_records WHERE space='type' AND ?1 IS NOT NULL",
        id,
    )? {
        if let Content::TypeDef(t) = c {
            types.insert(t.id.clone(), t);
        }
    }
    for (n, param) in def.parameter_value_ids.iter().enumerate() {
        let v = values
            .get(param)
            .ok_or_else(|| err("E_PARAM_ORDER", param))?;
        if v.function_id != def.id || v.role != "PARAM" || v.param_index != Some(n as u32) {
            return Err(err("E_PARAM_ORDER", param));
        }
    }
    if values
        .values()
        .filter(|v| v.role == "PARAM" && v.function_id == def.id)
        .count()
        != def.parameter_value_ids.len()
    {
        return Err(err("E_PARAM_ORDER", &def.id));
    }
    let instructions:Vec<InstructionDef>=load(u,"SELECT body FROM temp.ske_records WHERE space='instruction' AND object_id IN (SELECT i.id FROM instructions i JOIN basic_blocks b ON b.id=i.block_id WHERE b.function_id=?1) ORDER BY object_id",id)?.into_iter().filter_map(|c|if let Content::InstructionDef(i)=c{Some(i)}else{None}).collect();
    let edges:Vec<ControlEdgeDef>=load(u,"SELECT body FROM temp.ske_records WHERE space='edge' AND object_id IN (SELECT e.id FROM control_edges e JOIN basic_blocks b ON b.id=e.source_block_id WHERE b.function_id=?1)",id)?.into_iter().filter_map(|c|if let Content::ControlEdgeDef(e)=c{Some(e)}else{None}).collect();
    for i in &instructions {
        let mut outgoing: Vec<_> = edges
            .iter()
            .filter(|e| e.terminator_instruction_id == i.id)
            .cloned()
            .collect();
        outgoing.sort_by_key(|e| e.outcome_ordinal);
        if i.is_terminator {
            if matches!(i.opcode.as_str(), "BRANCH" | "SWITCH") && i.operand_value_ids.is_empty() {
                return Err(err("E_ARITY_MISMATCH", &i.id));
            }
            ske_cir::validate_edges(i, &outgoing, &values).map_err(|e| {
                err(
                    e.code.as_str(),
                    format!("[edges fn={} {}/{}] {}", def.id, i.id, i.opcode, e.message),
                )
            })?;
        }
    }
    for i in &instructions {
        ske_cir::validate_instruction(i, &types, &values).map_err(|e| {
            err(
                e.code.as_str(),
                format!("[instr fn={} {}/{}] {}", def.id, i.id, i.opcode, e.message),
            )
        })?;
        if i.opcode == "RETURN" {
            if i.operand_value_ids.len() != def.return_type_ids.len() {
                return Err(err("E_ARITY_MISMATCH", &i.id));
            }
            for (v, t) in i.operand_value_ids.iter().zip(&def.return_type_ids) {
                if values[v].type_id != *t {
                    return Err(err("E_TYPE_MISMATCH", &i.id));
                }
            }
        }
    }
    let graph = ske_graph::Graph::new(
        blocks.len(),
        edges.iter().map(|e| {
            (
                block_names[&e.source_block_id],
                block_names[&e.target_block_id],
            )
        }),
    )
    .map_err(|e| err("E_EDGE_SHAPE", e))?;
    let dom = graph.dominators(block_names[&def.entry_block_id]);
    let mut definitions: HashMap<&str, &InstructionDef> = HashMap::new();
    for i in &instructions {
        if !i.result_value_id.is_empty() {
            let v = &values[&i.result_value_id];
            if v.role != "SSA"
                || v.function_id != def.id
                || definitions.insert(&i.result_value_id, i).is_some()
            {
                return Err(err("E_SSA_MULTI_DEF", &i.id));
            }
        }
    }
    for v in values
        .values()
        .filter(|v| v.role == "SSA" && v.function_id == def.id)
    {
        if !definitions.contains_key(v.id.as_str()) {
            return Err(err("E_SSA_DOMINANCE", format!("undefined {}", v.id)));
        }
    }
    let check_use = |value: &str, block: &str, ordinal: u32| -> Result<()> {
        let v = values
            .get(value)
            .ok_or_else(|| err("E_SSA_DOMINANCE", value))?;
        if v.role != "GLOBAL" && v.function_id != def.id {
            return Err(err(
                "E_SSA_DOMINANCE",
                format!("{value} belongs to a different function"),
            ));
        }
        if !block_names.contains_key(block) {
            return Err(err("E_PHI_PRED_MISMATCH", block));
        }
        if v.role != "SSA" {
            return Ok(());
        }
        let d = definitions[value];
        if d.block_id == block {
            if d.ordinal >= ordinal {
                return Err(err("E_SSA_DOMINANCE", value));
            }
        } else if !dom.dominates(block_names[&d.block_id], block_names[block]) {
            return Err(err("E_SSA_DOMINANCE", value));
        }
        Ok(())
    };
    for i in &instructions {
        for value in &i.operand_value_ids {
            check_use(value, &i.block_id, i.ordinal)?;
        }
        if i.opcode == "PHI" {
            if let Some(p) = &i.payload {
                for input in &p.phi_inputs {
                    check_use(&input.value_id, &input.predecessor_block_id, u32::MAX)?;
                }
            }
        }
    }
    for i in instructions.iter().filter(|i| i.opcode == "PHI") {
        let expected: HashSet<_> = edges
            .iter()
            .filter(|e| e.target_block_id == i.block_id)
            .map(|e| e.source_block_id.as_str())
            .collect();
        let inputs = i
            .payload
            .as_ref()
            .map(|p| p.phi_inputs.as_slice())
            .unwrap_or_default();
        let actual: HashSet<_> = inputs
            .iter()
            .map(|p| p.predecessor_block_id.as_str())
            .collect();
        let preceded = instructions
            .iter()
            .any(|j| j.block_id == i.block_id && j.ordinal < i.ordinal && j.opcode != "PHI");
        if actual != expected
            || actual.len() != inputs.len()
            || preceded
            || inputs
                .iter()
                .any(|p| values[&p.value_id].type_id != values[&i.result_value_id].type_id)
        {
            return Err(err("E_PHI_PRED_MISMATCH", &i.id));
        }
    }
    Ok(())
}
