import { describe, expect, it } from "vitest";
import { formatShortcut } from "./platform";

describe("formatShortcut", () => {
  it("keeps canonical form off macOS", () => {
    expect(formatShortcut("Ctrl+Shift+K", false)).toBe("Ctrl+Shift+K");
  });

  it("uses macOS glyphs", () => {
    expect(formatShortcut("Ctrl+Shift+K", true)).toBe("⌘⇧K");
    expect(formatShortcut("Ctrl+,", true)).toBe("⌘,");
  });
});
