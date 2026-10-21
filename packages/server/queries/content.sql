--! touch_chunk
UPDATE chunk
SET touched_at = now()
WHERE hash = :hash::bytea AND state = 'valid'
RETURNING id;

--! insert_chunk
INSERT INTO chunk (hash, size, compression, stored_size, store, object_key)
VALUES (
    :hash::bytea,
    :size::bigint,
    :compression::text,
    :stored_size::bigint,
    :store::text,
    :object_key::text
)
ON CONFLICT (hash) WHERE state = 'valid' DO UPDATE
SET touched_at = now()
RETURNING id, object_key;

--! insert_blob
INSERT INTO blob (hash, size)
VALUES (:hash::bytea, :size::bigint)
ON CONFLICT (hash) DO UPDATE
SET touched_at = now()
RETURNING id, (xmax::text = '0') AS inserted;

--! insert_blob_chunks
INSERT INTO blob_chunk (blob_id, seq, chunk_id)
SELECT :blob_id::bigint, ordinality - 1, chunk_id
FROM unnest(:chunk_ids::bigint[]) WITH ORDINALITY AS chunks (chunk_id, ordinality);

--! insert_entry
INSERT INTO entry (cache, key, blob_id)
VALUES (:cache::text, :key::text, :blob_id::bigint)
ON CONFLICT (cache, key) DO NOTHING
RETURNING created_at;

--! entry_exists
SELECT EXISTS (
    SELECT FROM entry WHERE cache = :cache::text AND key = :key::text
) AS found;

--! find_entry
SELECT entry.blob_id, blob.hash, blob.size, entry.created_at
FROM entry
JOIN blob ON blob.id = entry.blob_id
WHERE entry.cache = :cache::text AND entry.key = :key::text;

--! blob_chunks
SELECT chunk.store, chunk.object_key, chunk.compression, chunk.size
FROM blob_chunk
JOIN chunk ON chunk.id = blob_chunk.chunk_id
WHERE blob_chunk.blob_id = :blob_id::bigint
ORDER BY blob_chunk.seq;

--! record_entry_accesses
UPDATE entry
SET last_access = greatest(last_access, now())
FROM unnest(:keys::text[]) AS accessed (key)
WHERE entry.cache = :cache::text AND entry.key = accessed.key;

--! expire_entries
DELETE FROM entry
WHERE cache = :cache::text AND last_access < :cutoff::timestamptz;

--! delete_orphan_blobs
DELETE FROM blob
WHERE touched_at < now() - make_interval(secs => :grace_seconds::float8)
    AND NOT EXISTS (SELECT FROM entry WHERE entry.blob_id = blob.id);

--! mark_orphan_chunks
UPDATE chunk
SET state = 'deleting'
WHERE state = 'valid'
    AND touched_at < now() - make_interval(secs => :grace_seconds::float8)
    AND NOT EXISTS (SELECT FROM blob_chunk WHERE blob_chunk.chunk_id = chunk.id);

--! deleting_chunks
SELECT id, store, object_key, stored_size
FROM chunk
WHERE state = 'deleting' AND id > :after::bigint
ORDER BY id
LIMIT :limit::bigint;

--! delete_chunks
DELETE FROM chunk
WHERE id = any(:ids::bigint[])
    AND state = 'deleting'
    AND NOT EXISTS (SELECT FROM blob_chunk WHERE blob_chunk.chunk_id = chunk.id);

--! known_objects
SELECT object_key
FROM chunk
WHERE store = :store::text AND object_key = any(:keys::text[]);

--! zstd_size : (size?)
SELECT CASE WHEN bool_and(chunk.compression = 'zstd') THEN sum(chunk.stored_size)::bigint END AS size
FROM blob_chunk
JOIN chunk ON chunk.id = blob_chunk.chunk_id
WHERE blob_chunk.blob_id = :blob_id::bigint;

--! missing_entries
SELECT key
FROM unnest(:keys::text[]) AS requested (key)
WHERE NOT EXISTS (
    SELECT FROM entry
    WHERE entry.key = requested.key AND entry.cache = any(:caches::text[])
);
