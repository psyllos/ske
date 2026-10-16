SELECT sn.id FROM snapshots sn WHERE sn.status='CLOSED' AND sn.inventory_status='COMPLETE' AND EXISTS (SELECT 1 FROM compilation_units u WHERE u.snapshot_id=sn.id AND u.inventory_status<>'COMPLETE');
