--! record_prune_expiry
INSERT INTO prune_expiry (task_id, cache, expired)
VALUES (:task_id::bigint, :cache::text, :expired::bigint)
ON CONFLICT (task_id, cache) DO UPDATE
SET expired = excluded.expired, finished_at = now();

--! record_prune_collection
INSERT INTO prune_collection (
    task_id, deleted_blobs, deleted_chunks, freed_bytes, swept_objects, retired_objects
)
VALUES (
    :task_id::bigint,
    :deleted_blobs::bigint,
    :deleted_chunks::bigint,
    :freed_bytes::bigint,
    :swept_objects::bigint,
    :retired_objects::bigint
)
ON CONFLICT (task_id) DO UPDATE
SET deleted_blobs = excluded.deleted_blobs,
    deleted_chunks = excluded.deleted_chunks,
    freed_bytes = excluded.freed_bytes,
    swept_objects = excluded.swept_objects,
    retired_objects = excluded.retired_objects,
    finished_at = now();

--! record_tier_run
INSERT INTO tier_run (task_id, moved_chunks, moved_bytes)
VALUES (:task_id::bigint, :moved_chunks::bigint, :moved_bytes::bigint)
ON CONFLICT (task_id) DO UPDATE
SET moved_chunks = excluded.moved_chunks,
    moved_bytes = excluded.moved_bytes,
    finished_at = now();
