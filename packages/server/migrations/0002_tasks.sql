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

CREATE TABLE schedule (
    kind text PRIMARY KEY,
    every_seconds bigint NOT NULL CHECK (every_seconds > 0),
    next_due timestamptz NOT NULL DEFAULT now()
);

INSERT INTO schedule (kind, every_seconds) VALUES ('prune', 86400);

CREATE TABLE prune_run (
    task_id bigint NOT NULL REFERENCES task ON DELETE CASCADE,
    cache text NOT NULL,
    scanned bigint NOT NULL,
    deleted bigint NOT NULL,
    freed_bytes bigint NOT NULL,
    finished_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (task_id, cache)
);
