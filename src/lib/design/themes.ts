/**
 * Themes: one spec colours the whole app and its terminals. A spec names a background, text,
 * accent and the 16 ANSI colours; every other token (surfaces, borders, muted text, status
 * colours) is worked out from those in OKLCH. Hand-tuned themes can pin exact tokens.
 */
import { type Ansi16, contrast, hexToOklch, oklchToHex, type TerminalColors } from "./palettes";
import { BUILT_IN } from "./themes.data";

export const TOKENS = [
  "bg",
  "surface-1",
  "surface-2",
  "surface-3",
  "surface-4",
  "border",
  "border-strong",
  "text",
  "text-muted",
  "text-faint",
  "accent",
  "accent-fg",
  "success",
  "warning",
  "danger",
  "scheme",
] as const;
export type Token = (typeof TOKENS)[number];
export type Tokens = Record<Token, string>;

export interface ThemeSpec {
  id: string;
  name: string;
  feel: string;
  /** #rrggbb */
  bg: string;
  text: string;
  accent: string;
  ansi: Ansi16;
  /** Exact tokens that win over the computed ones (the original hand-tuned themes). */
  tokens?: Partial<Tokens>;
}

/** The part of a theme people can edit: a custom theme. */
export interface CustomTheme {
  bg: string;
  text: string;
  accent: string;
  ansi: Ansi16;
}

export type ThemeId = (typeof BUILT_IN)[number]["id"];
export const THEMES: readonly (ThemeSpec & { id: ThemeId })[] = BUILT_IN;
export const THEME_IDS = BUILT_IN.map((t) => t.id) as ThemeId[];

export function isThemeId(v: unknown): v is ThemeId {
  return THEME_IDS.some((id) => id === v);
}

export function builtIn(id: ThemeId): ThemeSpec {
  return THEMES.find((t) => t.id === id) ?? (THEMES[0] as ThemeSpec);
}

/** A custom theme starting from a built-in one. */
export function customFrom(id: ThemeId): CustomTheme {
  const t = builtIn(id);
  return { bg: t.bg, text: t.text, accent: t.accent, ansi: { ...t.ansi } };
}

export function customSpec(c: CustomTheme): ThemeSpec {
  return { id: "custom", name: "Custom", feel: "Your own colours", ...c };
}

/** The theme in use: a built-in id, or "custom" with its colours. */
export function themeSpec(theme: string, custom: CustomTheme | null): ThemeSpec {
  if (theme === "custom" && custom) return customSpec(custom);
  return isThemeId(theme) ? builtIn(theme) : (THEMES[0] as ThemeSpec);
}

export function isDark(spec: ThemeSpec): boolean {
  return hexToOklch(spec.bg)[0] < 0.6;
}

const n = (v: number) => Number(v.toFixed(4));
const lch = (L: number, C: number, H: number, alpha?: number) =>
  `oklch(${n(L)} ${n(C)} ${n(H)}${alpha === undefined ? "" : ` / ${alpha}`})`;
const css = (hex: string) => lch(...hexToOklch(hex));

/** Minimum contrast against the panels text sits on (WCAG 2 ratios). */
const MIN = { text: 7, muted: 6, faint: 4.5, status: 3.5, accent: 3, onAccent: 4.5 } as const;
/** How far each panel layer must stand out from the page (dark: lighter, light: deeper). */
const PANEL_STEP = [1.1, 1.2, 1.32, 1.45] as const;

/**
 * Move a colour's lightness away from `against` (towards light on dark themes, towards dark on
 * light ones) just far enough to reach `min` contrast. Hue and chroma stay, so the theme's feel
 * does too; colours that already read well don't move.
 */
function readable(L: number, C: number, H: number, against: string, min: number, lighter: boolean) {
  let l = L;
  for (let i = 0; i < 200 && contrast(oklchToHex(l, C, H), against) < min; i++) {
    l = lighter ? Math.min(1, l + 0.005) : Math.max(0, l - 0.005);
    if (l === 0 || l === 1) break;
  }
  return l;
}

