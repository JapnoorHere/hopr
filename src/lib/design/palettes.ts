/**
 * Colour maths and the terminal's colours. Every theme (themes.ts) carries its own 16 ANSI
 * colours, so the terminal always matches the app.
 */
import type { ITheme } from "@xterm/xterm";

export type Ansi16 = Required<
  Pick<
    ITheme,
    | "black"
    | "red"
    | "green"
    | "yellow"
    | "blue"
    | "magenta"
    | "cyan"
    | "white"
    | "brightBlack"
    | "brightRed"
    | "brightGreen"
    | "brightYellow"
    | "brightBlue"
    | "brightMagenta"
    | "brightCyan"
    | "brightWhite"
  >
>;

export function ansi(c: string[]): Ansi16 {
  const [k, r, g, y, b, m, cy, w, bk, br, bg, by, bb, bm, bc, bw] = c as [
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
  ];
  return {
    black: k,
    red: r,
    green: g,
    yellow: y,
    blue: b,
    magenta: m,
    cyan: cy,
    white: w,
    brightBlack: bk,
    brightRed: br,
    brightGreen: bg,
    brightYellow: by,
    brightBlue: bb,
    brightMagenta: bm,
    brightCyan: bc,
    brightWhite: bw,
  };
}

/** Fully transparent, for hiding xterm's own cursor under the glide cursor. */
export const INVISIBLE = "#00000000";

let ctx: CanvasRenderingContext2D | null | undefined;

/** Resolve any CSS color (oklch, color-mix…) to #rrggbb[aa], which xterm's renderers need. */
export function resolveCssColor(css: string, alpha = 1): string {
  ctx ??= document.createElement("canvas").getContext("2d", { willReadFrequently: true });
  if (!ctx) return css;
  ctx.clearRect(0, 0, 1, 1);
  ctx.fillStyle = css.trim();
  ctx.fillRect(0, 0, 1, 1);
  const [r = 0, g = 0, b = 0] = ctx.getImageData(0, 0, 1, 1).data;
  const hex = (n: number) => n.toString(16).padStart(2, "0");
  const a = alpha < 1 ? hex(Math.round(alpha * 255)) : "";
  return `#${hex(r)}${hex(g)}${hex(b)}${a}`;
}

// ── Colour maths (OKLab/OKLCH, Björn Ottosson; WCAG 2 contrast) ──────────────────────

type Rgb = [number, number, number];

function hexToRgb(hex: string): Rgb {
  const h = hex.replace("#", "");
  const n = (i: number) => Number.parseInt(h.slice(i, i + 2), 16) / 255;
  return [n(0), n(2), n(4)];
}

function rgbToHex([r, g, b]: Rgb): string {
  const c = (v: number) =>
    Math.round(Math.min(1, Math.max(0, v)) * 255)
      .toString(16)
      .padStart(2, "0");
  return `#${c(r)}${c(g)}${c(b)}`;
}

const toLinear = (v: number) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
const toGamma = (v: number) => (v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055);

/** #rrggbb → [L 0..1, C, H degrees]. */
export function hexToOklch(hex: string): [number, number, number] {
  const [r, g, b] = hexToRgb(hex).map(toLinear) as Rgb;
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const A = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const B = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  const h = (Math.atan2(B, A) * 180) / Math.PI;
  return [L, Math.hypot(A, B), h < 0 ? h + 360 : h];
}

/** [L, C, H degrees] → #rrggbb (clipped to sRGB). */
export function oklchToHex(L: number, C: number, H: number): string {
  const a = C * Math.cos((H * Math.PI) / 180);
  const b = C * Math.sin((H * Math.PI) / 180);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return rgbToHex([
    toGamma(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    toGamma(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    toGamma(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
  ]);
}

/** The same hue with less chroma: a matte version of a colour. */
export function matte(hex: string, keep = 0.7): string {
  const [L, C, H] = hexToOklch(hex);
  return oklchToHex(L, C * keep, H);
}

/** WCAG 2 contrast ratio between two #rrggbb colours (1..21). */
export function contrast(a: string, b: string): number {
  const lum = (hex: string) => {
    const [r, g, bl] = hexToRgb(hex).map(toLinear) as Rgb;
    return 0.2126 * r + 0.7152 * g + 0.0722 * bl;
  };
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p) as [number, number];
  return (x + 0.05) / (y + 0.05);
}

// ── Terminal theme ────────────────────────────────────────────────────────────────

/** What the terminal needs from a theme, as #rrggbb. */
export interface TerminalColors {
  background: string;
  foreground: string;
  cursor: string;
  /** Faint text: scrollbar and inactive selection. */
  faint: string;
  ansi: Ansi16;
}

/** xterm theme from a theme's colours. Selection is a translucent wash of the cursor colour. */
export function terminalTheme(c: TerminalColors): ITheme {
  return {
    background: c.background,
    foreground: c.foreground,
    cursor: c.cursor,
    cursorAccent: c.background.slice(0, 7),
    selectionBackground: `${c.cursor}4d`,
    selectionInactiveBackground: `${c.faint}40`,
    scrollbarSliderBackground: `${c.faint}40`,
    scrollbarSliderHoverBackground: `${c.faint}66`,
    scrollbarSliderActiveBackground: `${c.faint}80`,
    ...c.ansi,
  };
}
