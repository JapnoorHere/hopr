import { describe, expect, it } from "vitest";
import type { Host } from "$lib/ipc";
import { highlight, matchesQuery, sectionHosts } from "./filter";

function host(over: Partial<Host>): Host {
  return {
    id: "h",
    label: "prod-db",
    hostname: "10.0.0.4",
    port: 22,
    protocol: "ssh",
    serial: null,
    proxyCommand: null,
    username: "deploy",
    groupId: null,
    tags: ["db", "eu"],
    auth: "password",
    keyId: null,
    jumpHostId: null,
    agentForwarding: false,
    proxy: null,
    hasProxyPassword: false,
    hasPassword: false,
    hasTotp: false,
    favorite: false,
    environment: "prod",
    notes: "",
    lastConnectedAt: null,
    connectCount: 0,
    createdAt: "",
    updatedAt: "",
    ...over,
  };
}

describe("matchesQuery", () => {
  it("matches any field and ANDs words", () => {
    const h = host({});
    expect(matchesQuery(h, "")).toBe(true);
    expect(matchesQuery(h, "PROD")).toBe(true);
    expect(matchesQuery(h, "10.0")).toBe(true);
    expect(matchesQuery(h, "deploy eu")).toBe(true);
    expect(matchesQuery(h, "deploy us")).toBe(false);
  });

  it("supports field filters", () => {
    const h = host({});
    expect(matchesQuery(h, "tag:db")).toBe(true);
    expect(matchesQuery(h, "tag:web")).toBe(false);
    expect(matchesQuery(h, "env:prod")).toBe(true);
    expect(matchesQuery(h, "user:root")).toBe(false);
  });
});

describe("sectionHosts", () => {
  it("builds favorites, recent, groups and ungrouped", () => {
    const hosts = [
      host({ id: "a", favorite: true, groupId: "g1" }),
      host({ id: "b", lastConnectedAt: "2026-10-16T10:00:00Z" }),
      host({ id: "c", groupId: "missing" }),
    ];
    const s = sectionHosts(hosts, [{ id: "g1", name: "Prod", sortOrder: 1 }], "");
    expect(s.map((x) => [x.id, x.hosts.map((h) => h.id)])).toEqual([
      ["favorites", ["a"]],
      ["recent", ["b"]],
      ["g1", ["a"]],
      ["ungrouped", ["b", "c"]],
    ]);
  });

  it("flattens to results when searching", () => {
    const s = sectionHosts([host({ id: "a" }), host({ id: "b", label: "web" })], [], "web");
    expect(s).toEqual([
      { id: "results", title: "Results", hosts: [expect.objectContaining({ id: "b" })] },
    ]);
  });
});

describe("highlight", () => {
  it("marks every plain search word, merging overlaps", () => {
    expect(highlight("api-prod-1", "prod")).toEqual([
      { text: "api-", hit: false },
      { text: "prod", hit: true },
      { text: "-1", hit: false },
    ]);
    expect(highlight("Staging", "stag ging")).toEqual([{ text: "Staging", hit: true }]);
  });

  it("ignores field filters and leaves text without hits whole", () => {
    expect(highlight("db-1", "env:prod")).toEqual([{ text: "db-1", hit: false }]);
    expect(highlight("db-1", "")).toEqual([{ text: "db-1", hit: false }]);
  });
});
