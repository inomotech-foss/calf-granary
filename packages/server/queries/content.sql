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
