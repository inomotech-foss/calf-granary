--! find_unihashes
SELECT requested.position, hashserv_unihash.unihash
FROM unnest(:methods::text[], :taskhashes::bytea[]) WITH ORDINALITY
    AS requested (method, taskhash, position)
JOIN hashserv_unihash
    ON hashserv_unihash.cache = :cache::text
    AND hashserv_unihash.method = requested.method
    AND hashserv_unihash.taskhash = requested.taskhash;

--! touch_unihashes
UPDATE hashserv_unihash
SET last_access = now()
FROM unnest(:methods::text[], :taskhashes::bytea[]) AS requested (method, taskhash)
WHERE hashserv_unihash.cache = :cache::text
    AND hashserv_unihash.method = requested.method
    AND hashserv_unihash.taskhash = requested.taskhash
    AND hashserv_unihash.last_access < now() - interval '1 hour';

--! existing_unihashes
SELECT requested.position
FROM unnest(:unihashes::bytea[]) WITH ORDINALITY AS requested (unihash, position)
WHERE EXISTS (
    SELECT FROM hashserv_unihash
    WHERE hashserv_unihash.cache = any(:caches::text[])
        AND hashserv_unihash.unihash = requested.unihash
);

--: HashservReport(owner?, pn?, pv?, pr?, task?, outhash_siginfo?)

--! find_task : HashservReport
SELECT
    hashserv_outhash.taskhash,
    hashserv_outhash.outhash,
    hashserv_unihash.unihash,
    hashserv_outhash.created_at,
    hashserv_outhash.owner,
    hashserv_outhash.pn,
    hashserv_outhash.pv,
    hashserv_outhash.pr,
    hashserv_outhash.task,
    hashserv_outhash.outhash_siginfo
FROM hashserv_outhash
JOIN hashserv_unihash USING (cache, method, taskhash)
WHERE hashserv_outhash.cache = :cache::text
    AND hashserv_outhash.method = :method::text
    AND hashserv_outhash.taskhash = :taskhash::bytea
ORDER BY hashserv_outhash.created_at
LIMIT 1;

--! find_outhash : HashservReport
SELECT
    hashserv_outhash.taskhash,
    hashserv_outhash.outhash,
    hashserv_unihash.unihash,
    hashserv_outhash.created_at,
    hashserv_outhash.owner,
    hashserv_outhash.pn,
    hashserv_outhash.pv,
    hashserv_outhash.pr,
    hashserv_outhash.task,
    hashserv_outhash.outhash_siginfo
FROM hashserv_outhash
JOIN hashserv_unihash USING (cache, method, taskhash)
WHERE hashserv_outhash.cache = :cache::text
    AND hashserv_outhash.method = :method::text
    AND hashserv_outhash.outhash = :outhash::bytea
ORDER BY hashserv_outhash.created_at
LIMIT 1;

--! insert_outhash (owner?, pn?, pv?, pr?, task?, outhash_siginfo?)
INSERT INTO hashserv_outhash (cache, method, taskhash, outhash, owner, pn, pv, pr, task, outhash_siginfo)
VALUES (
    :cache::text,
    :method::text,
    :taskhash::bytea,
    :outhash::bytea,
    :owner::text,
    :pn::text,
    :pv::text,
    :pr::text,
    :task::text,
    :outhash_siginfo::text
)
ON CONFLICT DO NOTHING;

--! find_equivalent
SELECT hashserv_unihash.unihash
FROM hashserv_outhash
JOIN hashserv_unihash USING (cache, method, taskhash)
WHERE hashserv_outhash.cache = :cache::text
    AND hashserv_outhash.method = :method::text
    AND hashserv_outhash.outhash = :outhash::bytea
    AND hashserv_outhash.taskhash != :taskhash::bytea
ORDER BY hashserv_outhash.created_at
LIMIT 1;

--! insert_unihash
INSERT INTO hashserv_unihash (cache, method, taskhash, unihash)
VALUES (:cache::text, :method::text, :taskhash::bytea, :unihash::bytea)
ON CONFLICT DO NOTHING;

--! find_unihash
SELECT unihash
FROM hashserv_unihash
WHERE cache = :cache::text AND method = :method::text AND taskhash = :taskhash::bytea;

--! expire_unihashes
DELETE FROM hashserv_unihash
WHERE cache = :cache::text AND last_access < :cutoff::timestamptz;
