SELECT f.id FROM functions f WHERE f.entry_block_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM basic_blocks b WHERE b.id=f.entry_block_id AND b.function_id=f.id);
