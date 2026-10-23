import { beforeEach, describe, expect, it } from "vitest";
import { shell } from "./shell.state.svelte";

describe("shell state", () => {
  beforeEach(() => {
    shell.close();
  });

  it("toggles a drawer open and closed", () => {
    shell.toggle("settings");
    expect(shell.drawer).toBe("settings");
    shell.toggle("settings");
    expect(shell.drawer).toBeNull();
  });

  it("Esc closes the open drawer", () => {
    shell.toggle("settings");
    expect(shell.closeTop()).toBe(true);
    expect(shell.drawer).toBeNull();
    expect(shell.closeTop()).toBe(false);
  });
});
