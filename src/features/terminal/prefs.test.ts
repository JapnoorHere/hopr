import { describe, expect, it } from "vitest";
import { DEFAULT_PREFS, decodeOsc52, parsePrefs } from "./prefs";

describe("terminal prefs", () => {
  it("fills defaults and clamps values", () => {
    expect(parsePrefs(null)).toEqual(DEFAULT_PREFS);
    const p = parsePrefs({
      fontSize: 99,
      lineHeight: 0.5,
      scrollback: 5,
      cursorStyle: "x",
      bell: "sound",
    });
    expect(p.fontSize).toBe(32);
    expect(p.lineHeight).toBe(1);
    expect(p.scrollback).toBe(1000);
    expect(p.cursorStyle).toBe("bar");
    expect(p.bell).toBe("sound");
  });

  it("decodes OSC 52 writes and refuses reads", () => {
    expect(decodeOsc52(`c;${btoa("héllo".replace("é", "e"))}`)).toBe("hello");
    expect(decodeOsc52("c;?")).toBeNull();
    expect(decodeOsc52("c;%%%")).toBeNull();
  });
});
