CREATE TABLE hashserv_unihash (
    cache cache_name NOT NULL,
    method hashserv_method NOT NULL,
    taskhash sha256_digest NOT NULL,
    unihash sha256_digest NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    accessed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (cache, method, taskhash)
);

CREATE INDEX hashserv_unihash_value ON hashserv_unihash (unihash, cache);

CREATE INDEX hashserv_unihash_accessed ON hashserv_unihash (cache, accessed_at);

CREATE TABLE hashserv_outhash (
    cache cache_name NOT NULL,
    method hashserv_method NOT NULL,
    taskhash sha256_digest NOT NULL,
    outhash sha256_digest NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    owner text,
    pn text,
    pv text,
    pr text,
    task text,
    outhash_siginfo text,
    PRIMARY KEY (cache, method, taskhash, outhash),
    FOREIGN KEY (cache, method, taskhash) REFERENCES hashserv_unihash ON DELETE CASCADE
);

CREATE INDEX hashserv_outhash_equivalent ON hashserv_outhash (cache, method, outhash, created_at);
