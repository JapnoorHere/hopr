-- Hopr schema v1 (HOPR_PLAN §5). Timestamps are RFC 3339 UTC. `*_enc` columns hold
-- XChaCha20-Poly1305 ciphertext from hopr-vault; plaintext secrets never touch this file.

CREATE TABLE vault_meta (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    header_json TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE app_settings (
    key        TEXT PRIMARY KEY,
    value_json TEXT NOT NULL
);

CREATE TABLE groups (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE ssh_keys (
    id              TEXT PRIMARY KEY,
    label           TEXT NOT NULL,
    algorithm       TEXT NOT NULL,
    fingerprint     TEXT NOT NULL,
    public_key      TEXT NOT NULL,
    comment         TEXT NOT NULL DEFAULT '',
    private_key_enc BLOB NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE hosts (
    id                TEXT PRIMARY KEY,
    group_id          TEXT REFERENCES groups(id) ON DELETE SET NULL,
    label             TEXT NOT NULL,
    hostname          TEXT NOT NULL,
    port              INTEGER NOT NULL DEFAULT 22,
    username          TEXT NOT NULL,
    auth              TEXT NOT NULL DEFAULT 'password',
    key_id            TEXT REFERENCES ssh_keys(id) ON DELETE RESTRICT,
    favorite          INTEGER NOT NULL DEFAULT 0,
    environment       TEXT NOT NULL DEFAULT 'none',
    notes             TEXT NOT NULL DEFAULT '',
    last_connected_at TEXT,
    connect_count     INTEGER NOT NULL DEFAULT 0,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    deleted_at        TEXT
);
CREATE INDEX hosts_label ON hosts(label);
CREATE INDEX hosts_group ON hosts(group_id);
CREATE INDEX hosts_last_connected ON hosts(last_connected_at);

CREATE TABLE host_tags (
    host_id TEXT NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    tag     TEXT NOT NULL,
    PRIMARY KEY (host_id, tag)
);

CREATE TABLE host_secrets (
    host_id       TEXT PRIMARY KEY REFERENCES hosts(id) ON DELETE CASCADE,
    password_enc  BLOB,
    totp_seed_enc BLOB
);

CREATE TABLE known_hosts (
    id            TEXT PRIMARY KEY,
    host_pattern  TEXT NOT NULL,
    port          INTEGER NOT NULL,
    key_type      TEXT NOT NULL,
    key_b64       TEXT NOT NULL,
    fingerprint   TEXT NOT NULL,
    first_seen_at TEXT NOT NULL,
    last_seen_at  TEXT NOT NULL,
    source        TEXT NOT NULL,
    UNIQUE (host_pattern, port, key_type, key_b64)
);
CREATE INDEX known_hosts_lookup ON known_hosts(host_pattern, port);
