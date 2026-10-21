DROP TABLE access;

DROP TABLE prune_run;

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
