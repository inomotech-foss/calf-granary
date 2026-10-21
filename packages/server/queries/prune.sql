--! record_prune_run
INSERT INTO prune_run (task_id, cache, scanned, deleted, freed_bytes)
VALUES (:task_id::bigint, :cache::text, :scanned::bigint, :deleted::bigint, :freed_bytes::bigint)
ON CONFLICT (task_id, cache) DO UPDATE
SET scanned = excluded.scanned,
    deleted = excluded.deleted,
    freed_bytes = excluded.freed_bytes,
    finished_at = now();
