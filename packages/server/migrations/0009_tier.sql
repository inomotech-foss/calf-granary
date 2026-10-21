CREATE TABLE tier_run (
    task_id bigint PRIMARY KEY REFERENCES task ON DELETE CASCADE,
    moved_chunks bigint NOT NULL,
    moved_bytes bigint NOT NULL,
    finished_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO schedule (kind, every_seconds) VALUES ('tier', 86400);
