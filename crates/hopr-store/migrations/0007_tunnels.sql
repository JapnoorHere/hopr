-- Phase 5a: saved port forwards (local / remote / dynamic SOCKS) per host.
CREATE TABLE tunnels (
    id          TEXT PRIMARY KEY,
    host_id     TEXT NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    label       TEXT NOT NULL DEFAULT '',
    kind        TEXT NOT NULL,
    bind_addr   TEXT NOT NULL,
    bind_port   INTEGER NOT NULL,
    dest_host   TEXT NOT NULL DEFAULT '',
    dest_port   INTEGER NOT NULL DEFAULT 0,
    auto_start  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX tunnels_host ON tunnels(host_id);
