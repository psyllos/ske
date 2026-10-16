SELECT e.id FROM control_edges e JOIN instructions t ON t.id=e.terminator_instruction_id WHERE t.block_id<>e.source_block_id OR t.is_terminator=0;
