/** Audit log presentation: titles, filters and CSV export (X7). */
import type { AuditEvent } from "$lib/ipc";

const TITLES: Record<string, string> = {
  "vault.unlock": "Unlocked",
  "vault.unlock_failed": "Unlock failed",
  "vault.lock": "Locked",
  "vault.recover": "Recovered with recovery key",
  "vault.password_changed": "Master password changed",
  "vault.forget_device": "Device forgotten",
  "session.connect": "Connected",
  "session.auth_failed": "Login failed",
  "host_key.trust": "Host key trusted",
  "host_key.replace": "Changed host key accepted",
  "host_key.blocked": "Changed host key blocked",
  "host_key.forget": "Host key forgotten",
  "host.copy_password": "Password copied",
  "host.delete": "Host deleted",
  "hosts.export": "Hosts exported",
  "key.deploy": "Key deployed",
  "key.deploy_failed": "Key deploy failed",
  "key.delete": "Key deleted",
  "settings.auto_lock": "Auto-lock changed",
  "settings.clipboard_clear": "Clipboard clearing changed",
};

export function auditTitle(kind: string): string {
  return TITLES[kind] ?? kind;
}

export type AuditFilter = "all" | "access" | "sessions" | "host_keys" | "secrets" | "settings";

export const AUDIT_FILTERS: { id: AuditFilter; label: string }[] = [
  { id: "all", label: "All events" },
  { id: "access", label: "Unlock & lock" },
  { id: "sessions", label: "Connections" },
  { id: "host_keys", label: "Host keys" },
  { id: "secrets", label: "Secrets & keys" },
  { id: "settings", label: "Settings & data" },
];

export function matchesFilter(e: AuditEvent, f: AuditFilter): boolean {
  const k = e.kind;
  switch (f) {
    case "all":
      return true;
    case "access":
      return k.startsWith("vault.");
    case "sessions":
      return k.startsWith("session.");
    case "host_keys":
      return k.startsWith("host_key.");
    case "secrets":
      return k === "host.copy_password" || k.startsWith("key.");
    case "settings":
      return k.startsWith("settings.") || k.startsWith("hosts.") || k === "host.delete";
  }
}

function field(v: string): string {
  const s = /^[=+\-@\t\r]/.test(v) ? `'${v}` : v;
  return /[",\r\n]/.test(s) ? `"${s.replaceAll('"', '""')}"` : s;
}

export function auditCsv(events: AuditEvent[]): string {
  const rows = events.map((e) =>
    [e.at, e.kind, auditTitle(e.kind), e.hostLabel ?? "", e.detail].map(field).join(","),
  );
  return `${["time,kind,event,host,detail", ...rows].join("\r\n")}\r\n`;
}
