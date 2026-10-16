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

/** xterm theme from the live design tokens plus the theme's ANSI palette. */
export function terminalTheme(
  theme: ThemeId,
  root: HTMLElement = document.documentElement,
): ITheme {
  const css = getComputedStyle(root);
  const token = (name: string, alpha?: number) =>
    resolveCssColor(css.getPropertyValue(name), alpha);
  return {
    background: token("--bg"),
    foreground: token("--text"),
    cursor: token("--accent"),
    cursorAccent: token("--bg"),
    selectionBackground: token("--accent", 0.3),
    selectionInactiveBackground: token("--text-faint", 0.25),
    scrollbarSliderBackground: token("--text-faint", 0.25),
    scrollbarSliderHoverBackground: token("--text-faint", 0.4),
    scrollbarSliderActiveBackground: token("--text-faint", 0.5),
    ...ANSI_PALETTES[theme],
  };
}
