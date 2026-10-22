CREATE TABLE task_kind (
    name text PRIMARY KEY
);

INSERT INTO task_kind (name) VALUES ('prune'), ('tier');

CREATE TABLE task (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind text NOT NULL REFERENCES task_kind,
    state task_state NOT NULL DEFAULT 'pending',
    requested_by text NOT NULL CHECK (requested_by <> ''),
    due_at timestamptz NOT NULL DEFAULT now(),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    worker text,
    lease_expires_at timestamptz,
    failure text,
    created_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz,
    CHECK (state <> 'running' OR worker IS NOT NULL),
    CHECK (state = 'running' OR lease_expires_at IS NULL),
    CHECK ((state IN ('done', 'failed')) = (finished_at IS NOT NULL)),
    CHECK (state <> 'done' OR failure IS NULL),
    CHECK (state <> 'failed' OR failure IS NOT NULL)
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
    kind text PRIMARY KEY REFERENCES task_kind,
    period interval NOT NULL CHECK (period > interval '0'),
    due_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO schedule (kind, period) VALUES ('prune', interval '24 hours'), ('tier', interval '24 hours');

CREATE TABLE prune_expiry (
    task_id bigint NOT NULL REFERENCES task ON DELETE CASCADE,
    cache cache_name NOT NULL,
    expired row_count NOT NULL,
    finished_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (task_id, cache)
);

CREATE TABLE prune_collection (
    task_id bigint PRIMARY KEY REFERENCES task ON DELETE CASCADE,
    deleted_blobs row_count NOT NULL,
    deleted_chunks row_count NOT NULL,
    freed_bytes byte_count NOT NULL,
    swept_objects row_count NOT NULL,
    retired_objects row_count NOT NULL,
    finished_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE tier_run (
    task_id bigint PRIMARY KEY REFERENCES task ON DELETE CASCADE,
    moved_chunks row_count NOT NULL,
    moved_bytes byte_count NOT NULL,
    finished_at timestamptz NOT NULL DEFAULT now()
);
