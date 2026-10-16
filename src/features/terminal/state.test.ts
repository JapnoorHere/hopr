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
    expect(describeState({ kind: "reconnecting", attempt: 2 }).tone).toBe("warn");
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

  it("flags busy states", () => {
    expect(isBusy({ kind: "authenticating", method: "password" })).toBe(true);
    expect(isBusy({ kind: "ready" })).toBe(false);
  });
});
