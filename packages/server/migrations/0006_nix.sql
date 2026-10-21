CREATE TABLE nix_path (
    cache text NOT NULL,
    key text NOT NULL,
    store_path text NOT NULL,
    refs text[] NOT NULL,
    deriver text,
    system text,
    ca text,
    PRIMARY KEY (cache, key),
    FOREIGN KEY (cache, key) REFERENCES entry ON DELETE CASCADE
);
