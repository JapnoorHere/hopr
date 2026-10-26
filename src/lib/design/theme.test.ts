import { describe, expect, it } from "vitest";
import {
  applyAppearance,
  DEFAULT_APPEARANCE,
  loadAppearance,
  parseAppearance,
  resolveMotion,
  saveAppearance,
} from "./theme";
import { customFrom } from "./themes";

function memoryStorage(): Storage {
  const m = new Map<string, string>();
  return {
    get length() {
      return m.size;
    },
    clear: () => m.clear(),
    getItem: (k) => m.get(k) ?? null,
    key: (i) => [...m.keys()][i] ?? null,
    removeItem: (k) => void m.delete(k),
    setItem: (k, v) => void m.set(k, v),
  };
}

describe("parseAppearance", () => {
  it("accepts valid input", () => {
    expect(
      parseAppearance({ theme: "moss", density: "compact", motion: "off", backdrop: "mica" }),
    ).toEqual(parseAppearance({ theme: "moss", density: "compact", motion: "off" }));
    expect(parseAppearance({ theme: "moss", density: "compact", motion: "off" })).toEqual({
      theme: "moss",
      custom: null,
      density: "compact",
      motion: "off",
    });
  });

  it("falls back per field on invalid input", () => {
    expect(parseAppearance({ theme: "neon", density: "spacious", motion: "wild" })).toEqual({
      theme: "ember",
      custom: null,
      density: "spacious",
      motion: "system",
    });
    expect(parseAppearance(null)).toEqual(DEFAULT_APPEARANCE);
    expect(parseAppearance({ density: "toString" })).toEqual(DEFAULT_APPEARANCE);
  });

  it("keeps a custom theme only with valid colours", () => {
    const custom = customFrom("nord");
    expect(parseAppearance({ theme: "custom", custom }).theme).toBe("custom");
    expect(parseAppearance({ theme: "custom", custom: { ...custom, bg: "red" } })).toMatchObject({
      theme: "ember",
      custom: null,
    });
  });
});

describe("storage round trip", () => {
  it("saves and loads", () => {
    const s = memoryStorage();
    const a = {
      theme: "paper",
      custom: null,
      density: "spacious",
      motion: "reduced",
    } as const;
    saveAppearance(a, s);
    expect(loadAppearance(s)).toEqual(a);
  });

  it("survives corrupt or missing storage", () => {
    const s = memoryStorage();
    s.setItem("hopr.appearance", "{not json");
    expect(loadAppearance(s)).toEqual(DEFAULT_APPEARANCE);
    expect(loadAppearance(null)).toEqual(DEFAULT_APPEARANCE);
  });
});

describe("applyAppearance", () => {
  it("sets theme attribute and density variable", () => {
    const el = document.createElement("div");
    applyAppearance({ theme: "tide", custom: null, density: "compact", motion: "off" }, el);
    expect(el.dataset.theme).toBe("tide");
    expect(el.style.getPropertyValue("--bg")).toBe("oklch(0.16 0.015 225)");
    expect(el.style.getPropertyValue("--density")).toBe("0.8");
    expect(el.dataset.motion).toBe("off");
  });
});

describe("resolveMotion", () => {
  it("follows the system only when asked to", () => {
    expect(resolveMotion("system", true)).toBe("reduced");
    expect(resolveMotion("system", false)).toBe("full");
    expect(resolveMotion("full", true)).toBe("full");
    expect(resolveMotion("off", false)).toBe("off");
  });
});
