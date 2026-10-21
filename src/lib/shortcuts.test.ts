import { describe, expect, it } from "vitest";
import { chordOf, matches, parseChord, Registry } from "./shortcuts";

const key = (init: KeyboardEventInit) => new KeyboardEvent("keydown", init);

describe("shortcuts", () => {
  it("parses chords", () => {
    expect(parseChord("Ctrl+Shift+P")).toEqual({ ctrl: true, shift: true, alt: false, key: "p" });
    expect(parseChord("Ctrl++")).toEqual({ ctrl: true, shift: false, alt: false, key: "+" });
    expect(parseChord("Ctrl+Shift+`").key).toBe("backquote");
  });

  it("matches physical keys even with Shift held", () => {
    const e = key({ key: "!", code: "Digit1", ctrlKey: true, shiftKey: true });
    expect(matches(e, "Ctrl+Shift+1", false)).toBe(true);
    expect(matches(e, "Ctrl+1", false)).toBe(false);
    const p = key({ key: "P", code: "KeyP", ctrlKey: true, shiftKey: true });
    expect(matches(p, "Ctrl+Shift+P", false)).toBe(true);
    const mac = key({ key: "p", code: "KeyP", metaKey: true, shiftKey: true });
    expect(matches(mac, "Ctrl+Shift+P", true)).toBe(true);
  });

  it("dispatches, hides inactive commands and reports conflicts", () => {
    const reg = new Registry();
    let ran = 0;
    const off = reg.register([
      { id: "t.a", title: "A", group: "T", keys: ["Ctrl+Shift+Y"], run: () => ran++ },
      {
        id: "t.b",
        title: "B",
        group: "T",
        keys: ["Ctrl+Shift+Y"],
        run: () => {},
        when: () => false,
      },
    ]);
    reg.find(key({ key: "Y", code: "KeyY", ctrlKey: true, shiftKey: true }))?.run();
    expect(ran).toBe(1);
    expect(reg.all().map((c) => c.id)).toEqual(["t.a"]);
    expect(reg.conflicts().map(([, ids]) => ids)).toEqual([["t.a", "t.b"]]);
    off();
    expect(reg.all()).toEqual([]);
  });

  it("rebinds commands and persists overrides", () => {
    const store = new Map<string, string>();
    const storage = {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
    } as unknown as Storage;
    const reg = new Registry(storage);
    let ran = 0;
    reg.register([{ id: "x", title: "X", group: "T", keys: ["Ctrl+Shift+Y"], run: () => ran++ }]);
    reg.setKeys("x", ["Ctrl+Alt+J"]);
    expect(
      reg.find(key({ key: "Y", code: "KeyY", ctrlKey: true, shiftKey: true })),
    ).toBeUndefined();
    reg.find(key({ key: "j", code: "KeyJ", ctrlKey: true, altKey: true }))?.run();
    expect(ran).toBe(1);
    const again = new Registry(storage);
    expect(again.overrides()).toEqual({ x: ["Ctrl+Alt+J"] });
    again.setKeys("x", null);
    expect(again.isCustom("x")).toBe(false);
  });

  it("records chords from key events", () => {
    expect(chordOf(key({ key: "K", code: "KeyK", ctrlKey: true, shiftKey: true }), false)).toBe(
      "Ctrl+Shift+K",
    );
    expect(chordOf(key({ key: "Control", ctrlKey: true }), false)).toBeNull();
    expect(
      chordOf(key({ key: "ArrowLeft", code: "ArrowLeft", ctrlKey: true, altKey: true }), false),
    ).toBe("Ctrl+Alt+ArrowLeft");
  });
});
