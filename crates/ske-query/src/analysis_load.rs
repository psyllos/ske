//! Reconstruct the pure engine input from the authoritative CIR storage.
use prost::Message;
use rusqlite::{Connection, OptionalExtension, Row};
use ske_cir::proto;
use ske_common::{Error, Result};
use ske_symbolic::program::{Block, Edge, Program};
use std::collections::BTreeMap;

fn integrity(error: impl std::fmt::Display) -> Error {
    Error::new("E_DB_INTEGRITY", error.to_string())
}

// Strip exactly the known unit prefix, not an arbitrary delimiter: unit keys
// themselves may contain '|'. Protobuf payloads already contain original IDs.
fn key(row: &Row<'_>, column: usize, prefix: &str) -> Result<String> {
    let stored: Option<String> = row.get(column).map_err(integrity)?;
    match stored {
        None => Ok(String::new()),
        Some(stored) => stored
            .strip_prefix(prefix)
            .map(str::to_owned)
            .ok_or_else(|| integrity(format!("local key outside unit: {stored}"))),
    }
}

fn blob<M: Message + Default>(row: &Row<'_>, column: usize) -> Result<Option<M>> {
    let bytes: Option<Vec<u8>> = row.get(column).map_err(integrity)?;
    bytes
        .map(|bytes| M::decode(bytes.as_slice()).map_err(integrity))
        .transpose()
}

