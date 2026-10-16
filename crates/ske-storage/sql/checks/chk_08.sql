SELECT e.id FROM control_edges e JOIN basic_blocks s ON s.id=e.source_block_id JOIN basic_blocks d ON d.id=e.target_block_id WHERE s.function_id<>d.function_id;
