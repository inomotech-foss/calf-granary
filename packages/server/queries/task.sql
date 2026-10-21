--! start_task
INSERT INTO task (kind, requested_by, state, attempts, worker)
VALUES (:kind::text, :requested_by::text, 'running', 1, :worker::text)
RETURNING id;

--! enqueue_due_schedules
WITH due AS (
    UPDATE schedule
    SET next_due = now() + make_interval(secs => every_seconds)
    WHERE next_due <= now()
    RETURNING kind
)
INSERT INTO task (kind, requested_by)
SELECT kind, 'schedule' FROM due
RETURNING id, kind;

--! claim_task
UPDATE task
SET state = 'running',
    attempts = attempts + 1,
    worker = :worker::text,
    lease_expires_at = now() + make_interval(secs => :lease_seconds::float8)
WHERE id = (
    SELECT id FROM task
    WHERE (state = 'pending' AND due_at <= now())
        OR (state = 'running' AND lease_expires_at < now())
    ORDER BY due_at
    LIMIT 1
    FOR UPDATE SKIP LOCKED
)
RETURNING id, kind, attempts;

--! renew_lease
UPDATE task
SET lease_expires_at = now() + make_interval(secs => :lease_seconds::float8)
WHERE id = :id::bigint AND worker = :worker::text AND state = 'running';

--! complete_task
UPDATE task
SET state = 'done', finished_at = now(), lease_expires_at = NULL, failure = NULL
WHERE id = :id::bigint AND worker = :worker::text AND state = 'running';

--! fail_task
UPDATE task
SET state = CASE WHEN attempts >= :max_attempts::integer THEN 'failed' ELSE 'pending' END,
    due_at = now() + make_interval(secs => :retry_seconds::float8),
    finished_at = CASE WHEN attempts >= :max_attempts::integer THEN now() END,
    lease_expires_at = NULL,
    failure = :failure::text
WHERE id = :id::bigint AND worker = :worker::text AND state = 'running';

--! release_task
UPDATE task
SET state = 'pending', attempts = attempts - 1, lease_expires_at = NULL
WHERE id = :id::bigint AND worker = :worker::text AND state = 'running';
