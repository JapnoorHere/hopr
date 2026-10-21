import { describe, expect, it } from "vitest";
import type { Host } from "$lib/ipc";
import { crumbs, jumpCandidates, jumpTrail, sshCommand } from "./chain";

function host(id: string, jumpHostId: string | null = null): Host {
  return {
    id,
    label: id,
    hostname: id,
    port: 22,
    protocol: "ssh",
    serial: null,
    proxyCommand: null,
    username: "u",
    groupId: null,
    tags: [],
    auth: "password",
    keyId: null,
    jumpHostId,
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
  };
}

describe("jumpTrail", () => {
  it("walks jumps outermost first", () => {
    const all = [host("bastion"), host("app", "bastion"), host("db", "app")];
    expect(jumpTrail(all[2] as Host, all).map((h) => h.id)).toEqual(["bastion", "app", "db"]);
  });

  it("stops at loops and missing hosts", () => {
    const loop = [host("a", "b"), host("b", "a")];
    expect(jumpTrail(loop[0] as Host, loop).map((h) => h.id)).toEqual(["b", "a"]);
    expect(jumpTrail(host("x", "gone"), []).map((h) => h.id)).toEqual(["x"]);
  });
});

describe("jumpCandidates", () => {
  it("excludes the host itself and anything that jumps through it", () => {
    const all = [host("bastion"), host("app", "bastion"), host("other")];
    expect(jumpCandidates("bastion", all).map((h) => h.id)).toEqual(["other"]);
    expect(jumpCandidates("app", all).map((h) => h.id)).toEqual(["bastion", "other"]);
    expect(jumpCandidates(null, all)).toHaveLength(3);
  });
});

describe("crumbs", () => {
  it("collapses long chains", () => {
    expect(crumbs(["laptop", "db"])).toEqual(["laptop", "db"]);
    expect(crumbs(["laptop", "a", "b", "db"])).toEqual(["laptop", "…", "db"]);
  });
});

describe("sshCommand", () => {
  it("spells out jumps and ports like OpenSSH", () => {
    const bastion = { ...host("bastion"), port: 2200 };
    const app = host("app", "bastion");
    const db = { ...host("db", "app"), port: 5022 };
    const all = [bastion, app, db];
    expect(sshCommand(host("solo"), all)).toBe("ssh u@solo");
    expect(sshCommand(db, all)).toBe("ssh -J u@bastion:2200,u@app -p 5022 u@db");
  });
});
