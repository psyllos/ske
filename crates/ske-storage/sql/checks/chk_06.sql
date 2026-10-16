SELECT i.id FROM instructions i WHERE i.is_terminator=1 AND i.ordinal < (SELECT MAX(j.ordinal) FROM instructions j WHERE j.block_id=i.block_id);
