SELECT f.id FROM functions f WHERE f.is_external=0 AND f.inventory_status<>'FAILED' AND (f.entry_block_id IS NULL OR NOT EXISTS (SELECT 1 FROM basic_blocks b WHERE b.function_id=f.id));
