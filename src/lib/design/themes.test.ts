import { describe, expect, it } from "vitest";
import { contrast, hexToOklch, oklchToHex } from "./palettes";
import {
  customFrom,
  isDark,
  normalizeHex,
  parseCustom,
  THEMES,
  TOKENS,
  terminalBackground,
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

describe("terminal background", () => {
  it("is the theme's own by default", () => {
    expect(terminalBackground(themeSpec("nord", null))).toBe("#2e3440");
  });

  it("lifts a dark background and deepens a light one", () => {
    const dark = themeSpec("midnight", null);
    expect(hexToOklch(terminalBackground(dark, { lift: 100, opacity: 100 }))[0]).toBeGreaterThan(
      0.12,
    );
    const light = themeSpec("paper", null);
    expect(hexToOklch(terminalBackground(light, { lift: 100, opacity: 100 }))[0]).toBeLessThan(
      hexToOklch(light.bg)[0],
    );
  });

  it("adds alpha when see-through", () => {
    expect(terminalBackground(themeSpec("nord", null), { lift: 0, opacity: 50 })).toBe("#2e344080");
  });
});

/** "oklch(L C H)" or "oklch(L C H / a)" as #rrggbb (alpha ignored). */
function hexOf(token: string): string {
  const [L, C, H] = token.slice(6, -1).split("/")[0]!.trim().split(/\s+/).map(Number) as [
    number,
    number,
    number,
  ];
  return oklchToHex(L, C, H);
}

describe("every theme is readable", () => {
  for (const th of THEMES) {
    it(`${th.name}: text, muted, faint, status and accent stand out`, () => {
      const t = themeTokens(th);
      const panel = hexOf(t["surface-2"]);
      const on = (k: keyof typeof t) => contrast(hexOf(t[k]), panel);
      expect(on("text"), "text").toBeGreaterThanOrEqual(7);
      expect(on("text-muted"), "muted").toBeGreaterThanOrEqual(4.5);
      expect(on("text-faint"), "faint").toBeGreaterThanOrEqual(3.5);
      for (const k of ["success", "warning", "danger"] as const) {
        expect(on(k), k).toBeGreaterThanOrEqual(3);
      }
      expect(contrast(hexOf(t.accent), hexOf(t.bg)), "accent").toBeGreaterThanOrEqual(3);
      expect(
        contrast(hexOf(t.accent), hexOf(t["accent-fg"])),
        "text on accent",
      ).toBeGreaterThanOrEqual(4.5);
    });
  }
});
