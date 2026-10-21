CREATE TABLE nix_upload (
    cache text NOT NULL,
    key text NOT NULL,
    file_hash bytea NOT NULL CHECK (length(file_hash) = 32),
    file_size bigint NOT NULL CHECK (file_size >= 0),
    PRIMARY KEY (cache, key),
    FOREIGN KEY (cache, key) REFERENCES entry ON DELETE CASCADE
);
