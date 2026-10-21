CREATE TABLE chunk (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    hash bytea NOT NULL CHECK (length(hash) = 32),
    size bigint NOT NULL CHECK (size >= 0),
    compression text NOT NULL CHECK (compression IN ('none', 'zstd')),
    stored_size bigint NOT NULL CHECK (stored_size >= 0),
    store text NOT NULL,
    object_key text NOT NULL,
    state text NOT NULL DEFAULT 'valid' CHECK (state IN ('valid', 'deleting')),
    touched_at timestamptz NOT NULL DEFAULT now(),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (store, object_key)
);

CREATE UNIQUE INDEX chunk_valid_hash ON chunk (hash) WHERE state = 'valid';

CREATE TABLE blob (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    hash bytea NOT NULL UNIQUE CHECK (length(hash) = 32),
    size bigint NOT NULL CHECK (size >= 0),
    touched_at timestamptz NOT NULL DEFAULT now(),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE blob_chunk (
    blob_id bigint NOT NULL REFERENCES blob ON DELETE CASCADE,
    seq integer NOT NULL CHECK (seq >= 0),
    chunk_id bigint NOT NULL REFERENCES chunk,
    PRIMARY KEY (blob_id, seq)
);

CREATE INDEX blob_chunk_chunk ON blob_chunk (chunk_id);

CREATE TABLE entry (
    cache text NOT NULL,
    key text NOT NULL,
    blob_id bigint NOT NULL REFERENCES blob,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_access timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (cache, key)
);

CREATE INDEX entry_blob ON entry (blob_id);

CREATE INDEX entry_last_access ON entry (cache, last_access);
