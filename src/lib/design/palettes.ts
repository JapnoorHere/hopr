/**
 * Terminal colors. ANSI palettes are separate from UI themes (HOPR_PLAN §7.4) so users can
 * keep a familiar palette inside any UI theme. Each built-in theme ships a matching one.
 */
import type { ITheme } from "@xterm/xterm";
import type { ThemeId } from "./theme";

type Ansi16 = Required<
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

function ansi(c: string[]): Ansi16 {
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

export const ANSI_PALETTES: Record<ThemeId, Ansi16> = {
  ember: ansi([
    "#1c1a18",
    "#e5675a",
    "#a9c46a",
    "#f2b544",
    "#7aa6d6",
    "#c993c7",
    "#74c2b4",
    "#d9d2c5",
    "#6b635a",
    "#ff8a7a",
    "#c3df85",
    "#ffcf6b",
    "#9cc2ee",
    "#e0b0dd",
    "#93dccd",
    "#f5efe4",
  ]),
  moss: ansi([
    "#121a16",
    "#f07167",
    "#b6f36a",
    "#e9d968",
    "#6fb3d2",
    "#c39bd3",
    "#6fd3b8",
    "#cfe0d4",
    "#4f6157",
    "#ff9488",
    "#cfff8f",
    "#f6ea8a",
    "#94cde6",
    "#dab8e6",
    "#94ecd2",
    "#eef7f0",
  ]),
  tide: ansi([
    "#10191d",
    "#ef6f6c",
    "#8fd694",
    "#f0c96a",
    "#5fb0f0",
    "#b79cf0",
    "#4fd1c5",
    "#cfdde2",
    "#4b5d65",
    "#ff918e",
    "#aef0b2",
    "#ffdc8a",
    "#86c7ff",
    "#cfb9ff",
    "#7ce8de",
    "#f0f7f9",
  ]),
  paper: ansi([
    "#2b2622",
    "#c8331f",
    "#4f7a28",
    "#9a6b00",
    "#2d5f9a",
    "#8a3f86",
    "#1f7a72",
    "#d6cfc2",
    "#6d655c",
    "#e4572e",
    "#66973a",
    "#b8860b",
    "#3d79c2",
    "#a8559f",
    "#2b958b",
    "#f4f1ea",
  ]),
};

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

// ── Terminal palettes ─────────────────────────────────────────────────────────────

/** A terminal palette. Theme palettes take background and text from the app theme. */
export interface TerminalPalette {
  name: string;
  feel: string;
  /** Own background/foreground/cursor; absent = the app theme's. */
  background?: string;
  foreground?: string;
  cursor?: string;
  /** The background it is designed for (swatches and contrast checks). */
  reference: string;
  ansi: Ansi16;
}

const matteAnsi = (a: Ansi16): Ansi16 =>
  Object.fromEntries(Object.entries(a).map(([k, v]) => [k, matte(v, 0.68)])) as Ansi16;

export const PALETTE_IDS = [
  "theme",
  "ember",
  "moss",
  "tide",
  "paper",
  "rose-pine",
  "kanagawa",
  "catppuccin-mocha",
  "gruvbox-material",
] as const;
export type PaletteId = (typeof PALETTE_IDS)[number];

export const TERMINAL_PALETTES: Record<Exclude<PaletteId, "theme">, TerminalPalette> = {
  ember: {
    name: "Ember Matte",
    feel: "Warm charcoal",
    reference: "#141210",
    ansi: matteAnsi(ANSI_PALETTES.ember),
  },
  moss: {
    name: "Moss Matte",
    feel: "Deep olive",
    reference: "#0e1511",
    ansi: matteAnsi(ANSI_PALETTES.moss),
  },
  tide: {
    name: "Tide Matte",
    feel: "Slate blue",
    reference: "#0c1418",
    ansi: matteAnsi(ANSI_PALETTES.tide),
  },
  paper: {
    name: "Paper Matte",
    feel: "Warm off-white",
    reference: "#f3efe6",
    ansi: matteAnsi(ANSI_PALETTES.paper),
  },
  // Community palettes (MIT), with their published colours.
  "rose-pine": {
    name: "Rosé Pine",
    feel: "Dusky plum",
    background: "#191724",
    foreground: "#e0def4",
    cursor: "#ebbcba",
    reference: "#191724",
    ansi: ansi([
      "#26233a",
      "#eb6f92",
      "#31748f",
      "#f6c177",
      "#9ccfd8",
      "#c4a7e7",
      "#ebbcba",
      "#e0def4",
      "#6e6a86",
      "#eb6f92",
      "#31748f",
      "#f6c177",
      "#9ccfd8",
      "#c4a7e7",
      "#ebbcba",
      "#e0def4",
    ]),
  },
  kanagawa: {
    name: "Kanagawa",
    feel: "Ink and wave",
    background: "#1f1f28",
    foreground: "#dcd7ba",
    cursor: "#c8c093",
    reference: "#1f1f28",
    ansi: ansi([
      "#16161d",
      "#c34043",
      "#76946a",
      "#c0a36e",
      "#7e9cd8",
      "#957fb8",
      "#6a9589",
      "#c8c093",
      "#727169",
      "#e82424",
      "#98bb6c",
      "#e6c384",
      "#7fb4ca",
      "#938aa9",
      "#7aa89f",
      "#dcd7ba",
    ]),
  },
  "catppuccin-mocha": {
    name: "Catppuccin Mocha",
    feel: "Soft pastel",
    background: "#1e1e2e",
    foreground: "#cdd6f4",
    cursor: "#f5e0dc",
    reference: "#1e1e2e",
    ansi: ansi([
      "#45475a",
      "#f38ba8",
      "#a6e3a1",
      "#f9e2af",
      "#89b4fa",
      "#f5c2e7",
      "#94e2d5",
      "#bac2de",
      "#585b70",
      "#f38ba8",
      "#a6e3a1",
      "#f9e2af",
      "#89b4fa",
      "#f5c2e7",
      "#94e2d5",
      "#a6adc8",
    ]),
  },
  "gruvbox-material": {
    name: "Gruvbox Material",
    feel: "Retro, soft brown",
    background: "#282828",
    foreground: "#d4be98",
    cursor: "#d4be98",
    reference: "#282828",
    ansi: ansi([
      "#32302f",
      "#ea6962",
      "#a9b665",
      "#d8a657",
      "#7daea3",
      "#d3869b",
      "#89b482",
      "#d4be98",
      "#928374",
      "#ea6962",
      "#a9b665",
      "#d8a657",
      "#7daea3",
      "#d3869b",
      "#89b482",
      "#ddc7a1",
    ]),
  },
};

/** The palette a terminal uses: "theme" means the app theme's matte palette. */
export function resolvePalette(theme: ThemeId, id: PaletteId): TerminalPalette {
  return TERMINAL_PALETTES[id === "theme" ? theme : id];
}

/**
 * xterm theme: the palette's colours, with background, text and cursor from the app theme
 * unless the palette brings its own. Selection is a translucent wash of the cursor colour.
 */
export function terminalTheme(
  theme: ThemeId,
  root: HTMLElement = document.documentElement,
  palette: PaletteId = "theme",
): ITheme {
  const css = getComputedStyle(root);
  const token = (name: string, alpha?: number) =>
    resolveCssColor(css.getPropertyValue(name), alpha);
  const p = resolvePalette(theme, palette);
  const bg = p.background ?? token("--bg");
  const cursor = p.cursor ?? token("--accent");
  return {
    background: bg,
    foreground: p.foreground ?? token("--text"),
    cursor,
    cursorAccent: bg,
    selectionBackground: `${cursor.slice(0, 7)}4d`,
    selectionInactiveBackground: token("--text-faint", 0.25),
    scrollbarSliderBackground: token("--text-faint", 0.25),
    scrollbarSliderHoverBackground: token("--text-faint", 0.4),
    scrollbarSliderActiveBackground: token("--text-faint", 0.5),
    ...p.ansi,
  };
}