/** Every design token for a theme, as CSS colours. */
export function themeTokens(spec: ThemeSpec): Tokens {
  const [L, C, H] = hexToOklch(spec.bg);
  const [tL0, tC, tH] = hexToOklch(spec.text);
  const [aL0, aC, aH] = hexToOklch(spec.accent);
  const dark = isDark(spec);
  const clampL = (v: number) => Math.min(1, Math.max(0, v));
  // Panels step away from the page by at least a visible amount: on near-black a fixed
  // lightness step is invisible, so each layer is pushed until it reaches its contrast step.
  const surfaceL = dark
    ? [1, 2, 3, 4].map((i) =>
        readable(clampL(L + 0.025 * i), C, H, spec.bg, PANEL_STEP[i - 1] as number, true),
      )
    : [-0.025, -0.05, 0.015, 0.03].map((d) => clampL(L + d));
  const surface = surfaceL.map((l) => lch(l, C, H));
  // Text is checked against the panel it usually sits on (cards, menus), not just the page.
  const panel = oklchToHex(surfaceL[1] as number, C, H);
  const page = spec.bg;
  const legible = (l: number, c: number, h: number, min: number, against = panel) =>
    lch(readable(l, c, h, against, min, dark), c, h);
  const tL = readable(tL0, tC, tH, panel, MIN.text, dark);
  const status = (hex: string) => {
    const [l, c, h] = hexToOklch(hex);
    return legible(l, c, h, MIN.status);
  };
  // The accent shows as text and as a fill; it must stand out from the page and carry text.
  let aL = readable(aL0, aC, aH, page, MIN.accent, dark);
  const ink = { l: 0.2, c: aC * 0.15 };
  const paperInk = { l: 0.98, c: 0.01 };
  const onAccent = (l: number) => {
    const a = oklchToHex(l, aC, aH);
    return contrast(a, oklchToHex(ink.l, ink.c, aH)) >=
      contrast(a, oklchToHex(paperInk.l, paperInk.c, aH))
      ? ink
      : paperInk;
  };
  let fgInk = onAccent(aL);
  // Still weak: move the accent away from its text colour until the label reads.
  for (
    let i = 0;
    i < 100 && contrast(oklchToHex(aL, aC, aH), oklchToHex(fgInk.l, fgInk.c, aH)) < MIN.onAccent;
    i++
  ) {
    aL = fgInk === ink ? Math.min(1, aL + 0.005) : Math.max(0, aL - 0.005);
  }
  fgInk = onAccent(aL);
  const fg = lch(fgInk.l, fgInk.c, aH);
  const computed: Tokens = {
    bg: css(spec.bg),
    "surface-1": surface[0] as string,
    "surface-2": surface[1] as string,
    "surface-3": surface[2] as string,
    "surface-4": surface[3] as string,
    border: dark ? "oklch(1 0 0 / 0.1)" : lch(0.2, 0.02, H, 0.13),
    "border-strong": dark ? "oklch(1 0 0 / 0.18)" : lch(0.2, 0.02, H, 0.22),
    text: lch(tL, tC, tH),
    "text-muted": legible(L + (tL - L) * 0.75, tC, tH, MIN.muted),
    "text-faint": legible(L + (tL - L) * 0.55, tC, tH, MIN.faint),
    accent: lch(aL, aC, aH),
    "accent-fg": fg,
    success: status(spec.ansi.green),
    warning: status(spec.ansi.yellow),
    danger: status(spec.ansi.red),
    scheme: dark ? "dark" : "light",
  };
  return { ...computed, ...spec.tokens };
}

/** Inline CSS variables for a theme: the window root, or a preview card. */
export function themeStyle(spec: ThemeSpec): string {
  const t = themeTokens(spec);
  return TOKENS.map((k) => `--${k}: ${t[k]}`).join("; ");
}

