use super::*;

pub(crate) fn file(u: &Unit<'_>, v: &SourceFileDef, root: Option<&Path>) -> Result<()> {
    if let Some(root) = root {
        let canonical_root = root
            .canonicalize()
            .map_err(|e| err("E_SOURCE_HASH_MISMATCH", e.to_string()))?;
        let path = root
            .join(&v.path)
            .canonicalize()
            .map_err(|e| err("E_SOURCE_HASH_MISMATCH", format!("{}: {e}", v.path)))?;
        if !path.starts_with(&canonical_root) {
            return Err(err("E_PATH_INVALID", "source symlink escapes source root"));
        }
        if !path.is_file() {
            return Err(err(
                "E_SOURCE_HASH_MISMATCH",
                "source must be a regular file",
            ));
        }
        let file = std::fs::File::open(path)
            .map_err(|e| err("E_SOURCE_HASH_MISMATCH", format!("{}: {e}", v.path)))?;
        let hash = ske_common::sha256_reader(file)
            .map_err(|e| err("E_SOURCE_HASH_MISMATCH", format!("{}: {e}", v.path)))?;
        if hash != v.content_sha256 {
            return Err(err("E_SOURCE_HASH_MISMATCH", &v.path));
        }
    }
    let old: Option<(i64, String)> =
        u.db.query_row(
            "SELECT id,content_hash FROM source_files WHERE snapshot_id=?1 AND path=?2",
            params![u.snapshot, v.path],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(sql)?;
    if let Some((_, hash)) = old {
        if hash != v.content_sha256 {
            return Err(err("E_SOURCE_HASH_MISMATCH", &v.path));
        }
    } else {
        u.db.execute("INSERT INTO source_files(id,snapshot_id,path,language,content_hash) VALUES(?1,?2,?3,?4,?5)",params![u.get("file",&v.path)?,u.snapshot,v.path,v.language,v.content_sha256]).map_err(sql)?;
    }
    Ok(())
}

pub(crate) fn span(u: &Unit<'_>, span: Option<&Span>) -> Result<Option<i64>> {
    let Some(s) = span else { return Ok(None) };
    let file = u.get("file", &s.path)?;
    u.db.execute("INSERT INTO source_spans(source_file_id,start_line,start_column,end_line,end_column) VALUES(?1,?2,?3,?4,?5) ON CONFLICT DO NOTHING",params![file,s.start_line,s.start_column,s.end_line,s.end_column]).map_err(sql)?;
    u.db.query_row("SELECT id FROM source_spans WHERE source_file_id=?1 AND start_line=?2 AND start_column=?3 AND end_line=?4 AND end_column=?5",params![file,s.start_line,s.start_column,s.end_line,s.end_column],|r|r.get(0)).map(Some).map_err(sql)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn diagnostic(
    u: &Unit<'_>,
    severity: &str,
    code: &str,
    impact: &str,
    message: &str,
    function: Option<i64>,
    span: Option<i64>,
) -> Result<()> {
    u.db.execute("INSERT INTO diagnostics(snapshot_id,unit_id,function_id,source_span_id,severity,code,impact,message) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![u.snapshot,u.id,function,span,severity,code,impact,message]).map_err(sql)?;
    Ok(())
}

pub(crate) fn record(u: &Unit<'_>, c: Content) -> Result<()> {
    let db = u.db;
    match c {
        Content::TypeDef(v) => {
            db.execute("INSERT INTO cir_types(id,snapshot_id,unit_id,local_key,kind,descriptor,payload) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![u.get("type",&v.id)?,u.snapshot,u.id,u.local(&v.id),v.kind,v.canonical_descriptor,v.encode_to_vec()]).map_err(sql)?;
        }
        Content::SymbolDef(v) => {
            db.execute("INSERT INTO symbols(id,snapshot_id,unit_id,local_key,owner_symbol_id,name,qualified_name,kind,type_id,primary_span_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![u.get("symbol",&v.id)?,u.snapshot,u.id,u.local(&v.id),u.optional("symbol",&v.owner_id)?,v.name,v.qualified_name,v.kind,u.optional("type",&v.type_id)?,span(u,v.spans.first())?]).map_err(sql)?;
        }
        Content::FunctionDef(v) => {
            db.execute("INSERT INTO functions(id,symbol_id,entry_block_id,return_type_ids,is_external,inventory_status) VALUES(?1,?2,?3,?4,?5,'COMPLETE')",params![u.get("function",&v.id)?,u.get("symbol",&v.symbol_id)?,u.optional("block",&v.entry_block_id)?,serde_json::to_string(&v.return_type_ids).unwrap(),v.is_external]).map_err(sql)?;
        }
        Content::ValueDef(v) => {
            db.execute("INSERT INTO values_ir(id,snapshot_id,unit_id,function_id,local_key,type_id,value_role,name,symbol_id,literal,param_index) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",params![u.get("value",&v.id)?,u.snapshot,u.id,u.optional("function",&v.function_id)?,u.local(&v.id),u.get("type",&v.type_id)?,v.role,v.name,u.optional("symbol",&v.symbol_id)?,v.literal.as_ref().map(Message::encode_to_vec),v.param_index]).map_err(sql)?;
        }
        Content::BlockDef(v) => {
            db.execute(
                "INSERT INTO basic_blocks(id,function_id,local_key,ordinal) VALUES(?1,?2,?3,?4)",
                params![
                    u.get("block", &v.id)?,
                    u.get("function", &v.function_id)?,
                    u.local(&v.id),
                    v.ordinal
                ],
            )
            .map_err(sql)?;
        }
        Content::InstructionDef(v) => {
            let id = u.get("instruction", &v.id)?;
            db.execute("INSERT INTO instructions(id,block_id,local_key,ordinal,opcode,result_value_id,is_terminator,payload,primary_span_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id,u.get("block",&v.block_id)?,u.local(&v.id),v.ordinal,v.opcode,u.optional("value",&v.result_value_id)?,v.is_terminator,v.payload.as_ref().map(Message::encode_to_vec),span(u,v.spans.first())?]).map_err(sql)?;
            for (n, value) in v.operand_value_ids.iter().enumerate() {
                db.execute(
                    "INSERT INTO instruction_operands VALUES(?1,?2,?3)",
                    params![id, n as i64, u.get("value", value)?],
                )
                .map_err(sql)?;
            }
            if let Some(p) = v.payload {
                if let Some(operation_payload::Details::Opaque(ref opaque)) = p.details {
                    if opaque.may_mutate_memory == Some(false)
                        || opaque.may_exit_abnormally == Some(false)
                    {
                        diagnostic(
                            u,
                            "INFO",
                            "ADAPTER_EFFECT_CLAIMS_TRUSTED",
                            "SYMBOLIC",
                            &format!(
                                "adapter effect assertion for instruction {} in unit {}",
                                v.id, u.key
                            ),
                            None,
                            None,
                        )?;
                    }
                }
                for (n, input) in p.phi_inputs.iter().enumerate() {
                    db.execute(
                        "INSERT INTO phi_inputs VALUES(?1,?2,?3,?4)",
                        params![
                            id,
                            n as i64,
                            u.get("block", &input.predecessor_block_id)?,
                            u.get("value", &input.value_id)?
                        ],
                    )
                    .map_err(|_| err("E_PHI_PRED_MISMATCH", "duplicate PHI predecessor"))?;
                }
            }
        }
        Content::ControlEdgeDef(v) => {
            let id = u.get("edge", &v.id)?;
            db.execute("INSERT INTO control_edges(id,local_key,source_block_id,target_block_id,terminator_instruction_id,ordinal,label,edge_kind,guard_value_id,guard_negated,exception_filter_type_id,exception_catch_all) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![id,u.local(&v.id),u.get("block",&v.source_block_id)?,u.get("block",&v.target_block_id)?,u.get("instruction",&v.terminator_instruction_id)?,v.outcome_ordinal,v.outcome_label,v.edge_kind,u.optional("value",&v.guard_value_id)?,v.guard_negated,u.optional("type",&v.exception_filter_type_id)?,v.exception_catch_all]).map_err(sql)?;
            for (n, value) in v.case_value_ids.iter().enumerate() {
                db.execute(
                    "INSERT INTO control_edge_case_values VALUES(?1,?2,?3)",
                    params![id, n as i64, u.get("value", value)?],
                )
                .map_err(sql)?;
            }
        }
        Content::SourceDecisionDef(v) => {
            let id = u.get("decision", &v.id)?;
            db.execute("INSERT INTO source_decisions(id,snapshot_id,unit_id,function_id,parent_decision_id,local_key,kind,source_span_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![id,u.snapshot,u.id,u.optional("function",&v.function_id)?,u.optional("decision",&v.parent_decision_id)?,u.local(&v.id),v.decision_kind,span(u,v.span.as_ref())?]).map_err(sql)?;
            for (n, o) in v.outcomes.iter().enumerate() {
                db.execute("INSERT INTO source_outcomes(id,decision_id,local_key,ordinal,label,elided,activation_mapping_complete) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![u.get("outcome",&o.id)?,id,u.local(&o.id),n as i64,o.label,o.elided,o.activation_mapping_complete]).map_err(sql)?;
            }
        }
        Content::ImplicitExitSite(v) => {
            db.execute("INSERT INTO implicit_exit_sites(id,snapshot_id,unit_id,function_id,source_span_id,local_key,exit_kind,modeling_status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![u.get("exit",&v.id)?,u.snapshot,u.id,u.get("function",&v.function_id)?,span(u,v.span.as_ref())?,u.local(&v.id),v.exit_kind,v.modeling_status]).map_err(sql)?;
        }
        Content::Diagnostic(v) => diagnostic(
            u,
            &v.severity,
            &v.code,
            &v.impact,
            &v.message,
            u.optional("function", &v.affected_function_id)?,
            span(u, v.span.as_ref())?,
        )?,
        _ => (),
    }
    Ok(())
}

pub(crate) fn derive(u: &Unit<'_>) -> Result<()> {
    let db = u.db;
    db.execute("INSERT INTO ir_decisions(terminator_instruction_id,kind) SELECT i.id,i.opcode FROM instructions i JOIN basic_blocks b ON b.id=i.block_id JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=?1 AND i.opcode IN ('BRANCH','SWITCH','INVOKE') AND (SELECT COUNT(*) FROM control_edges WHERE terminator_instruction_id=i.id)>=2",[u.id]).map_err(sql)?;
    db.execute("INSERT INTO ir_outcomes(decision_id,control_edge_id,ordinal,label) SELECT d.id,e.id,e.ordinal,e.label FROM ir_decisions d JOIN control_edges e ON e.terminator_instruction_id=d.terminator_instruction_id JOIN instructions i ON i.id=d.terminator_instruction_id JOIN basic_blocks b ON b.id=i.block_id JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=?1 ORDER BY d.id,e.ordinal",[u.id]).map_err(sql)?;
    fn outcome(u: &Unit<'_>, edge: &str) -> Result<i64> {
        u.db.query_row(
            "SELECT id FROM ir_outcomes WHERE control_edge_id=?1",
            [u.get("edge", edge)?],
            |r| r.get(0),
        )
        .map_err(|_| {
            err(
                "E_SOURCE_ACTIVATION_INVALID",
                format!("edge {edge} is not an IR outcome"),
            )
        })
    }
    u.records(|c| {
        match c{
            Content::SourceIrLink(v)=>{
                db.execute("INSERT INTO source_ir_links VALUES(?1,?2,?3,?4,?5)",params![u.get("outcome",&v.source_outcome_id)?,outcome(u,&v.control_edge_id)?,v.mapping_kind,v.quality,v.inverted]).map_err(sql)?;
            }
            Content::SourceOutcomeActivationCase(v)=>{
                if v.ordered_control_edge_ids.is_empty(){return Err(err("E_SOURCE_ACTIVATION_INVALID","empty activation case"));}
                db.execute("INSERT INTO source_outcome_activation_cases(source_outcome_id,case_ordinal,quality) VALUES(?1,?2,?3)",params![u.get("outcome",&v.source_outcome_id)?,v.case_ordinal,v.quality]).map_err(|_|err("E_SOURCE_ACTIVATION_INVALID","duplicate activation ordinal"))?;
                let case=db.last_insert_rowid();
                for (n,edge) in v.ordered_control_edge_ids.iter().enumerate(){
                    db.execute("INSERT INTO source_outcome_activation_terms VALUES(?1,?2,?3)",params![case,n as i64,outcome(u,edge)?]).map_err(sql)?;
                }
            }
            Content::InstructionDef(v) if matches!(v.opcode.as_str(),"CALL"|"INVOKE"|"DEFER_PUSH"|"SPAWN")=>{
                let Some(operation_payload::Details::Call(call))=v.payload.and_then(|p|p.details) else{return Err(err("E_PAYLOAD_MISSING","call payload required"));};
                let instruction=u.get("instruction",&v.id)?;
                db.execute("INSERT INTO call_sites(instruction_id,dispatch_kind,resolution_status,world_closed,may_raise,may_mutate_memory,is_pure_proven,is_referentially_transparent_proven) VALUES(?1,?2,'UNRESOLVED',?3,?4,?5,?6,?7)",params![instruction,call.dispatch_kind,call.world_closed,call.may_raise.unwrap_or(true),call.may_mutate_memory.unwrap_or(true),call.is_pure_proven,call.is_referentially_transparent_proven]).map_err(sql)?;
                if call.may_raise==Some(false)||call.may_mutate_memory==Some(false){
                    diagnostic(u,"INFO","ADAPTER_EFFECT_CLAIMS_TRUSTED","SYMBOLIC",&format!("adapter effect assertion for instruction {} in unit {}",v.id,u.key),None,None)?;
                }
            }
            _=>(),
        } Ok(())
    })?;
    u.records(|c|{
        if let Content::CallTargetDef(v)=c {
            db.execute("INSERT INTO call_targets SELECT cs.id,?2,?3 FROM call_sites cs WHERE instruction_id=?1",params![u.get("instruction",&v.call_instruction_id)?,u.get("function",&v.target_function_id)?,v.resolution_kind]).map_err(sql)?;
        }Ok(())
    })?;
    db.execute("UPDATE call_sites SET resolution_status=CASE WHEN EXISTS(SELECT 1 FROM call_targets t WHERE t.call_site_id=call_sites.id) THEN CASE WHEN NOT EXISTS(SELECT 1 FROM call_targets t JOIN functions f ON f.id=t.target_function_id WHERE t.call_site_id=call_sites.id AND f.is_external=0) THEN 'EXTERNAL' WHEN (SELECT COUNT(*) FROM call_targets t WHERE t.call_site_id=call_sites.id)=1 AND dispatch_kind='STATIC' AND world_closed=1 THEN 'RESOLVED_UNIQUE' ELSE 'RESOLVED_POSSIBLE' END WHEN EXISTS(SELECT 1 FROM instruction_operands o JOIN values_ir v ON v.id=o.value_id JOIN functions f ON f.symbol_id=v.symbol_id WHERE o.instruction_id=call_sites.instruction_id AND o.operand_index=0 AND v.value_role='GLOBAL' AND f.is_external=1) THEN 'EXTERNAL' ELSE 'UNRESOLVED' END WHERE instruction_id IN (SELECT i.id FROM instructions i JOIN basic_blocks b ON b.id=i.block_id JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=?1)",[u.id]).map_err(sql)?;
    let bad:i64=db.query_row("SELECT COUNT(*) FROM source_outcomes o JOIN source_decisions d ON d.id=o.decision_id WHERE d.unit_id=?1 AND ((o.activation_mapping_complete=1 AND (NOT EXISTS(SELECT 1 FROM source_outcome_activation_cases c WHERE c.source_outcome_id=o.id) OR EXISTS(SELECT 1 FROM source_outcome_activation_cases c WHERE c.source_outcome_id=o.id AND c.quality<>'EXACT'))) OR EXISTS(SELECT 1 FROM source_outcome_activation_cases c WHERE c.source_outcome_id=o.id GROUP BY c.source_outcome_id HAVING MIN(c.case_ordinal)<>0 OR MAX(c.case_ordinal)+1<>COUNT(*)))",[u.id],|r|r.get(0)).map_err(sql)?;
    if bad > 0 {
        return Err(err(
            "E_SOURCE_ACTIVATION_INVALID",
            "activation cases incomplete, nonexact or noncontiguous",
        ));
    }
    db.execute("INSERT INTO diagnostics(snapshot_id,unit_id,function_id,source_span_id,severity,code,impact,message)
        SELECT d.snapshot_id,d.unit_id,d.function_id,d.source_span_id,'WARNING','SOURCE_MISMATCH','INVENTORY','source outcome without IR provenance: '||o.local_key
        FROM source_outcomes o JOIN source_decisions d ON d.id=o.decision_id
        WHERE d.unit_id=?1 AND o.elided=0 AND NOT EXISTS(SELECT 1 FROM source_ir_links l WHERE l.source_outcome_id=o.id)",[u.id]).map_err(sql)?;
    db.execute("INSERT INTO diagnostics(snapshot_id,unit_id,function_id,source_span_id,severity,code,impact,message)
        SELECT d.snapshot_id,d.unit_id,d.function_id,d.source_span_id,'INFO','SOURCE_ACTIVATION_INCOMPLETE','SYMBOLIC','source activation is not complete: '||o.local_key
        FROM source_outcomes o JOIN source_decisions d ON d.id=o.decision_id
        WHERE d.unit_id=?1 AND o.elided=0 AND o.activation_mapping_complete=0
        AND NOT EXISTS(SELECT 1 FROM diagnostics x WHERE x.unit_id=d.unit_id AND x.code='SOURCE_ACTIVATION_INCOMPLETE')",[u.id]).map_err(sql)?;
    Ok(())
}
