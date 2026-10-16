import { describe, expect, it } from "vitest";
import {
  applyAppearance,
  DEFAULT_APPEARANCE,
  loadAppearance,
  parseAppearance,
  saveAppearance,
} from "./theme";

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
    expect(parseAppearance({ theme: "moss", density: "compact" })).toEqual({
      theme: "moss",
      density: "compact",
    });
  });

  it("falls back per field on invalid input", () => {
    expect(parseAppearance({ theme: "neon", density: "spacious" })).toEqual({
      theme: "ember",
      density: "spacious",
    });
    expect(parseAppearance(null)).toEqual(DEFAULT_APPEARANCE);
    expect(parseAppearance({ density: "toString" })).toEqual(DEFAULT_APPEARANCE);
  });
});

describe("storage round trip", () => {
  it("saves and loads", () => {
    const s = memoryStorage();
    saveAppearance({ theme: "paper", density: "spacious" }, s);
    expect(loadAppearance(s)).toEqual({ theme: "paper", density: "spacious" });
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
    applyAppearance({ theme: "tide", density: "compact" }, el);
    expect(el.dataset.theme).toBe("tide");
    expect(el.style.getPropertyValue("--density")).toBe("0.8");
  });
});
