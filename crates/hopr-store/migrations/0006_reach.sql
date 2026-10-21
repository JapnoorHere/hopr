-- Phase 4d: telnet and serial hosts, ProxyCommand, SSH certificates.
ALTER TABLE hosts ADD COLUMN protocol TEXT NOT NULL DEFAULT 'ssh';
-- Serial line settings as JSON (baud, dataBits, parity, stopBits, flow); NULL otherwise.
ALTER TABLE hosts ADD COLUMN serial_json TEXT;
ALTER TABLE hosts ADD COLUMN proxy_command TEXT;
-- OpenSSH certificate (public) signed for this key, used with it when present.
ALTER TABLE ssh_keys ADD COLUMN certificate TEXT;
