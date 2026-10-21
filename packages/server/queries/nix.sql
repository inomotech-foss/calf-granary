--! insert_nix_path (deriver?, system?, ca?)
INSERT INTO nix_path (cache, key, name, refs, deriver, system, ca)
VALUES (
    :cache::text,
    :key::text,
    :name::text,
    :refs::text[],
    :deriver::text,
    :system::text,
    :ca::text
);

--! find_nix_path : (deriver?, system?, ca?, file_size?)
SELECT
    nix_path.name,
    nix_path.refs,
    nix_path.deriver,
    nix_path.system,
    nix_path.ca,
    entry.blob_id,
    blob.hash,
    blob.size,
    blob_zstd_size(blob.id) AS file_size
FROM nix_path
JOIN entry USING (cache, key)
JOIN blob ON blob.id = entry.blob_id
WHERE nix_path.cache = :cache::text AND nix_path.key = :key::text;

--! missing_nix_paths
SELECT hash
FROM unnest(:hashes::text[]) AS requested (hash)
WHERE NOT EXISTS (
    SELECT FROM nix_path
    WHERE nix_path.key = requested.hash AND nix_path.cache = any(:caches::text[])
);

--! insert_nix_upload
INSERT INTO nix_upload (cache, key, file_hash, file_size)
VALUES (:cache::text, :key::text, :file_hash::bytea, :file_size::bigint)
ON CONFLICT (cache, key) DO NOTHING;

--! find_nix_upload
SELECT entry.blob_id, blob.hash, blob.size, nix_upload.file_hash, nix_upload.file_size
FROM nix_upload
JOIN entry USING (cache, key)
JOIN blob ON blob.id = entry.blob_id
WHERE nix_upload.cache = :cache::text AND nix_upload.key = :key::text;
