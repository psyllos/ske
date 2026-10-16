-- Pas de BEGIN/COMMIT/PRAGMA ici. schema_migrations est créée par le runner.

CREATE TABLE repositories (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    root_uri TEXT NOT NULL UNIQUE
);

CREATE TABLE snapshots (
    id INTEGER PRIMARY KEY,
    repository_id INTEGER NOT NULL REFERENCES repositories(id),
    revision TEXT,
    source_fingerprint TEXT,
    config_fingerprint TEXT,
    status TEXT NOT NULL DEFAULT 'OPEN' CHECK (status IN ('OPEN','CLOSED','ABORTED')),
    inventory_status TEXT NOT NULL DEFAULT 'PARTIAL'
        CHECK (inventory_status IN ('COMPLETE','PARTIAL','FAILED')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    closed_at TEXT,
    CHECK (status <> 'CLOSED' OR (source_fingerprint IS NOT NULL
                                  AND config_fingerprint IS NOT NULL
                                  AND closed_at IS NOT NULL))
);

CREATE TABLE compilation_units (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    unit_key TEXT NOT NULL,
    language TEXT NOT NULL,
    adapter_name TEXT NOT NULL,
    adapter_version TEXT NOT NULL,
    toolchain_version TEXT NOT NULL,
    build_fingerprint TEXT NOT NULL,
    semantic_profile TEXT NOT NULL,
    semantic_profile_hash TEXT NOT NULL,
    capability_flags TEXT NOT NULL DEFAULT '[]',          -- tableau JSON
    inventory_status TEXT NOT NULL
        CHECK (inventory_status IN ('COMPLETE','PARTIAL','FAILED')),
    expected_source_decisions INTEGER NOT NULL DEFAULT 0 CHECK (expected_source_decisions >= 0),
    source_decision_count INTEGER NOT NULL DEFAULT 0 CHECK (source_decision_count >= 0),
    ir_decision_count INTEGER NOT NULL DEFAULT 0 CHECK (ir_decision_count >= 0),
    ir_edge_count INTEGER NOT NULL DEFAULT 0 CHECK (ir_edge_count >= 0),
    UNIQUE (snapshot_id, unit_key)
);

CREATE TABLE source_files (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    path TEXT NOT NULL,
    language TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    UNIQUE (snapshot_id, path)
);

CREATE TABLE source_spans (
    id INTEGER PRIMARY KEY,
    source_file_id INTEGER NOT NULL REFERENCES source_files(id),
    start_line INTEGER NOT NULL CHECK (start_line > 0),
    start_column INTEGER NOT NULL CHECK (start_column > 0),
    end_line INTEGER NOT NULL CHECK (end_line > 0),
    end_column INTEGER NOT NULL CHECK (end_column > 0),     -- EXCLUSIF
    CHECK (end_line > start_line OR (end_line = start_line AND end_column >= start_column)),
    UNIQUE (source_file_id, start_line, start_column, end_line, end_column)
);

CREATE TABLE cir_types (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    unit_id INTEGER NOT NULL REFERENCES compilation_units(id),
    local_key TEXT NOT NULL,                                 -- unit_key|id
    kind TEXT NOT NULL,
    descriptor TEXT NOT NULL,
    payload BLOB NOT NULL,                                   -- TypeDef protobuf sérialisé (autoritatif)
    UNIQUE (snapshot_id, local_key)
);

CREATE TABLE symbols (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    unit_id INTEGER NOT NULL REFERENCES compilation_units(id),
    local_key TEXT NOT NULL,
    owner_symbol_id INTEGER REFERENCES symbols(id),
    name TEXT NOT NULL,
    qualified_name TEXT NOT NULL,
    kind TEXT NOT NULL,
    type_id INTEGER REFERENCES cir_types(id),
    primary_span_id INTEGER REFERENCES source_spans(id),
    structural_fingerprint TEXT,
    UNIQUE (snapshot_id, local_key)
);

CREATE TABLE functions (
    id INTEGER PRIMARY KEY,
    symbol_id INTEGER NOT NULL UNIQUE REFERENCES symbols(id),
    entry_block_id INTEGER,                                  -- validé applicativement (E_ENTRY_BLOCK)
    return_type_ids TEXT NOT NULL DEFAULT '[]',               -- tableau ordonné d'identifiants CIR de types (vérifié à import)
    is_external INTEGER NOT NULL DEFAULT 0 CHECK (is_external IN (0,1)),
    resolved_function_id INTEGER REFERENCES functions(id),   -- stub externe lié à une fonction réelle
    inventory_status TEXT NOT NULL DEFAULT 'PARTIAL'
        CHECK (inventory_status IN ('COMPLETE','PARTIAL','FAILED')),
    CHECK (resolved_function_id IS NULL OR is_external = 1)
);

CREATE TABLE values_ir (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    unit_id INTEGER NOT NULL REFERENCES compilation_units(id),
    function_id INTEGER REFERENCES functions(id),            -- NULL pour une GLOBAL
    local_key TEXT NOT NULL,
    type_id INTEGER NOT NULL REFERENCES cir_types(id),
    value_role TEXT NOT NULL CHECK (value_role IN ('PARAM','CONST','SSA','GLOBAL','MEMORY')),
    name TEXT,
    symbol_id INTEGER REFERENCES symbols(id),                -- GLOBAL
    literal BLOB,                                            -- LiteralValue protobuf (CONST)
    param_index INTEGER CHECK (param_index IS NULL OR param_index >= 0),
    UNIQUE (snapshot_id, local_key),
    UNIQUE (function_id, param_index),
    CHECK ((value_role = 'PARAM') = (param_index IS NOT NULL)),
    CHECK ((value_role = 'CONST') = (literal IS NOT NULL)),
    CHECK ((value_role = 'GLOBAL') = (symbol_id IS NOT NULL))
);

CREATE TABLE basic_blocks (
    id INTEGER PRIMARY KEY,
    function_id INTEGER NOT NULL REFERENCES functions(id),
    local_key TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    UNIQUE (function_id, local_key),
    UNIQUE (function_id, ordinal)
);

CREATE TABLE instructions (
    id INTEGER PRIMARY KEY,
    block_id INTEGER NOT NULL REFERENCES basic_blocks(id),
    local_key TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    opcode TEXT NOT NULL,
    result_value_id INTEGER REFERENCES values_ir(id),
    is_terminator INTEGER NOT NULL DEFAULT 0 CHECK (is_terminator IN (0,1)),
    payload BLOB,                                            -- OperationPayload protobuf
    primary_span_id INTEGER REFERENCES source_spans(id),
    UNIQUE (block_id, local_key),
    UNIQUE (block_id, ordinal)
);
CREATE UNIQUE INDEX idx_one_terminator_per_block ON instructions(block_id) WHERE is_terminator = 1;

CREATE TABLE instruction_operands (
    instruction_id INTEGER NOT NULL REFERENCES instructions(id),
    operand_index INTEGER NOT NULL CHECK (operand_index >= 0),
    value_id INTEGER NOT NULL REFERENCES values_ir(id),
    PRIMARY KEY (instruction_id, operand_index)
);

CREATE TABLE phi_inputs (
    instruction_id INTEGER NOT NULL REFERENCES instructions(id),
    input_index INTEGER NOT NULL CHECK (input_index >= 0),
    predecessor_block_id INTEGER NOT NULL REFERENCES basic_blocks(id),
    value_id INTEGER NOT NULL REFERENCES values_ir(id),
    PRIMARY KEY (instruction_id, input_index),
    UNIQUE (instruction_id, predecessor_block_id)
);

CREATE TABLE control_edges (
    id INTEGER PRIMARY KEY,
    local_key TEXT NOT NULL,
    source_block_id INTEGER NOT NULL REFERENCES basic_blocks(id),
    target_block_id INTEGER NOT NULL REFERENCES basic_blocks(id),
    terminator_instruction_id INTEGER NOT NULL REFERENCES instructions(id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    label TEXT NOT NULL,
    edge_kind TEXT NOT NULL CHECK (edge_kind IN
        ('UNCONDITIONAL','BRANCH_TRUE','BRANCH_FALSE','SWITCH_CASE','SWITCH_DEFAULT','NORMAL','EXCEPTION')),
    guard_value_id INTEGER REFERENCES values_ir(id),
    guard_negated INTEGER NOT NULL DEFAULT 0 CHECK (guard_negated IN (0,1)),
    exception_filter_type_id INTEGER REFERENCES cir_types(id),
    exception_catch_all INTEGER NOT NULL DEFAULT 0 CHECK (exception_catch_all IN (0,1)),
    UNIQUE (terminator_instruction_id, ordinal)
);

CREATE TABLE control_edge_case_values (
    edge_id INTEGER NOT NULL REFERENCES control_edges(id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    value_id INTEGER NOT NULL REFERENCES values_ir(id),
    PRIMARY KEY (edge_id, ordinal)
);

-- Sites de sortie anormale implicite, inventoriés même sans arête CFG explicite.
CREATE TABLE implicit_exit_sites (
  id INTEGER PRIMARY KEY,
  snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
  unit_id INTEGER NOT NULL REFERENCES compilation_units(id),
  function_id INTEGER NOT NULL REFERENCES functions(id),
  source_span_id INTEGER REFERENCES source_spans(id),
  local_key TEXT NOT NULL,
  exit_kind TEXT NOT NULL,
  modeling_status TEXT NOT NULL CHECK(modeling_status IN ('MODELED','UNMODELED','UNKNOWN')),
  UNIQUE(snapshot_id, local_key)
);

CREATE TABLE source_decisions (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    unit_id INTEGER NOT NULL REFERENCES compilation_units(id),
    function_id INTEGER REFERENCES functions(id),            -- NULL hors fonction
    parent_decision_id INTEGER REFERENCES source_decisions(id),
    local_key TEXT NOT NULL,
    kind TEXT NOT NULL,
    source_span_id INTEGER NOT NULL REFERENCES source_spans(id),
    UNIQUE (snapshot_id, local_key)
);

CREATE TABLE source_outcomes (
    id INTEGER PRIMARY KEY,
    decision_id INTEGER NOT NULL REFERENCES source_decisions(id),
    local_key TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    label TEXT NOT NULL,
    elided INTEGER NOT NULL DEFAULT 0 CHECK (elided IN (0,1)),
    activation_mapping_complete INTEGER NOT NULL DEFAULT 0 CHECK (activation_mapping_complete IN (0,1)),
    UNIQUE (decision_id, ordinal)
);

CREATE TABLE ir_decisions (
    id INTEGER PRIMARY KEY,
    terminator_instruction_id INTEGER NOT NULL UNIQUE REFERENCES instructions(id),
    kind TEXT NOT NULL CHECK (kind IN ('BRANCH','SWITCH','INVOKE'))
);

CREATE TABLE ir_outcomes (
    id INTEGER PRIMARY KEY,
    decision_id INTEGER NOT NULL REFERENCES ir_decisions(id),
    control_edge_id INTEGER NOT NULL UNIQUE REFERENCES control_edges(id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    label TEXT NOT NULL,
    UNIQUE (decision_id, ordinal)
);

CREATE TABLE source_ir_links (
    source_outcome_id INTEGER NOT NULL REFERENCES source_outcomes(id),
    ir_outcome_id INTEGER NOT NULL REFERENCES ir_outcomes(id),
    mapping_kind TEXT NOT NULL CHECK (mapping_kind IN
        ('DIRECT','SHORT_CIRCUIT','SWITCH_CASE','LOOP_CONDITION','IMPLICIT')),
    quality TEXT NOT NULL CHECK (quality IN ('EXACT','APPROX','HEURISTIC')),
    inverted INTEGER NOT NULL DEFAULT 0 CHECK (inverted IN (0,1)),
    PRIMARY KEY (source_outcome_id, ir_outcome_id)
);

-- v1.2 : activation source ; les liens source_ir_links ne sont pas des formules de preuve.
CREATE TABLE source_outcome_activation_cases (
    id INTEGER PRIMARY KEY,
    source_outcome_id INTEGER NOT NULL REFERENCES source_outcomes(id),
    case_ordinal INTEGER NOT NULL CHECK (case_ordinal >= 0),
    quality TEXT NOT NULL CHECK (quality IN ('EXACT','APPROX','HEURISTIC')),
    UNIQUE (source_outcome_id, case_ordinal)
);

CREATE TABLE source_outcome_activation_terms (
    case_id INTEGER NOT NULL REFERENCES source_outcome_activation_cases(id),
    term_ordinal INTEGER NOT NULL CHECK (term_ordinal >= 0),
    ir_outcome_id INTEGER NOT NULL REFERENCES ir_outcomes(id),
    PRIMARY KEY (case_id, term_ordinal)
);
CREATE INDEX idx_source_activation_term_outcome
  ON source_outcome_activation_terms(ir_outcome_id);

CREATE TABLE call_sites (
    id INTEGER PRIMARY KEY,
    instruction_id INTEGER NOT NULL UNIQUE REFERENCES instructions(id),
    dispatch_kind TEXT NOT NULL CHECK (dispatch_kind IN
        ('STATIC','VIRTUAL','INTERFACE','TRAIT','REFLECTIVE','DYNAMIC','UNKNOWN')),
    resolution_status TEXT NOT NULL CHECK (resolution_status IN
        ('RESOLVED_UNIQUE','RESOLVED_POSSIBLE','UNRESOLVED','EXTERNAL')),
    world_closed INTEGER NOT NULL DEFAULT 0 CHECK (world_closed IN (0,1)),
    may_raise INTEGER NOT NULL DEFAULT 1 CHECK (may_raise IN (0,1)),
    may_mutate_memory INTEGER NOT NULL DEFAULT 1 CHECK (may_mutate_memory IN (0,1)),
    is_pure_proven INTEGER NOT NULL DEFAULT 0 CHECK (is_pure_proven IN (0,1)),
    is_referentially_transparent_proven INTEGER NOT NULL DEFAULT 0 CHECK (is_referentially_transparent_proven IN (0,1)),
    CHECK (is_referentially_transparent_proven = 0 OR is_pure_proven = 1)
);

CREATE TABLE call_targets (
    call_site_id INTEGER NOT NULL REFERENCES call_sites(id),
    target_function_id INTEGER NOT NULL REFERENCES functions(id),
    resolution_method TEXT NOT NULL,
    PRIMARY KEY (call_site_id, target_function_id)
);

CREATE TABLE diagnostics (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    unit_id INTEGER REFERENCES compilation_units(id),
    function_id INTEGER REFERENCES functions(id),
    source_span_id INTEGER REFERENCES source_spans(id),
    analysis_run_id INTEGER REFERENCES analysis_runs(id),    -- diagnostics du moteur ; NULL pour ceux de l'import
    severity TEXT NOT NULL CHECK (severity IN ('INFO','WARNING','ERROR')),
    code TEXT NOT NULL,
    impact TEXT NOT NULL CHECK (impact IN
        ('INVENTORY','SYMBOLIC','EFFECTS','CALLGRAPH','TYPES','CONCURRENCY','OTHER')),
    message TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- DAG symbolique canonique par snapshot
CREATE TABLE symbolic_expressions (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    expression_hash TEXT NOT NULL,
    kind TEXT NOT NULL,
    type_id INTEGER REFERENCES cir_types(id),
    payload BLOB,
    UNIQUE (snapshot_id, expression_hash)
);

CREATE TABLE expression_operands (
    expression_id INTEGER NOT NULL REFERENCES symbolic_expressions(id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    operand_expression_id INTEGER NOT NULL REFERENCES symbolic_expressions(id),
    PRIMARY KEY (expression_id, ordinal)
);

CREATE TABLE analysis_runs (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    entry_function_id INTEGER REFERENCES functions(id),
    analysis_kind TEXT NOT NULL,                             -- 'BRANCH_ANALYSIS','SUMMARY','BEHAVIOR'
    engine_version TEXT NOT NULL,
    solver_name TEXT,
    solver_version TEXT,
    context_name TEXT NOT NULL DEFAULT 'default',
    context_json TEXT NOT NULL DEFAULT '{}',
    budget_json TEXT NOT NULL DEFAULT '{}',
    config_fingerprint TEXT NOT NULL,
    assumptions_fingerprint TEXT NOT NULL,
    status TEXT NOT NULL CHECK
        (status IN ('PENDING','RUNNING','COMPLETED','PARTIAL','FAILED','CANCELLED')),
    exploration_closed INTEGER NOT NULL DEFAULT 0 CHECK (exploration_closed IN (0,1)),
    cut_count INTEGER NOT NULL DEFAULT 0 CHECK (cut_count >= 0),
    metrics_json TEXT NOT NULL DEFAULT '{}',
    error_message TEXT,
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    finished_at TEXT
);

CREATE TABLE analysis_assumptions (
    id INTEGER PRIMARY KEY,
    analysis_run_id INTEGER NOT NULL REFERENCES analysis_runs(id),
    kind TEXT NOT NULL,
    description TEXT NOT NULL,
    payload TEXT
);

CREATE TABLE solver_queries (
    id INTEGER PRIMARY KEY,
    analysis_run_id INTEGER NOT NULL REFERENCES analysis_runs(id),
    query_fingerprint TEXT NOT NULL,
    smtlib TEXT,
    result TEXT NOT NULL CHECK (result IN ('SAT','UNSAT','UNKNOWN','TIMEOUT','ERROR')),
    timeout_kind TEXT CHECK (timeout_kind IS NULL OR timeout_kind IN ('RLIMIT','WALL_CLOCK','BUDGET')),
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    reason TEXT,
    solver_seed INTEGER NOT NULL DEFAULT 0,
    solver_rlimit INTEGER NOT NULL DEFAULT 0,
    UNIQUE (analysis_run_id, query_fingerprint),
    CHECK (result <> 'UNSAT' OR smtlib IS NOT NULL)
);

CREATE TABLE outcome_arrivals (
    id INTEGER PRIMARY KEY,
    analysis_run_id INTEGER NOT NULL REFERENCES analysis_runs(id),
    ir_outcome_id INTEGER NOT NULL REFERENCES ir_outcomes(id),
    arrival_seq INTEGER NOT NULL CHECK (arrival_seq >= 0),
    query_id INTEGER REFERENCES solver_queries(id),           -- NULL si aucune requête (budget)
    result TEXT NOT NULL CHECK (result IN ('SAT','UNSAT','UNKNOWN','TIMEOUT','UNSUPPORTED')),
    soundness TEXT NOT NULL CHECK (soundness IN ('EXACT','OVERAPPROX','OPAQUE','UNSUPPORTED')),
    path_json TEXT NOT NULL DEFAULT '[]',                     -- ids d'ir_outcomes empruntées
    UNIQUE (analysis_run_id, ir_outcome_id, arrival_seq)
);

CREATE TABLE proof_records (
    id INTEGER PRIMARY KEY,
    analysis_run_id INTEGER NOT NULL REFERENCES analysis_runs(id),
    ir_outcome_id INTEGER NOT NULL REFERENCES ir_outcomes(id),
    verifier_name TEXT NOT NULL,
    verifier_version TEXT NOT NULL,
    verdict TEXT NOT NULL CHECK (verdict IN ('ACCEPTED','REJECTED')),
    verification_level TEXT NOT NULL DEFAULT 'SOLVER_REPLAYED' CHECK
      (verification_level IN ('SOLVER_REPLAYED','MODEL_UNREACHABLE','CERTIFIED_UNREACHABLE')),
    model_soundness TEXT NOT NULL CHECK (model_soundness IN ('EXACT','OVERAPPROX','OPAQUE','UNSUPPORTED')),
    eliminated_arrivals INTEGER NOT NULL CHECK (eliminated_arrivals >= 0),
    residual_frontier INTEGER NOT NULL CHECK (residual_frontier >= 0),
    reason TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (verdict <> 'ACCEPTED' OR verification_level = 'MODEL_UNREACHABLE') -- v0.x
);

CREATE TABLE branch_analysis_results (
    id INTEGER PRIMARY KEY,
    analysis_run_id INTEGER NOT NULL REFERENCES analysis_runs(id),
    ir_outcome_id INTEGER NOT NULL REFERENCES ir_outcomes(id),
    status TEXT NOT NULL CHECK (status IN
      ('UNANALYZED','POSSIBLE','WITNESSED','PROVEN_UNREACHABLE','UNKNOWN','TIMEOUT','UNSUPPORTED')),
    soundness TEXT NOT NULL CHECK (soundness IN ('EXACT','OVERAPPROX','OPAQUE','UNSUPPORTED')),
    explanation TEXT,
    proof_id INTEGER REFERENCES proof_records(id),
    UNIQUE (analysis_run_id, ir_outcome_id)
);

CREATE TABLE branch_cases (
    id INTEGER PRIMARY KEY,
    result_id INTEGER NOT NULL REFERENCES branch_analysis_results(id),
    guard_expression_id INTEGER REFERENCES symbolic_expressions(id),
    witness_json TEXT,
    validation_method TEXT,                                  -- 'SMT_MODEL' en v0.3
    note TEXT
);

CREATE TABLE function_summaries (
    id INTEGER PRIMARY KEY,
    function_id INTEGER NOT NULL REFERENCES functions(id),
    produced_by_run_id INTEGER NOT NULL REFERENCES analysis_runs(id),
    context_fingerprint TEXT NOT NULL,
    summary_fingerprint TEXT NOT NULL,
    precision TEXT NOT NULL CHECK (precision IN ('EXACT','OVERAPPROX','PARTIAL','UNKNOWN')),
    complete INTEGER NOT NULL CHECK (complete IN (0,1)),
    unknowns_json TEXT NOT NULL DEFAULT '[]',
    reused_from_summary_id INTEGER REFERENCES function_summaries(id),
    UNIQUE (function_id, context_fingerprint, summary_fingerprint)
);

CREATE TABLE summary_cases (
    id INTEGER PRIMARY KEY,
    summary_id INTEGER NOT NULL REFERENCES function_summaries(id),
    case_ordinal INTEGER NOT NULL CHECK (case_ordinal >= 0),
    guard_json TEXT NOT NULL,                                -- tableau d'ids symbolic_expressions (conjonction ordonnée)
    exit_kind TEXT NOT NULL CHECK (exit_kind IN ('RETURN','PANIC','THROW','UNREACHABLE_EXIT')),
    memory_delta_json TEXT NOT NULL DEFAULT '[]',
    precision TEXT NOT NULL CHECK (precision IN ('EXACT','OVERAPPROX','PARTIAL','UNKNOWN')),
    exit_span_id INTEGER REFERENCES source_spans(id),
    UNIQUE (summary_id, case_ordinal)
);

CREATE TABLE summary_case_returns (
    summary_case_id INTEGER NOT NULL REFERENCES summary_cases(id),
    return_ordinal INTEGER NOT NULL CHECK (return_ordinal >= 0),
    expression_id INTEGER NOT NULL REFERENCES symbolic_expressions(id),
    PRIMARY KEY (summary_case_id, return_ordinal)
);

CREATE TABLE summary_case_outcomes (
    summary_case_id INTEGER NOT NULL REFERENCES summary_cases(id),
    path_ordinal INTEGER NOT NULL CHECK (path_ordinal >= 0),
    ir_outcome_id INTEGER NOT NULL REFERENCES ir_outcomes(id),
    PRIMARY KEY (summary_case_id, path_ordinal)
);

CREATE TABLE observable_effects (
    id INTEGER PRIMARY KEY,
    summary_case_id INTEGER NOT NULL REFERENCES summary_cases(id),
    effect_ordinal INTEGER NOT NULL CHECK (effect_ordinal >= 0),
    effect_kind TEXT NOT NULL,
    target_descriptor TEXT,
    payload TEXT,
    semantic_model_fingerprint TEXT,
    precision TEXT NOT NULL,
    UNIQUE (summary_case_id, effect_ordinal)
);

CREATE TABLE behavior_scenarios (
    id INTEGER PRIMARY KEY,
    analysis_run_id INTEGER NOT NULL REFERENCES analysis_runs(id),
    entry_function_id INTEGER NOT NULL REFERENCES functions(id),
    local_key TEXT NOT NULL,
    name TEXT,
    status TEXT NOT NULL,
    precision TEXT NOT NULL,
    UNIQUE (analysis_run_id, local_key)
);

CREATE TABLE behavior_cases (
    id INTEGER PRIMARY KEY,
    behavior_id INTEGER NOT NULL REFERENCES behavior_scenarios(id),
    summary_case_id INTEGER REFERENCES summary_cases(id),
    case_ordinal INTEGER NOT NULL CHECK (case_ordinal >= 0),
    guard_text TEXT NOT NULL,                                -- rendu R1-R6
    result_text TEXT NOT NULL,
    effects_text TEXT,
    precision TEXT NOT NULL,
    UNIQUE (behavior_id, case_ordinal)
);

CREATE TABLE behavior_case_outcomes (
    behavior_case_id INTEGER NOT NULL REFERENCES behavior_cases(id),
    ir_outcome_id INTEGER NOT NULL REFERENCES ir_outcomes(id),
    PRIMARY KEY (behavior_case_id, ir_outcome_id)
);

CREATE TABLE functional_rules (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    behavior_case_id INTEGER REFERENCES behavior_cases(id),
    rule_key TEXT NOT NULL,
    short_name TEXT,
    description TEXT NOT NULL,
    origin TEXT NOT NULL CHECK (origin IN ('DETERMINISTIC','ABSTRACT_DERIVED','LLM_INFERRED')),
    validation_status TEXT NOT NULL DEFAULT 'UNVALIDATED' CHECK (validation_status IN
        ('UNVALIDATED','HUMAN_VALIDATED','REJECTED','STALE')),
    prompt_version TEXT,
    llm_model TEXT,
    model_parameters TEXT,
    UNIQUE (snapshot_id, rule_key),
    UNIQUE (behavior_case_id)
);

CREATE TABLE knowledge_evidence (
    id INTEGER PRIMARY KEY,
    functional_rule_id INTEGER NOT NULL REFERENCES functional_rules(id),
    source_span_id INTEGER REFERENCES source_spans(id),
    instruction_id INTEGER REFERENCES instructions(id),
    ir_outcome_id INTEGER REFERENCES ir_outcomes(id),
    summary_case_id INTEGER REFERENCES summary_cases(id),
    relation TEXT NOT NULL,
    CHECK ((source_span_id IS NOT NULL) + (instruction_id IS NOT NULL)
         + (ir_outcome_id IS NOT NULL) + (summary_case_id IS NOT NULL) = 1)
);

CREATE TABLE analysis_dependencies (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    dependent_kind TEXT NOT NULL,
    dependent_key TEXT NOT NULL,
    dependency_kind TEXT NOT NULL,
    dependency_key TEXT NOT NULL,
    UNIQUE (snapshot_id, dependent_kind, dependent_key, dependency_kind, dependency_key)
);

-- Index
CREATE INDEX idx_units_snapshot ON compilation_units(snapshot_id);
CREATE INDEX idx_files_snapshot ON source_files(snapshot_id);
CREATE INDEX idx_spans_file ON source_spans(source_file_id);
CREATE INDEX idx_symbols_name ON symbols(snapshot_id, qualified_name);
CREATE INDEX idx_symbols_unit ON symbols(unit_id);
CREATE INDEX idx_values_function ON values_ir(function_id);
CREATE INDEX idx_blocks_function ON basic_blocks(function_id);
CREATE INDEX idx_instr_block ON instructions(block_id, ordinal);
CREATE INDEX idx_operands_value ON instruction_operands(value_id);
CREATE INDEX idx_phi_pred ON phi_inputs(predecessor_block_id);
CREATE INDEX idx_edges_source ON control_edges(source_block_id);
CREATE INDEX idx_edges_target ON control_edges(target_block_id);
CREATE INDEX idx_source_decisions_snapshot ON source_decisions(snapshot_id);
CREATE INDEX idx_source_decisions_function ON source_decisions(function_id);
CREATE INDEX idx_source_outcomes_decision ON source_outcomes(decision_id);
CREATE INDEX idx_ir_outcomes_decision ON ir_outcomes(decision_id);
CREATE INDEX idx_source_ir_ir ON source_ir_links(ir_outcome_id);
CREATE INDEX idx_call_targets_function ON call_targets(target_function_id);
CREATE INDEX idx_diagnostics_snapshot ON diagnostics(snapshot_id, impact);
CREATE INDEX idx_diagnostics_run ON diagnostics(analysis_run_id);
CREATE INDEX idx_expression_operands_child ON expression_operands(operand_expression_id);
CREATE INDEX idx_analysis_run_snapshot ON analysis_runs(snapshot_id, analysis_kind);
CREATE INDEX idx_arrivals_outcome ON outcome_arrivals(analysis_run_id, ir_outcome_id);
CREATE INDEX idx_branch_results_run_status ON branch_analysis_results(analysis_run_id, status);
CREATE INDEX idx_summaries_function ON function_summaries(function_id);
CREATE INDEX idx_behaviors_entry ON behavior_scenarios(entry_function_id);
CREATE INDEX idx_rules_snapshot ON functional_rules(snapshot_id, validation_status);
CREATE INDEX idx_evidence_rule ON knowledge_evidence(functional_rule_id);
CREATE INDEX idx_dependencies_dependency
    ON analysis_dependencies(snapshot_id, dependency_kind, dependency_key);

-- La preuve ne peut être ACCEPTED avant fermeture globale du run.
CREATE TRIGGER trg_proof_accept_only_closed
BEFORE INSERT ON proof_records
WHEN NEW.verdict='ACCEPTED'
  AND NOT EXISTS (
      SELECT 1 FROM analysis_runs r
      WHERE r.id=NEW.analysis_run_id AND r.status='COMPLETED'
        AND r.exploration_closed=1 AND r.cut_count=0)
BEGIN SELECT RAISE(ABORT, 'E_PROOF_REQUIRED'); END;

-- V1.2 : preuve et faits à sa base sont APPEND-ONLY après acceptation.
CREATE TRIGGER trg_proof_immutable_update
BEFORE UPDATE ON proof_records
BEGIN SELECT RAISE(ABORT, 'E_PROOF_IMMUTABLE'); END;
CREATE TRIGGER trg_proof_immutable_delete
BEFORE DELETE ON proof_records
BEGIN SELECT RAISE(ABORT, 'E_PROOF_IMMUTABLE'); END;

-- Interdire la mutation des attributs de run déterminants (finished_at/metrics_json restent modifiables ;
-- status : voir trg_proven_run_status_final).
CREATE TRIGGER trg_proven_run_frozen_update
BEFORE UPDATE OF snapshot_id, entry_function_id, engine_version, solver_name, solver_version,
                 context_name, context_json, budget_json, config_fingerprint,
                 assumptions_fingerprint, exploration_closed, cut_count
ON analysis_runs
WHEN EXISTS (SELECT 1 FROM proof_records p WHERE p.analysis_run_id=OLD.id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proven_run_frozen_delete
BEFORE DELETE ON analysis_runs
WHEN EXISTS (SELECT 1 FROM proof_records p WHERE p.analysis_run_id=OLD.id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;

-- v1.4 : le statut d'un run portant une preuve acceptée est définitif (COMPLETED).
CREATE TRIGGER trg_proven_run_status_final
BEFORE UPDATE OF status ON analysis_runs
WHEN NEW.status <> 'COMPLETED'
  AND EXISTS (SELECT 1 FROM proof_records p WHERE p.analysis_run_id=OLD.id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;

-- Les arrivées, requêtes et hypothèses d'un run certifié au niveau modèle sont figées.
-- NEW pour les INSERT ; OLD pour UPDATE/DELETE.
CREATE TRIGGER trg_proof_support_analysis_assumptions_insert
BEFORE INSERT ON analysis_assumptions
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=NEW.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_analysis_assumptions_update
BEFORE UPDATE ON analysis_assumptions
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=OLD.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_analysis_assumptions_delete
BEFORE DELETE ON analysis_assumptions
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=OLD.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_solver_queries_insert
BEFORE INSERT ON solver_queries
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=NEW.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_solver_queries_update
BEFORE UPDATE ON solver_queries
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=OLD.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_solver_queries_delete
BEFORE DELETE ON solver_queries
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=OLD.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_outcome_arrivals_insert
BEFORE INSERT ON outcome_arrivals
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=NEW.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_outcome_arrivals_update
BEFORE UPDATE ON outcome_arrivals
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=OLD.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_outcome_arrivals_delete
BEFORE DELETE ON outcome_arrivals
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=OLD.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;

-- Triggers : règles de soundness portées par la base (§8.3)
CREATE TRIGGER trg_proven_requires_proof_ins
BEFORE INSERT ON branch_analysis_results
WHEN NEW.status = 'PROVEN_UNREACHABLE'
BEGIN
    SELECT RAISE(ABORT, 'E_PROOF_REQUIRED')
    WHERE NOT EXISTS (
        SELECT 1 FROM proof_records p
        WHERE p.id = NEW.proof_id
          AND p.verdict = 'ACCEPTED'
          AND p.verification_level = 'MODEL_UNREACHABLE'
          AND p.residual_frontier = 0
          AND p.model_soundness IN ('EXACT','OVERAPPROX','OPAQUE')
          AND p.ir_outcome_id = NEW.ir_outcome_id
          AND p.analysis_run_id = NEW.analysis_run_id
          AND EXISTS (
              SELECT 1 FROM analysis_runs r
              WHERE r.id = NEW.analysis_run_id
                AND r.status = 'COMPLETED' AND r.exploration_closed = 1 AND r.cut_count = 0));
END;

CREATE TRIGGER trg_proven_requires_proof_upd
BEFORE UPDATE OF status, proof_id, ir_outcome_id, analysis_run_id ON branch_analysis_results
WHEN NEW.status = 'PROVEN_UNREACHABLE'
BEGIN
    SELECT RAISE(ABORT, 'E_PROOF_REQUIRED')
    WHERE NOT EXISTS (
        SELECT 1 FROM proof_records p
        WHERE p.id = NEW.proof_id
          AND p.verdict = 'ACCEPTED'
          AND p.verification_level = 'MODEL_UNREACHABLE'
          AND p.residual_frontier = 0
          AND p.model_soundness IN ('EXACT','OVERAPPROX','OPAQUE')
          AND p.ir_outcome_id = NEW.ir_outcome_id
          AND p.analysis_run_id = NEW.analysis_run_id
          AND EXISTS (
              SELECT 1 FROM analysis_runs r
              WHERE r.id = NEW.analysis_run_id
                AND r.status = 'COMPLETED' AND r.exploration_closed = 1 AND r.cut_count = 0));
END;

-- Un résultat démontré ne peut être supprimé/rétrogradé dans le même run ;
-- une correction doit créer un nouveau run avec provenance.
CREATE TRIGGER trg_proven_result_immutable_update
BEFORE UPDATE ON branch_analysis_results
WHEN OLD.status = 'PROVEN_UNREACHABLE'
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proven_result_immutable_delete
BEFORE DELETE ON branch_analysis_results
WHEN OLD.status = 'PROVEN_UNREACHABLE'
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;

CREATE TRIGGER trg_no_witnessed_ins
BEFORE INSERT ON branch_analysis_results
WHEN NEW.status = 'WITNESSED'
BEGIN
    SELECT RAISE(ABORT, 'E_WITNESSED_NOT_PRODUCIBLE');
END;

CREATE TRIGGER trg_no_witnessed_upd
BEFORE UPDATE OF status ON branch_analysis_results
WHEN NEW.status = 'WITNESSED'
BEGIN
    SELECT RAISE(ABORT, 'E_WITNESSED_NOT_PRODUCIBLE');
END;
