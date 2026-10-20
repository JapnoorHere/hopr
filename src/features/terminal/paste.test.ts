import { describe, expect, it } from "vitest";
import { checkPaste, LONG_PASTE_CHARS, smartKey } from "./paste";

describe("checkPaste", () => {
  it("lets single-line pastes through", () => {
    expect(checkPaste("ls -la")).toEqual({ confirm: false, lines: 1 });
  });

  it("confirms anything with a newline, counting lines", () => {
    expect(checkPaste("rm -rf build\n")).toEqual({ confirm: true, lines: 1 });
    expect(checkPaste("a\r\nb\nc")).toEqual({ confirm: true, lines: 3 });
  });

  it("confirms very long single lines", () => {
    expect(checkPaste("x".repeat(LONG_PASTE_CHARS + 1)).confirm).toBe(true);
  });
});

describe("smartKey", () => {
  const key = (k: string, mods: Partial<{ shiftKey: boolean; altKey: boolean }> = {}) => ({
    type: "keydown",
    key: k,
    ctrlKey: true,
    shiftKey: false,
    altKey: false,
    metaKey: false,
    ...mods,
  });

  it("copies with a selection, interrupts without one", () => {
    expect(smartKey(key("c"), true, true)).toBe("copy");
    expect(smartKey(key("c"), true, false)).toBeNull();
    expect(smartKey(key("v"), true, false)).toBe("paste");
  });

  it("stays out of the way when off or with other modifiers", () => {
    expect(smartKey(key("c"), false, true)).toBeNull();
    expect(smartKey(key("v"), false, false)).toBeNull();
    expect(smartKey(key("C", { shiftKey: true }), true, true)).toBeNull();
    expect(smartKey(key("c", { altKey: true }), true, true)).toBeNull();
    expect(smartKey({ ...key("c"), type: "keyup" }, true, true)).toBeNull();
  });
});
