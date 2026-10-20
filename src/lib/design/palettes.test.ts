import { describe, expect, it } from "vitest";
import { contrast, hexToOklch, matte, oklchToHex, TERMINAL_PALETTES } from "./palettes";

// The app themes' text colours (tokens.css), for palettes that use the theme's text.
const THEME_TEXT: Record<string, string> = {
  ember: oklchToHex(0.93, 0.01, 80),
  moss: oklchToHex(0.93, 0.015, 140),
  tide: oklchToHex(0.93, 0.01, 210),
  paper: oklchToHex(0.22, 0.012, 60),
};

describe("colour maths", () => {
  it("round-trips through OKLCH", () => {
    for (const hex of ["#e5675a", "#7aa6d6", "#1c1a18", "#f5efe4"]) {
      const [L, C, H] = hexToOklch(hex);
      expect(oklchToHex(L, C, H)).toBe(hex);
    }
  });

  it("matte keeps lightness and hue but lowers chroma", () => {
    const [L1, C1, H1] = hexToOklch("#e5675a");
    const [L2, C2, H2] = hexToOklch(matte("#e5675a", 0.7));
    expect(L2).toBeCloseTo(L1, 2);
    expect(H2).toBeCloseTo(H1, 0);
    expect(C2).toBeLessThan(C1 * 0.75);
  });

  it("measures WCAG contrast", () => {
    expect(contrast("#000000", "#ffffff")).toBeCloseTo(21, 0);
    expect(contrast("#777777", "#777777")).toBeCloseTo(1, 5);
  });
});

describe("terminal palettes", () => {
  for (const [id, p] of Object.entries(TERMINAL_PALETTES)) {
    it(`${p.name} stays readable on its background`, () => {
      const text = p.foreground ?? THEME_TEXT[id] ?? "#ffffff";
      expect(contrast(text, p.reference)).toBeGreaterThanOrEqual(7);
      for (const c of [
        p.ansi.red,
        p.ansi.green,
        p.ansi.yellow,
        p.ansi.blue,
        p.ansi.magenta,
        p.ansi.cyan,
      ]) {
        expect(contrast(c, p.reference), `${c} on ${p.reference}`).toBeGreaterThanOrEqual(3);
      }
    });
  }
});
