import { describe, expect, it } from "vitest";
import type { Host } from "$lib/ipc";
import { hostAddress, reachTitle } from "./describe";

function host(over: Partial<Host>): Host {
  return {
    id: "h",
    label: "web",
    hostname: "10.0.0.4",
    port: 22,
    protocol: "ssh",
    serial: null,
    proxyCommand: null,
    username: "deploy",
    groupId: null,
    tags: [],
    auth: "password",
    keyId: null,
    jumpHostId: null,
    agentForwarding: false,
    proxy: null,
    hasProxyPassword: false,
    hasPassword: false,
    hasTotp: false,
    favorite: false,
    environment: "none",
    notes: "",
    lastConnectedAt: null,
    connectCount: 0,
    createdAt: "",
    updatedAt: "",
    ...over,
  };
}

describe("hostAddress", () => {
  it("shows user@host and a port only when it isn't 22", () => {
    expect(hostAddress(host({}))).toBe("deploy@10.0.0.4");
    expect(hostAddress(host({ port: 2222 }))).toBe("deploy@10.0.0.4:2222");
  });

  it("names telnet and serial", () => {
    expect(hostAddress(host({ protocol: "telnet", username: "", port: 23 }))).toBe(
      "telnet · 10.0.0.4:23",
    );
    expect(hostAddress(host({ protocol: "serial", hostname: "COM3" }))).toBe(
      "serial · COM3 · 9600 baud",
    );
  });

  it("says how it is reached", () => {
    expect(hostAddress(host({ jumpHostId: "b" }), "bastion")).toBe("deploy@10.0.0.4 · via bastion");
    expect(hostAddress(host({ proxyCommand: "nc %h %p" }))).toBe(
      "deploy@10.0.0.4 · via ProxyCommand",
    );
  });
});

describe("reachTitle", () => {
  it("describes each probe result", () => {
    const h = host({});
    expect(reachTitle(h, undefined)).toBe("Not checked yet");
    expect(reachTitle(h, { id: "h", ms: 12, status: "reachable" })).toBe("Reachable · 12 ms");
    expect(reachTitle(h, { id: "h", ms: null, status: "unreachable" })).toBe("Not reachable");
    expect(reachTitle(host({ jumpHostId: "b" }), { id: "h", ms: null, status: "skipped" })).toBe(
      "Reached through a jump host",
    );
  });
});
