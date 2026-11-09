/**
 * Find-in-terminal highlight colours, worked out per theme so every theme gets the same
 * quality: matches are a soft wash of the theme's yellow, the current match a stronger wash
 * of a second hue with a bright outline. Fills are opaque #rrggbb (xterm needs that) and stay
 * close enough to the background that the text on them keeps at least 4.5:1 contrast.
 */
import { contrast, hexToOklch, oklchToHex, type TerminalColors } from "./palettes";

export interface SearchHighlight {
  matchBackground: string;
  matchBorder: string;
  matchOverviewRuler: string;
  activeMatchBackground: string;
  activeMatchBorder: string;
  activeMatchColorOverviewRuler: string;
}

/** Text on a highlight keeps at least this contrast (WCAG AA for body text). */
export const MIN_TEXT_ON_HIGHLIGHT = 4.5;
/** Outlines stand out from the background at least this much (WCAG non-text contrast). */
export const MIN_OUTLINE = 3;

/** Fallback hues when a theme's colour is too grey to carry one. */
const AMBER = 85;
const ORANGE = 45;

const hueOf = (hex: string, fallback: number) => {
  const [, C, H] = hexToOklch(hex);
  return C < 0.04 ? fallback : H;
};
const hueGap = (a: number, b: number) => {
  const d = Math.abs(a - b) % 360;
  return d > 180 ? 360 - d : d;
};

/**
 * A wash of hue `H`, up to `step` lightness away from the background, eased back towards it
 * until the text on top reads at `min` contrast. Returns the colour and the step it took.
 */
function wash(bg: string, fg: string, H: number, C: number, step: number, min: number) {
  const [bgL] = hexToOklch(bg);
  // Chroma eases with the step: a near-background wash with full colour would still dim text.
  const at = (s: number) =>
    oklchToHex(Math.min(1, Math.max(0, bgL + s)), C * Math.max(0.4, Math.abs(s / step)), H);
  let s = step;
  while (Math.abs(s) > 0.005 && contrast(at(s), fg) < min) s *= 0.92;
  return { hex: at(s), step: s };
}

/** A vivid version of hue `H` that stands out from the background by `min`. */
function outline(bg: string, H: number, C: number, dark: boolean, min: number): string {
  let L = dark ? 0.72 : 0.55;
  for (let i = 0; i < 100 && contrast(oklchToHex(L, C, H), bg) < min; i++) {
    L = dark ? Math.min(1, L + 0.01) : Math.max(0, L - 0.01);
  }
  return oklchToHex(L, C, H);
}

/** Highlight colours for a terminal's colours and the theme's accent. */
export function searchHighlight(c: TerminalColors, accent: string): SearchHighlight {
  const bg = c.background.slice(0, 7);
  const fg = c.foreground.slice(0, 7);
  const dark = hexToOklch(bg)[0] < 0.6;
  const matchH = hueOf(c.ansi.yellow, AMBER);
  // The current match uses the accent's hue unless it is too close to the matches' yellow.
  const accentH = hueOf(accent, ORANGE);
  const activeH = hueGap(accentH, matchH) >= 35 ? accentH : ORANGE;
  const dir = dark ? 1 : -1;

  // A theme whose own text is barely above 4.5:1 can't keep it on any wash: keep most of it.
  const min = Math.min(MIN_TEXT_ON_HIGHLIGHT, contrast(bg, fg) * 0.9);
  // The current match first; the other matches are a lighter wash of the room it left, so
  // the current one is always the stronger of the two.
  const active = wash(bg, fg, activeH, 0.13, 0.27 * dir, min);
  const match = wash(bg, fg, matchH, 0.08, active.step * 0.55, min);
  const activeMatchBackground = active.hex;
  const matchBackground = match.hex;
  const matchBorder = outline(bg, matchH, 0.13, dark, MIN_OUTLINE);
  // The current match's outline is the strongest mark on screen.
  const activeMatchBorder = outline(bg, activeH, 0.16, dark, 4.5);
  return {
    matchBackground,
    matchBorder,
    matchOverviewRuler: matchBorder,
    activeMatchBackground,
    activeMatchBorder,
    activeMatchColorOverviewRuler: activeMatchBorder,
  };
}
