import { describe, expect, it } from "vitest";
import { contrast, hexToOklch } from "./palettes";
import {
  customFrom,
  isDark,
  normalizeHex,
  parseCustom,
  THEMES,
  TOKENS,
  terminalColors,
  themeSpec,
  themeStyle,
  themeTokens,
} from "./themes";

describe("built-in themes", () => {
  for (const th of THEMES) {
    it(`${th.name} reads well`, () => {
      expect(contrast(th.text, th.bg)).toBeGreaterThanOrEqual(4.5);
      expect(themeTokens(th)["accent-fg"]).toMatch(/^oklch\(/);
    });
  }

  it("keeps the original hand-tuned tokens", () => {
    const ember = themeTokens(themeSpec("ember", null));
    expect(ember.bg).toBe("oklch(0.165 0.006 60)");
    expect(ember["surface-2"]).toBe("oklch(0.215 0.008 60)");
  });

  it("splits into dark and light by background", () => {
    expect(isDark(themeSpec("nord", null))).toBe(true);
    expect(isDark(themeSpec("solarized-light", null))).toBe(false);
    expect(themeTokens(themeSpec("catppuccin-latte", null)).scheme).toBe("light");
  });
});

describe("computed tokens", () => {
  it("defines every token", () => {
    const tokens = themeTokens(themeSpec("tokyo-night", null));
    for (const k of TOKENS) expect(tokens[k], k).toBeTruthy();
    expect(themeStyle(themeSpec("tokyo-night", null))).toContain("--surface-2: oklch(");
  });

  it("steps dark surfaces up and light surfaces down", () => {
    const L = (css: string) => Number(css.slice(6).split(" ")[0]);
    const dark = themeTokens(themeSpec("dracula", null));
    expect(L(dark["surface-2"])).toBeGreaterThan(L(dark.bg));
    const light = themeTokens(themeSpec("rose-pine-dawn", null));
    expect(L(light["surface-2"])).toBeLessThan(L(light.bg));
  });

  it("takes status colours from the terminal palette", () => {
    const spec = themeSpec("gruvbox-material", null);
    const [L] = hexToOklch(spec.ansi.red);
    expect(themeTokens(spec).danger.startsWith(`oklch(${Number(L.toFixed(4))} `)).toBe(true);
  });
});

describe("terminal colours", () => {
  it("use the theme's background, text and accent", () => {
    const spec = themeSpec("nord", null);
    const c = terminalColors(spec);
    expect(c.background).toBe("#2e3440");
    expect(c.foreground).toBe("#d8dee9");
    expect(c.cursor).toBe("#88c0d0");
    expect(c.ansi).toBe(spec.ansi);
    expect(c.faint).toMatch(/^#[0-9a-f]{6}$/);
  });
});

describe("custom themes", () => {
  it("start from a built-in theme and apply", () => {
    const custom = { ...customFrom("kanagawa"), accent: "#ff0080" };
    const spec = themeSpec("custom", custom);
    expect(spec.id).toBe("custom");
    expect(terminalColors(spec).cursor).toBe("#ff0080");
  });

  it("fall back to Ember when the custom colours are missing", () => {
    expect(themeSpec("custom", null).id).toBe("ember");
  });

  it("parse only complete, valid colours", () => {
    const good = customFrom("dracula");
    expect(parseCustom(JSON.parse(JSON.stringify(good)))).toEqual(good);
    expect(parseCustom({ ...good, text: "#12" })).toBeNull();
    expect(parseCustom({ ...good, ansi: { ...good.ansi, red: undefined } })).toBeNull();
    expect(parseCustom("nope")).toBeNull();
    expect(parseCustom({ ...good, bg: "#ABCDEF" })?.bg).toBe("#abcdef");
  });

  it("normalise typed hex", () => {
    expect(normalizeHex("ABC123")).toBe("#abc123");
    expect(normalizeHex(" #abc123 ")).toBe("#abc123");
    expect(normalizeHex("#abc")).toBeNull();
  });
});
