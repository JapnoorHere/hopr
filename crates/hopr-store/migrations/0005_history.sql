-- Phase 4c: command and connection history (§9.19). No foreign key to hosts: history
-- outlives a deleted host and keeps its label.
CREATE TABLE command_history (
    id          TEXT PRIMARY KEY,
    host_id     TEXT,
    host_label  TEXT NOT NULL,
    session_id  TEXT NOT NULL,
    command     TEXT NOT NULL,
    cwd         TEXT,
    exit_code   INTEGER,
    exact       INTEGER NOT NULL,
    started_at  TEXT NOT NULL,
    duration_ms INTEGER
);
CREATE INDEX command_history_time ON command_history(started_at);
CREATE INDEX command_history_host_time ON command_history(host_id, started_at);

CREATE TABLE connection_history (
    id          TEXT PRIMARY KEY,
    host_id     TEXT,
    host_label  TEXT NOT NULL,
    kind        TEXT NOT NULL,
    started_at  TEXT NOT NULL,
    ended_at    TEXT,
    end_reason  TEXT,
    bytes_in    INTEGER NOT NULL DEFAULT 0,
    bytes_out   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX connection_history_time ON connection_history(started_at);
CREATE INDEX connection_history_host_time ON connection_history(host_id, started_at);
