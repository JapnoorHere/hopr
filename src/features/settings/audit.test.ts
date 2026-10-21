import { describe, expect, it } from "vitest";
import type { AuditEvent } from "$lib/ipc";
import { auditCsv, auditTitle, matchesFilter } from "./audit";

const ev = (kind: string, detail = ""): AuditEvent => ({
  id: kind,
  at: "2026-10-17T10:00:00Z",
  kind,
  hostId: null,
  hostLabel: null,
  detail,
});

describe("audit presentation", () => {
  it("titles known kinds and falls back to the raw kind", () => {
    expect(auditTitle("host_key.replace")).toBe("Changed host key accepted");
    expect(auditTitle("something.new")).toBe("something.new");
  });

  it("filters by family", () => {
    expect(matchesFilter(ev("vault.unlock_failed"), "access")).toBe(true);
    expect(matchesFilter(ev("host.copy_password"), "secrets")).toBe(true);
    expect(matchesFilter(ev("key.deploy"), "secrets")).toBe(true);
    expect(matchesFilter(ev("session.connect"), "host_keys")).toBe(false);
    expect(matchesFilter(ev("hosts.export"), "settings")).toBe(true);
  });

  it("exports CSV with quoting and formula defusing", () => {
    const csv = auditCsv([ev("session.connect", '=HYPERLINK(1), "x"')]);
    expect(csv.split("\r\n")[0]).toBe("time,kind,event,host,detail");
    expect(csv).toContain(`"'=HYPERLINK(1), ""x"""`);
  });
});
