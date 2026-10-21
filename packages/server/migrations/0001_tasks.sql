CREATE TABLE task (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind text NOT NULL,
    state text NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'running', 'done', 'failed')),
    requested_by text NOT NULL,
    due_at timestamptz NOT NULL DEFAULT now(),
    attempts integer NOT NULL DEFAULT 0,
    worker text,
    lease_expires_at timestamptz,
    failure text,
    created_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz
);

CREATE INDEX task_due ON task (due_at) WHERE state IN ('pending', 'running');

CREATE FUNCTION notify_task() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('task', '');
    RETURN NULL;
END
$$;

CREATE TRIGGER task_pending
AFTER INSERT OR UPDATE OF state, due_at ON task
FOR EACH ROW WHEN (NEW.state = 'pending')
EXECUTE FUNCTION notify_task();

CREATE TABLE schedule (
    kind text PRIMARY KEY,
    every_seconds bigint NOT NULL CHECK (every_seconds > 0),
    next_due timestamptz NOT NULL DEFAULT now()
);

INSERT INTO schedule (kind, every_seconds) VALUES ('prune', 86400), ('tier', 86400);

CREATE TABLE prune_expiry (
    task_id bigint NOT NULL REFERENCES task ON DELETE CASCADE,
    cache text NOT NULL,
    expired bigint NOT NULL,
    finished_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (task_id, cache)
);

CREATE TABLE prune_collection (
    task_id bigint PRIMARY KEY REFERENCES task ON DELETE CASCADE,
    deleted_blobs bigint NOT NULL,
    deleted_chunks bigint NOT NULL,
    freed_bytes bigint NOT NULL,
    swept_objects bigint NOT NULL,
    finished_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE tier_run (
    task_id bigint PRIMARY KEY REFERENCES task ON DELETE CASCADE,
    moved_chunks bigint NOT NULL,
    moved_bytes bigint NOT NULL,
    finished_at timestamptz NOT NULL DEFAULT now()
);
