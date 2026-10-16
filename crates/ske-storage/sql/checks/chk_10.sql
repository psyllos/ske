SELECT o.id FROM ir_outcomes o JOIN ir_decisions d ON d.id=o.decision_id JOIN control_edges e ON e.id=o.control_edge_id WHERE e.terminator_instruction_id<>d.terminator_instruction_id;
