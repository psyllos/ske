CREATE TABLE functional_capabilities (
  id INTEGER PRIMARY KEY,
  snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
  canonical_key TEXT NOT NULL,
  title TEXT,
  origin TEXT NOT NULL CHECK (origin IN ('DETERMINISTIC','ABSTRACT_DERIVED','LLM_INFERRED')),
  validation_status TEXT NOT NULL CHECK (validation_status IN ('UNVALIDATED','HUMAN_VALIDATED','REJECTED','STALE')),
  UNIQUE(snapshot_id, canonical_key)
);
CREATE TABLE capability_scenarios (
  capability_id INTEGER NOT NULL REFERENCES functional_capabilities(id),
  scenario_id INTEGER NOT NULL REFERENCES behavior_scenarios(id),
  PRIMARY KEY(capability_id, scenario_id)
);
CREATE TABLE scenario_links (
  id INTEGER PRIMARY KEY,
  from_scenario_id INTEGER NOT NULL REFERENCES behavior_scenarios(id),
  to_scenario_id INTEGER REFERENCES behavior_scenarios(id),
  relation_kind TEXT NOT NULL CHECK (relation_kind IN ('CALL','EVENT','HTTP','MESSAGE','PERSISTENCE','EXTERNAL_UNKNOWN')),
  confidence_kind TEXT NOT NULL CHECK (confidence_kind IN ('CONFIRMED','INFERRED','UNKNOWN')),
  evidence_json TEXT NOT NULL DEFAULT '[]',
  assumptions_json TEXT NOT NULL DEFAULT '[]',
  CHECK (to_scenario_id IS NOT NULL OR relation_kind = 'EXTERNAL_UNKNOWN')
);
CREATE INDEX idx_capability_scenarios_scenario ON capability_scenarios(scenario_id);
CREATE INDEX idx_scenario_links_from ON scenario_links(from_scenario_id);
