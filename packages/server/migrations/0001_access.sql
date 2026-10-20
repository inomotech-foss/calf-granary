CREATE TABLE access (
    bucket text NOT NULL,
    key text NOT NULL,
    last_access timestamptz NOT NULL,
    PRIMARY KEY (bucket, key)
);