fn collect<T>(
    db: &Connection,
    sql: &str,
    id: i64,
    mut read: impl FnMut(&Row<'_>) -> Result<T>,
) -> Result<Vec<T>> {
    let mut statement = db.prepare(sql).map_err(integrity)?;
    let mut rows = statement.query([id]).map_err(integrity)?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().map_err(integrity)? {
        result.push(read(row)?);
    }
    Ok(result)
}

/// Load one function and its unit-local types, symbols and global values.
/// The caller owns transaction/snapshot consistency and analysis persistence.
pub fn load(db: &Connection, function_id: i64) -> Result<Program> {
    let (unit_id, unit_key): (i64, String) = db
        .query_row(
            "SELECT s.unit_id,u.unit_key FROM functions f
             JOIN symbols s ON s.id=f.symbol_id
             JOIN compilation_units u ON u.id=s.unit_id WHERE f.id=?1",
            [function_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(integrity)?
        .ok_or_else(|| Error::new("E_NOT_FOUND", format!("function {function_id} not found")))?;
    let prefix = format!("{unit_key}|");
    let mut function = collect(
        db,
        "SELECT s.local_key,b.local_key,f.return_type_ids,f.is_external
         FROM functions f JOIN symbols s ON s.id=f.symbol_id
         LEFT JOIN basic_blocks b ON b.id=f.entry_block_id WHERE f.id=?1",
        function_id,
        |r| {
            let symbol_id = key(r, 0, &prefix)?;
            let returns: String = r.get(2).map_err(integrity)?;
            Ok(proto::FunctionDef {
                // §4.6 mandates function ID = symbol ID; no function payload
                // or local_key is persisted in the normative schema.
                id: symbol_id.clone(),
                symbol_id,
                entry_block_id: key(r, 1, &prefix)?,
                return_type_ids: serde_json::from_str(&returns).map_err(integrity)?,
                is_external: r.get(3).map_err(integrity)?,
                parameter_value_ids: Vec::new(),
            })
        },
    )?
    .into_iter()
    .next()
    .ok_or_else(|| integrity("function disappeared while loading"))?;

    let types = collect(
        db,
        "SELECT local_key,payload FROM cir_types WHERE unit_id=?1 ORDER BY local_key",
        unit_id,
        |r| {
            let id = key(r, 0, &prefix)?;
            let definition: proto::TypeDef =
                blob(r, 1)?.ok_or_else(|| integrity("missing type payload"))?;
            if definition.id != id {
                return Err(integrity("type payload/local key mismatch"));
            }
            Ok((id, definition))
        },
    )?
    .into_iter()
    .collect();
    let symbols = collect(
        db,
        "SELECT local_key,qualified_name FROM symbols WHERE unit_id=?1 ORDER BY local_key",
        unit_id,
        |r| Ok((key(r, 0, &prefix)?, r.get(1).map_err(integrity)?)),
    )?
    .into_iter()
    .collect();
    let values: BTreeMap<_, _> = collect(db,
        "SELECT v.local_key,t.local_key,v.value_role,v.name,s.local_key,v.literal,v.param_index,v.function_id
         FROM values_ir v JOIN cir_types t ON t.id=v.type_id
         LEFT JOIN symbols s ON s.id=v.symbol_id
         WHERE v.unit_id=(SELECT s.unit_id FROM functions f JOIN symbols s ON s.id=f.symbol_id WHERE f.id=?1)
         AND (v.function_id IS NULL OR v.function_id=?1)
         ORDER BY v.local_key",
        function_id, |r| {
            let id = key(r, 0, &prefix)?;
            let owner: Option<i64> = r.get(7).map_err(integrity)?;
            Ok((id.clone(), proto::ValueDef {
                id,
                function_id: owner.map(|_| function.id.clone()).unwrap_or_default(),
                type_id: key(r, 1, &prefix)?,
                role: r.get(2).map_err(integrity)?,
                name: r.get::<_, Option<String>>(3).map_err(integrity)?.unwrap_or_default(),
                symbol_id: key(r, 4, &prefix)?,
                literal: blob(r, 5)?,
                param_index: r.get(6).map_err(integrity)?,
            }))
        })?.into_iter().collect();
    let mut parameters: Vec<_> = values
        .values()
        .filter(|v| v.role == "PARAM")
        .map(|v| {
            v.param_index
                .map(|index| (index, v.id.clone()))
                .ok_or_else(|| integrity("parameter without index"))
        })
        .collect::<Result<_>>()?;
    parameters.sort();
    for (expected, (actual, _)) in parameters.iter().enumerate() {
        if usize::try_from(*actual).ok() != Some(expected) {
            return Err(integrity("noncontiguous parameter indices"));
        }
    }
    function.parameter_value_ids = parameters.into_iter().map(|(_, id)| id).collect();
    let mut blocks = BTreeMap::new();
    for (block_id, id, ordinal) in collect(
        db,
        "SELECT id,local_key,ordinal FROM basic_blocks WHERE function_id=?1 ORDER BY ordinal",
        function_id,
        |r| {
            Ok((
                r.get::<_, i64>(0).map_err(integrity)?,
                key(r, 1, &prefix)?,
                r.get::<_, u32>(2).map_err(integrity)?,
            ))
        },
    )? {
        let instructions = collect(
            db,
            "SELECT i.id,i.local_key,i.ordinal,i.opcode,v.local_key,i.payload,i.is_terminator
             FROM instructions i LEFT JOIN values_ir v ON v.id=i.result_value_id
             WHERE i.block_id=?1 ORDER BY i.ordinal",
            block_id,
            |r| {
                let instruction_id: i64 = r.get(0).map_err(integrity)?;
                let operands = collect(db,
                    "SELECT v.local_key FROM instruction_operands o JOIN values_ir v ON v.id=o.value_id
                     WHERE o.instruction_id=?1 ORDER BY o.operand_index",
                    instruction_id, |r| key(r, 0, &prefix))?;
                Ok(proto::InstructionDef {
                    id: key(r, 1, &prefix)?,
                    block_id: id.clone(),
                    ordinal: r.get(2).map_err(integrity)?,
                    opcode: r.get(3).map_err(integrity)?,
                    result_value_id: key(r, 4, &prefix)?,
                    operand_value_ids: operands,
                    payload: blob(r, 5)?,
                    is_terminator: r.get(6).map_err(integrity)?,
                    spans: Vec::new(),
                })
            },
        )?;
        let edges = collect(
            db,
            "SELECT e.id,e.local_key,b.local_key,i.local_key,e.ordinal,e.label,
             v.local_key,e.guard_negated,e.edge_kind,t.local_key,e.exception_catch_all,o.id
             FROM control_edges e JOIN basic_blocks b ON b.id=e.target_block_id
             JOIN instructions i ON i.id=e.terminator_instruction_id
             LEFT JOIN values_ir v ON v.id=e.guard_value_id
             LEFT JOIN cir_types t ON t.id=e.exception_filter_type_id
             LEFT JOIN ir_outcomes o ON o.control_edge_id=e.id
             WHERE e.source_block_id=?1 ORDER BY e.ordinal",
            block_id,
            |r| {
                let edge_id: i64 = r.get(0).map_err(integrity)?;
                let cases = collect(db,
                    "SELECT v.local_key FROM control_edge_case_values c JOIN values_ir v ON v.id=c.value_id
                     WHERE c.edge_id=?1 ORDER BY c.ordinal",
                    edge_id, |r| key(r, 0, &prefix))?;
                Ok(Edge {
                    definition: proto::ControlEdgeDef {
                        id: key(r, 1, &prefix)?,
                        source_block_id: id.clone(),
                        target_block_id: key(r, 2, &prefix)?,
                        terminator_instruction_id: key(r, 3, &prefix)?,
                        outcome_ordinal: r.get(4).map_err(integrity)?,
                        outcome_label: r.get(5).map_err(integrity)?,
                        guard_value_id: key(r, 6, &prefix)?,
                        guard_negated: r.get(7).map_err(integrity)?,
                        edge_kind: r.get(8).map_err(integrity)?,
                        case_value_ids: cases,
                        exception_filter_type_id: key(r, 9, &prefix)?,
                        exception_catch_all: r.get(10).map_err(integrity)?,
                    },
                    outcome_id: r
                        .get::<_, Option<i64>>(11)
                        .map_err(integrity)?
                        .map(|id| id.to_string()),
                })
            },
        )?;
        blocks.insert(
            id,
            Block {
                ordinal,
                instructions,
                edges,
            },
        );
    }
    Ok(Program {
        function,
        types,
        values,
        blocks,
        symbols,
    })
}
