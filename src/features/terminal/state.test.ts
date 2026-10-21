import { describe, expect, it } from "vitest";
import { describeState, isBusy } from "./state";

describe("describeState", () => {
  it("maps lifecycle states to labels and tones", () => {
    expect(describeState({ kind: "connecting", hop: 1, total: 1 })).toEqual({
      label: "Connecting…",
      tone: "busy",
    });
    expect(describeState({ kind: "connecting", hop: 2, total: 3 }).label).toBe(
      "Connecting (hop 2 of 3)…",
    );
    expect(describeState({ kind: "ready" })).toEqual({ label: "Connected", tone: "live" });
    expect(
      describeState({ kind: "reconnecting", attempt: 2, maxAttempts: 10, retryInSecs: 2 }),
    ).toEqual({ label: "Reconnecting (attempt 2 of 10)…", tone: "warn" });
    expect(describeState({ kind: "closed", reason: "connection_lost" }).tone).toBe("down");
    expect(describeState({ kind: "closed", reason: "remote_exited" }).label).toBe("Session ended");
  });

  it("surfaces the error message for failures", () => {
    const s = describeState({
      kind: "failed",
      error: { code: "auth_failed", message: "Nope", detail: null, retryable: false },
    });
    expect(s).toEqual({ label: "Nope", tone: "down" });
  });

  it("describes prompts", () => {
    const prompt = {
      host: "h",
      port: 22,
      keyType: "ssh-ed25519",
      fingerprint: "SHA256:x",
      changed: true,
      previous: ["SHA256:y"],
    };
    expect(describeState({ kind: "verifying_host_key", prompt }).tone).toBe("down");
    expect(
      describeState({ kind: "verifying_host_key", prompt: { ...prompt, changed: false } }).tone,
    ).toBe("warn");
    expect(
      describeState({
        kind: "prompting",
        prompt: { title: "Two-factor login", instructions: "", fields: [], canRemember: false },
      }).label,
    ).toBe("Two-factor login");
  });

  it("flags busy states", () => {
    expect(isBusy({ kind: "authenticating", method: "password" })).toBe(true);
    expect(isBusy({ kind: "ready" })).toBe(false);
  });
});
