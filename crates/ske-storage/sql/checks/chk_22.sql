SELECT 'EMPTY_CASE:' || c.id AS violation
FROM source_outcome_activation_cases c
WHERE NOT EXISTS (
    SELECT 1 FROM source_outcome_activation_terms t WHERE t.case_id=c.id
)
UNION ALL
SELECT 'ELIDED_WITH_CASE:' || so.id
FROM source_outcomes so
WHERE so.elided=1 AND EXISTS (
    SELECT 1 FROM source_outcome_activation_cases c WHERE c.source_outcome_id=so.id
)
UNION ALL
SELECT 'COMPLETE_NO_CASE:' || so.id
FROM source_outcomes so
WHERE so.elided=0 AND so.activation_mapping_complete=1
  AND NOT EXISTS (
    SELECT 1 FROM source_outcome_activation_cases c WHERE c.source_outcome_id=so.id
  )
UNION ALL
SELECT 'COMPLETE_NONEXACT:' || so.id
FROM source_outcomes so
WHERE so.elided=0 AND so.activation_mapping_complete=1
  AND EXISTS (
    SELECT 1 FROM source_outcome_activation_cases c
    WHERE c.source_outcome_id=so.id AND c.quality <> 'EXACT'
  )
UNION ALL
SELECT 'CROSS_UNIT:' || t.case_id || ':' || t.term_ordinal
FROM source_outcome_activation_terms t
JOIN source_outcome_activation_cases c ON c.id=t.case_id
JOIN source_outcomes so ON so.id=c.source_outcome_id
JOIN source_decisions sd ON sd.id=so.decision_id
JOIN ir_outcomes io ON io.id=t.ir_outcome_id
JOIN ir_decisions idec ON idec.id=io.decision_id
JOIN instructions ins ON ins.id=idec.terminator_instruction_id
JOIN basic_blocks bb ON bb.id=ins.block_id
JOIN functions f ON f.id=bb.function_id
JOIN symbols sym ON sym.id=f.symbol_id
WHERE sym.unit_id <> sd.unit_id OR sym.snapshot_id <> sd.snapshot_id;
