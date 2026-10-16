import { beforeEach, describe, expect, it } from "vitest";
import { shell } from "./shell.state.svelte";

describe("shell state", () => {
  beforeEach(() => {
    shell.close();
    shell.pinned = false;
  });

  it("toggles a drawer open and closed", () => {
    shell.toggle("settings");
    expect(shell.drawer).toBe("settings");
    shell.toggle("settings");
    expect(shell.drawer).toBeNull();
  });

  it("Esc closes a floating drawer", () => {
    shell.toggle("settings");
    expect(shell.closeTop()).toBe(true);
    expect(shell.drawer).toBeNull();
    expect(shell.closeTop()).toBe(false);
  });

  it("Esc leaves a pinned drawer open", () => {
    shell.toggle("settings");
    shell.togglePin();
    expect(shell.closeTop()).toBe(false);
    expect(shell.drawer).toBe("settings");
  });
});
