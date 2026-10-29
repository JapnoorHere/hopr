import { afterEach, describe, expect, it } from "vitest";
import {
  readJson,
  readText,
  removeKey,
  scoped,
  setStorageScope,
  writeJson,
  writeText,
} from "./storage";

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

const broken = {
  getItem: () => {
    throw new Error("blocked");
  },
  setItem: () => {
    throw new Error("quota");
  },
  removeItem: () => {
    throw new Error("blocked");
  },
} as unknown as Storage;

afterEach(() => setStorageScope("default"));

describe("storage", () => {
  it("round-trips text and JSON", () => {
    const s = memoryStorage();
    writeText("a", "1", s);
    writeJson("b", { x: [1, 2] }, s);
    expect(readText("a", s)).toBe("1");
    expect(readJson("b", (r) => r, null, s)).toEqual({ x: [1, 2] });
    removeKey("a", s);
    expect(readText("a", s)).toBeNull();
  });

  it("falls back on missing, corrupt or rejected values", () => {
    const s = memoryStorage();
    s.setItem("bad", "{nope");
    s.setItem("num", "5");
    const asText = (r: unknown) => {
      if (typeof r !== "string") throw new Error("not text");
      return r;
    };
    expect(readJson("missing", asText, "fb", s)).toBe("fb");
    expect(readJson("bad", asText, "fb", s)).toBe("fb");
    expect(readJson("num", asText, "fb", s)).toBe("fb");
  });

  it("never throws when storage is missing or blocked", () => {
    expect(readText("a", null)).toBeNull();
    expect(readText("a", broken)).toBeNull();
    expect(() => writeText("a", "1", broken)).not.toThrow();
    expect(() => removeKey("a", broken)).not.toThrow();
  });

  it("scopes keys to non-default profiles only", () => {
    expect(scoped("k")).toBe("k");
    setStorageScope("p1");
    expect(scoped("k")).toBe("k@p1");
    setStorageScope("default");
    expect(scoped("k")).toBe("k");
  });
});
