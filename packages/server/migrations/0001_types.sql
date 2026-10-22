CREATE DOMAIN sha256_digest AS bytea
    CHECK (octet_length(VALUE) = 32);

CREATE DOMAIN byte_count AS bigint
    CHECK (VALUE >= 0);

CREATE DOMAIN row_count AS bigint
    CHECK (VALUE >= 0);

CREATE DOMAIN cache_name AS text
    CHECK (VALUE ~ '^[a-z0-9][a-z0-9-]*$');

CREATE DOMAIN store_name AS text
    CHECK (VALUE ~ '^[a-z0-9][a-z0-9-]*$');

CREATE DOMAIN entry_key AS text
    CHECK (VALUE !~ '(^|/)\.{0,2}(/|$)');

CREATE DOMAIN chunk_key AS text
    CHECK (VALUE ~ '^chunks/[0-9a-f]{32}$');

CREATE DOMAIN nix_hash_part AS text
    CHECK (VALUE ~ '^[0-9a-df-np-sv-z]{32}$');

CREATE DOMAIN nix_path_name AS text
    CHECK (VALUE ~ '^[A-Za-z0-9+_?=-][A-Za-z0-9+._?=-]{0,210}$');

CREATE DOMAIN nix_base_name AS text
    CHECK (VALUE ~ '^[0-9a-df-np-sv-z]{32}-[A-Za-z0-9+_?=-][A-Za-z0-9+._?=-]{0,210}$');

CREATE DOMAIN nix_info_value AS text
    CHECK (VALUE <> '' AND VALUE !~ '[[:cntrl:]]');

CREATE DOMAIN hashserv_method AS text
    CHECK (VALUE ~ '^\S+$');

CREATE TYPE task_state AS ENUM ('pending', 'running', 'done', 'failed');

CREATE TYPE chunk_state AS ENUM ('valid', 'deleting');

CREATE TYPE chunk_compression AS ENUM ('plain', 'zstd');
