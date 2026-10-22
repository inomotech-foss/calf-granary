CREATE TABLE nix_path (
    cache cache_name NOT NULL,
    key nix_hash_part NOT NULL,
    name nix_path_name NOT NULL,
    refs nix_base_name[] NOT NULL CHECK (array_position(refs, NULL) IS NULL),
    deriver nix_base_name,
    system nix_info_value,
    ca nix_info_value,
    PRIMARY KEY (cache, key),
    FOREIGN KEY (cache, key) REFERENCES entry ON DELETE CASCADE
);

CREATE TABLE nix_upload (
    cache cache_name NOT NULL,
    key entry_key NOT NULL,
    file_hash sha256_digest NOT NULL,
    file_size byte_count NOT NULL,
    PRIMARY KEY (cache, key),
    FOREIGN KEY (cache, key) REFERENCES entry ON DELETE CASCADE
);
