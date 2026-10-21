-- Phase 5d: deletions to sync. Triggers record them, so every delete path is covered.
-- Times use the same RFC 3339 UTC form as the app (compared after parsing).
CREATE TABLE sync_tombstones (
    kind       TEXT NOT NULL,
    id         TEXT NOT NULL,
    deleted_at TEXT NOT NULL,
    PRIMARY KEY (kind, id)
);

CREATE TRIGGER sync_del_group AFTER DELETE ON groups BEGIN
    INSERT OR REPLACE INTO sync_tombstones VALUES ('group', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER sync_del_key AFTER DELETE ON ssh_keys BEGIN
    INSERT OR REPLACE INTO sync_tombstones VALUES ('key', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER sync_del_host AFTER DELETE ON hosts BEGIN
    INSERT OR REPLACE INTO sync_tombstones VALUES ('host', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER sync_del_snippet AFTER DELETE ON snippets BEGIN
    INSERT OR REPLACE INTO sync_tombstones VALUES ('snippet', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER sync_del_tunnel AFTER DELETE ON tunnels BEGIN
    INSERT OR REPLACE INTO sync_tombstones VALUES ('tunnel', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER sync_del_secret AFTER DELETE ON snippet_secrets BEGIN
    INSERT OR REPLACE INTO sync_tombstones VALUES ('secret', OLD.name, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
