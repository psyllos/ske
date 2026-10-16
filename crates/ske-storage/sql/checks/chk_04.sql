SELECT b.id FROM basic_blocks b WHERE NOT EXISTS (SELECT 1 FROM instructions i WHERE i.block_id=b.id AND i.is_terminator=1);
