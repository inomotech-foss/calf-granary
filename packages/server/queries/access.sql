--! record_accesses
INSERT INTO access (bucket, key, last_access)
SELECT :bucket::text, key, :accessed_at::timestamptz
FROM unnest(:keys::text[]) AS key
ON CONFLICT (bucket, key) DO UPDATE
SET last_access = greatest(access.last_access, excluded.last_access);

--! last_accesses
SELECT key, last_access
FROM access
WHERE bucket = :bucket::text AND key = any(:keys::text[]);

--! forget_accesses_before
DELETE FROM access
WHERE bucket = :bucket::text AND last_access < :cutoff::timestamptz;
