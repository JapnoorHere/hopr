import { describe, expect, it } from "vitest";
import type { SessionState } from "$lib/ipc";
import { advance, linkState, nodeState, START, stageLabel, stepState } from "./connect";

const run = (states: SessionState[]) => states.reduce(advance, START);

describe("connecting card model", () => {
  it("follows one hop from reaching to ready", () => {
    let p = run([{ kind: "connecting", hop: 1, total: 1 }]);
    expect(p.stage).toBe("reach");
    expect(linkState(p, 1)).toBe("active");
    expect(nodeState(p, 1)).toBe("idle");
    expect(stageLabel(p, ["prod-db"])).toBe("Reaching prod-db");

    p = advance(p, { kind: "handshaking", hop: 1, total: 1, rttMs: 38 });
    expect(p.rtts).toEqual([38]);
    expect(linkState(p, 1)).toBe("done");
    expect(nodeState(p, 1)).toBe("active");

    p = advance(p, { kind: "authenticating", method: "publickey" });
    expect(stageLabel(p, ["prod-db"])).toBe("Logging in with your key");
    expect(stepState(p, "secure")).toBe("done");
    expect(stepState(p, "login")).toBe("active");

    p = advance(p, { kind: "opening_shell" });
    expect(stageLabel(p, ["prod-db"])).toBe("Opening a shell");
    p = advance(p, { kind: "ready" });
    expect(nodeState(p, 1)).toBe("done");
    expect(stepState(p, "ready")).toBe("done");
  });

  it("walks a jump chain and keeps each hop's round trip", () => {
    const p = run([
      { kind: "connecting", hop: 1, total: 2 },
      { kind: "handshaking", hop: 1, total: 2, rttMs: 20 },
      { kind: "authenticating", method: "agent" },
      { kind: "connecting", hop: 2, total: 2 },
    ]);
    expect(p.rtts).toEqual([20, null]);
    expect(nodeState(p, 1)).toBe("done");
    expect(linkState(p, 2)).toBe("active");
    expect(stageLabel(p, ["bastion", "app"])).toBe("Reaching app through bastion");
    // Steps follow the hop in progress: hop 2 starts over at Reach.
    expect(stepState(p, "reach")).toBe("active");
    expect(stepState(p, "secure")).toBe("idle");
  });

  it("marks where a failure happened", () => {
    const network = run([
      { kind: "connecting", hop: 1, total: 1 },
      {
        kind: "failed",
        error: { code: "connect_failed", message: "No route", detail: null, retryable: true },
      },
    ]);
    expect(linkState(network, 1)).toBe("broken");
    expect(nodeState(network, 1)).toBe("idle");

    const login = run([
      { kind: "connecting", hop: 1, total: 1 },
      { kind: "handshaking", hop: 1, total: 1, rttMs: null },
      { kind: "authenticating", method: "password" },
      {
        kind: "failed",
        error: { code: "auth_failed", message: "Denied", detail: null, retryable: false },
      },
    ]);
    expect(linkState(login, 1)).toBe("done");
    expect(nodeState(login, 1)).toBe("broken");
    expect(stepState(login, "login")).toBe("broken");
  });
});
