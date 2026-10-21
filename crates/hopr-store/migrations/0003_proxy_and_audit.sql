-- Phase 3d: per-host SOCKS5/HTTP proxy for the first hop (C7) and the local audit log (X7).
ALTER TABLE hosts ADD COLUMN proxy_protocol TEXT;
ALTER TABLE hosts ADD COLUMN proxy_host TEXT;
ALTER TABLE hosts ADD COLUMN proxy_port INTEGER;
ALTER TABLE hosts ADD COLUMN proxy_username TEXT;
ALTER TABLE host_secrets ADD COLUMN proxy_password_enc BLOB;

-- Security-relevant events. `detail` is plain text and never holds a secret.
CREATE TABLE audit_events (
    id         TEXT PRIMARY KEY,
    at         TEXT NOT NULL,
    kind       TEXT NOT NULL,
    host_id    TEXT,
    host_label TEXT,
    detail     TEXT NOT NULL DEFAULT ''
);
CREATE INDEX audit_events_at ON audit_events(at);
