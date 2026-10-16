SELECT v.id FROM values_ir v JOIN functions f ON f.id=v.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.snapshot_id<>v.snapshot_id;
