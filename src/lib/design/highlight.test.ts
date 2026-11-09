import { describe, expect, it } from "vitest";
import { MIN_OUTLINE, MIN_TEXT_ON_HIGHLIGHT, searchHighlight } from "./highlight";
import { contrast } from "./palettes";
import { THEMES, terminalColors } from "./themes";

const HEX = /^#[0-9a-f]{6}$/;

describe("search highlight", () => {
  for (const theme of THEMES) {
    for (const lift of [0, 60]) {
      const c = terminalColors(theme, lift);
      const h = searchHighlight(c, theme.accent);
      const bg = c.background.slice(0, 7);

      it(`${theme.id} (lift ${lift}): every colour is opaque #rrggbb`, () => {
        for (const v of Object.values(h)) expect(v).toMatch(HEX);
      });

      it(`${theme.id} (lift ${lift}): text on a match stays readable`, () => {
        // AA, or most of the theme's own contrast when the theme itself is near AA.
        const min = Math.min(MIN_TEXT_ON_HIGHLIGHT, contrast(bg, c.foreground) * 0.9);
        expect(contrast(h.matchBackground, c.foreground)).toBeGreaterThanOrEqual(min);
        expect(contrast(h.activeMatchBackground, c.foreground)).toBeGreaterThanOrEqual(min);
      });

      it(`${theme.id} (lift ${lift}): matches stand out from the page and from each other`, () => {
        expect(contrast(h.matchBorder, bg)).toBeGreaterThanOrEqual(MIN_OUTLINE);
        expect(contrast(h.activeMatchBorder, bg)).toBeGreaterThanOrEqual(4.5);
        expect(h.matchBackground).not.toBe(bg);
        expect(h.activeMatchBackground).not.toBe(h.matchBackground);
        // The current match is the stronger wash.
        expect(contrast(h.activeMatchBackground, bg)).toBeGreaterThan(
          contrast(h.matchBackground, bg),
        );
      });
    }
  }
});
