CREATE TABLE chunk (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    hash sha256_digest NOT NULL,
    size byte_count NOT NULL,
    compression chunk_compression NOT NULL,
    stored_size byte_count NOT NULL,
    store store_name NOT NULL,
    object_key chunk_key NOT NULL,
    state chunk_state NOT NULL DEFAULT 'valid',
    touched_at timestamptz NOT NULL DEFAULT now(),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (store, object_key)
);

CREATE UNIQUE INDEX chunk_valid_hash ON chunk (hash) WHERE state = 'valid';

CREATE TABLE blob (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    hash sha256_digest NOT NULL UNIQUE,
    size byte_count NOT NULL,
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
    cache cache_name NOT NULL,
    key entry_key NOT NULL,
    blob_id bigint NOT NULL REFERENCES blob,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_access timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (cache, key)
);

CREATE INDEX entry_blob ON entry (blob_id);

CREATE INDEX entry_last_access ON entry (cache, last_access);