/**
 * The terminal's background as #rrggbb: the theme's, lifted by Settings > Terminal >
 * Background brightness (0..100: lighter on a dark theme, a touch deeper on a light one).
 */
export function terminalBackground(spec: ThemeSpec, lift = 0): string {
  if (!lift) return spec.bg;
  const [L, C, H] = hexToOklch(spec.bg);
  // Blend the lightness toward a soft charcoal (dark themes) or a deeper paper (light ones).
  const target = isDark(spec) ? Math.max(L, 0.4) : Math.min(L, 0.82);
  return oklchToHex(L + (target - L) * (lift / 100), C, H);
}

/** The terminal's colours for a theme. */
export function terminalColors(spec: ThemeSpec, lift = 0): TerminalColors {
  const [L] = hexToOklch(spec.bg);
  const [tL0, tC, tH] = hexToOklch(spec.text);
  // Terminal text gets the same floor as the app's text, against the terminal background.
  const tL = readable(tL0, tC, tH, spec.bg, MIN.text, isDark(spec));
  return {
    background: terminalBackground(spec, lift),
    foreground: tL === tL0 ? spec.text : oklchToHex(tL, tC, tH),
    cursor: spec.accent,
    faint: oklchToHex(
      readable(L + (tL - L) * 0.55, tC, tH, spec.bg, MIN.faint, isDark(spec)),
      tC,
      tH,
    ),
    ansi: spec.ansi,
  };
}

const HEX = /^#[0-9a-f]{6}$/i;

/** A typed colour ("ABC123", "#abc123") as #rrggbb, or null when it isn't one. */
export function normalizeHex(v: string): string | null {
  const h = v.trim().replace(/^#?/, "#").toLowerCase();
  return HEX.test(h) ? h : null;
}
export const ANSI_KEYS = [
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
  "brightBlack",
  "brightRed",
  "brightGreen",
  "brightYellow",
  "brightBlue",
  "brightMagenta",
  "brightCyan",
  "brightWhite",
] as const satisfies readonly (keyof Ansi16)[];

/** Validate an untrusted custom theme (storage, pasted JSON). Null when it isn't one. */
export function parseCustom(raw: unknown): CustomTheme | null {
  if (typeof raw !== "object" || raw === null) return null;
  const r = raw as Record<string, unknown>;
  const a = r.ansi;
  const hex = (v: unknown) => (typeof v === "string" && HEX.test(v) ? v.toLowerCase() : null);
  const bg = hex(r.bg);
  const text = hex(r.text);
  const accent = hex(r.accent);
  if (!bg || !text || !accent || typeof a !== "object" || a === null) return null;
  const out: Partial<Ansi16> = {};
  for (const k of ANSI_KEYS) {
    const v = hex((a as Record<string, unknown>)[k]);
    if (!v) return null;
    out[k] = v;
  }
  return { bg, text, accent, ansi: out as Ansi16 };
}

/** Avatar colours for profiles: distinct, and readable with dark or white initials. */
export const PROFILE_COLORS = [
  "#7aa2f7",
  "#bb9af7",
  "#f7768e",
  "#ff9e64",
  "#e0af68",
  "#9ece6a",
  "#73daca",
  "#7dcfff",
  "#c0caf5",
  "#a9b1d6",
] as const;

/** A profile's saved colours as a theme (the picker draws each profile in its own look). */
export function lookSpec(look: { bg: string; text: string; accent: string } | null): ThemeSpec {
  const base = builtIn("ember");
  if (!look) return base;
  return {
    ...base,
    id: "look",
    tokens: undefined,
    bg: look.bg,
    text: look.text,
    accent: look.accent,
  };
}

/** Ink for text on a colour: dark on light colours, white on dark ones. */
export function inkOn(hex: string): string {
  return hexToOklch(hex)[0] > 0.62 ? "#16161d" : "#ffffff";
}
