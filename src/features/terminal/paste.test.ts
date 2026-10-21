import { describe, expect, it } from "vitest";
import { checkPaste, LONG_PASTE_CHARS } from "./paste";

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
