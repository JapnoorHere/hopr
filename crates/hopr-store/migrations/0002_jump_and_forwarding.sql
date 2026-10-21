-- Phase 3a: connect through another saved host (jump chains) and agent forwarding.
ALTER TABLE hosts ADD COLUMN jump_host_id TEXT REFERENCES hosts(id) ON DELETE SET NULL;
ALTER TABLE hosts ADD COLUMN agent_forwarding INTEGER NOT NULL DEFAULT 0;
