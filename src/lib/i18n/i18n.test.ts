import { describe, expect, it } from "vitest";
import { en, type MessageKey } from "./en";
import { format, LOCALES, resolveLocale } from "./i18n.svelte";

const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe("i18n catalogs", () => {
  for (const [code, { messages }] of Object.entries(LOCALES)) {
    it(`${code} has every message with the same placeholders`, () => {
      for (const key of Object.keys(en) as MessageKey[]) {
        const msg = messages[key];
        expect(msg, `${code}: ${key}`).toBeTruthy();
        expect(placeholders(msg), `${code}: ${key}`).toEqual(placeholders(en[key]));
      }
      expect(Object.keys(messages).sort()).toEqual(Object.keys(en).sort());
    });
  }

  it("fills placeholders and leaves unknown ones", () => {
    expect(format("Import {count} sessions", { count: 3 })).toBe("Import 3 sessions");
    expect(format("Hi {who}", {})).toBe("Hi {who}");
    expect(format("plain")).toBe("plain");
  });

  it("follows the system language when supported", () => {
    expect(resolveLocale("system", ["de-AT", "en-US"])).toBe("de");
    expect(resolveLocale("system", ["fr-FR", "en-GB"])).toBe("en");
    expect(resolveLocale("system", ["fr-FR"])).toBe("en");
    expect(resolveLocale("de", ["en-US"])).toBe("de");
  });
});
