/**
 * Themes: one spec colours the whole app and its terminals. A spec names a background, text,
 * accent and the 16 ANSI colours; every other token (surfaces, borders, muted text, status
 * colours) is worked out from those in OKLCH. Hand-tuned themes can pin exact tokens.
 */
import {
  type Ansi16,
  ansi,
  contrast,
  hexToOklch,
  matte,
  oklchToHex,
  type TerminalColors,
} from "./palettes";

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

const ok = (L: number, C: number, H: number) => oklchToHex(L, C, H);
const matteAnsi = (a: Ansi16): Ansi16 =>
  Object.fromEntries(Object.entries(a).map(([k, v]) => [k, matte(v, 0.68)])) as Ansi16;

const BUILT_IN = [
  {
    id: "ember",
    name: "Ember",
    feel: "Cozy, distinctive",
    bg: ok(0.165, 0.006, 60),
    text: ok(0.93, 0.01, 80),
    accent: ok(0.78, 0.15, 70),
    ansi: matteAnsi(
      ansi([
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
    ),
    tokens: {
      bg: "oklch(0.165 0.006 60)",
      "surface-1": "oklch(0.19 0.007 60)",
      "surface-2": "oklch(0.215 0.008 60)",
      "surface-3": "oklch(0.245 0.009 60)",
      "surface-4": "oklch(0.275 0.01 60)",
      text: "oklch(0.93 0.01 80)",
      "text-muted": "oklch(0.74 0.012 75)",
      "text-faint": "oklch(0.58 0.012 70)",
      accent: "oklch(0.78 0.15 70)",
      "accent-fg": "oklch(0.2 0.02 60)",
      success: "oklch(0.78 0.16 145)",
      warning: "oklch(0.85 0.15 95)",
      danger: "oklch(0.66 0.19 28)",
    },
  },
  {
    id: "moss",
    name: "Moss",
    feel: "Hacker, refined",
    bg: ok(0.155, 0.01, 160),
    text: ok(0.93, 0.015, 140),
    accent: ok(0.9, 0.19, 130),
    ansi: matteAnsi(
      ansi([
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
    ),
    tokens: {
      bg: "oklch(0.155 0.01 160)",
      "surface-1": "oklch(0.18 0.011 160)",
      "surface-2": "oklch(0.205 0.012 160)",
      "surface-3": "oklch(0.235 0.013 160)",
      "surface-4": "oklch(0.265 0.014 160)",
      text: "oklch(0.93 0.015 140)",
      "text-muted": "oklch(0.74 0.015 140)",
      "text-faint": "oklch(0.58 0.015 140)",
      accent: "oklch(0.9 0.19 130)",
      "accent-fg": "oklch(0.2 0.03 140)",
      success: "oklch(0.8 0.16 155)",
      warning: "oklch(0.85 0.15 85)",
      danger: "oklch(0.66 0.19 25)",
    },
  },
  {
    id: "tide",
    name: "Tide",
    feel: "Calm, techy",
    bg: ok(0.16, 0.015, 225),
    text: ok(0.93, 0.01, 210),
    accent: ok(0.8, 0.12, 185),
    ansi: matteAnsi(
      ansi([
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
    ),
    tokens: {
      bg: "oklch(0.16 0.015 225)",
      "surface-1": "oklch(0.185 0.016 225)",
      "surface-2": "oklch(0.21 0.017 225)",
      "surface-3": "oklch(0.24 0.018 225)",
      "surface-4": "oklch(0.27 0.019 225)",
      text: "oklch(0.93 0.01 210)",
      "text-muted": "oklch(0.74 0.014 215)",
      "text-faint": "oklch(0.58 0.014 215)",
      accent: "oklch(0.8 0.12 185)",
      "accent-fg": "oklch(0.2 0.03 200)",
      success: "oklch(0.79 0.15 150)",
      warning: "oklch(0.85 0.15 85)",
      danger: "oklch(0.67 0.18 25)",
    },
  },
  // Community colour schemes (MIT), with their published colours.
  {
    id: "rose-pine",
    name: "Rosé Pine",
    feel: "Dusky plum",
    bg: "#191724",
    text: "#e0def4",
    accent: "#ebbcba",
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
  {
    id: "kanagawa",
    name: "Kanagawa",
    feel: "Ink and wave",
    bg: "#1f1f28",
    text: "#dcd7ba",
    accent: "#7e9cd8",
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
  {
    id: "catppuccin-mocha",
    name: "Catppuccin Mocha",
    feel: "Soft pastel",
    bg: "#1e1e2e",
    text: "#cdd6f4",
    accent: "#cba6f7",
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
  {
    id: "gruvbox-material",
    name: "Gruvbox Material",
    feel: "Retro, soft brown",
    bg: "#282828",
    text: "#d4be98",
    accent: "#d8a657",
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
  {
    id: "nord",
    name: "Nord",
    feel: "Arctic, quiet",
    bg: "#2e3440",
    text: "#d8dee9",
    accent: "#88c0d0",
    ansi: ansi([
      "#3b4252",
      "#bf616a",
      "#a3be8c",
      "#ebcb8b",
      "#81a1c1",
      "#b48ead",
      "#88c0d0",
      "#e5e9f0",
      "#4c566a",
      "#bf616a",
      "#a3be8c",
      "#ebcb8b",
      "#81a1c1",
      "#b48ead",
      "#8fbcbb",
      "#eceff4",
    ]),
  },
  {
    id: "tokyo-night",
    name: "Tokyo Night",
    feel: "Neon city at night",
    bg: "#1a1b26",
    text: "#c0caf5",
    accent: "#7aa2f7",
    ansi: ansi([
      "#15161e",
      "#f7768e",
      "#9ece6a",
      "#e0af68",
      "#7aa2f7",
      "#bb9af7",
      "#7dcfff",
      "#a9b1d6",
      "#414868",
      "#f7768e",
      "#9ece6a",
      "#e0af68",
      "#7aa2f7",
      "#bb9af7",
      "#7dcfff",
      "#c0caf5",
    ]),
  },
  {
    id: "everforest",
    name: "Everforest",
    feel: "Forest, easy on the eyes",
    bg: "#2d353b",
    text: "#d3c6aa",
    accent: "#a7c080",
    ansi: ansi([
      "#475258",
      "#e67e80",
      "#a7c080",
      "#dbbc7f",
      "#7fbbb3",
      "#d699b6",
      "#83c092",
      "#d3c6aa",
      "#859289",
      "#e67e80",
      "#a7c080",
      "#dbbc7f",
      "#7fbbb3",
      "#d699b6",
      "#83c092",
      "#e4dcc8",
    ]),
  },
  {
    id: "dracula",
    name: "Dracula",
    feel: "Vivid, high energy",
    bg: "#282a36",
    text: "#f8f8f2",
    accent: "#bd93f9",
    ansi: ansi([
      "#21222c",
      "#ff5555",
      "#50fa7b",
      "#f1fa8c",
      "#bd93f9",
      "#ff79c6",
      "#8be9fd",
      "#f8f8f2",
      "#6272a4",
      "#ff6e6e",
      "#69ff94",
      "#ffffa5",
      "#d6acff",
      "#ff92df",
      "#a4ffff",
      "#ffffff",
    ]),
  },
  {
    id: "midnight",
    name: "Midnight",
    feel: "True black, for OLED",
    bg: "#000000",
    text: "#e4e4e7",
    accent: "#a78bfa",
    ansi: ansi([
      "#18181b",
      "#f87171",
      "#4ade80",
      "#facc15",
      "#60a5fa",
      "#c084fc",
      "#22d3ee",
      "#d4d4d8",
      "#52525b",
      "#fca5a5",
      "#86efac",
      "#fde047",
      "#93c5fd",
      "#d8b4fe",
      "#67e8f9",
      "#fafafa",
    ]),
  },
  {
    id: "paper",
    name: "Paper",
    feel: "Warm off-white",
    bg: ok(0.96, 0.012, 85),
    text: ok(0.22, 0.012, 60),
    accent: ok(0.6, 0.19, 35),
    ansi: matteAnsi(
      ansi([
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
    ),
    tokens: {
      bg: "oklch(0.96 0.012 85)",
      "surface-1": "oklch(0.935 0.013 85)",
      "surface-2": "oklch(0.91 0.014 85)",
      "surface-3": "oklch(0.975 0.008 85)",
      "surface-4": "oklch(0.99 0.005 85)",
      border: "oklch(0.2 0.02 60 / 0.12)",
      "border-strong": "oklch(0.2 0.02 60 / 0.2)",
      text: "oklch(0.22 0.012 60)",
      "text-muted": "oklch(0.42 0.012 60)",
      "text-faint": "oklch(0.55 0.012 60)",
      accent: "oklch(0.6 0.19 35)",
      "accent-fg": "oklch(0.98 0.01 85)",
      success: "oklch(0.52 0.14 150)",
      warning: "oklch(0.6 0.14 75)",
      danger: "oklch(0.55 0.2 28)",
    },
  },
  {
    id: "rose-pine-dawn",
    name: "Rosé Pine Dawn",
    feel: "Morning blush",
    bg: "#faf4ed",
    text: "#575279",
    accent: "#d7827e",
    ansi: ansi([
      "#575279",
      "#b4637a",
      "#286983",
      "#ea9d34",
      "#56949f",
      "#907aa9",
      "#d7827e",
      "#797593",
      "#9893a5",
      "#b4637a",
      "#286983",
      "#ea9d34",
      "#56949f",
      "#907aa9",
      "#d7827e",
      "#575279",
    ]),
  },
  {
    id: "catppuccin-latte",
    name: "Catppuccin Latte",
    feel: "Pastel daylight",
    bg: "#eff1f5",
    text: "#4c4f69",
    accent: "#8839ef",
    ansi: ansi([
      "#5c5f77",
      "#d20f39",
      "#40a02b",
      "#df8e1d",
      "#1e66f5",
      "#ea76cb",
      "#179299",
      "#acb0be",
      "#6c6f85",
      "#d20f39",
      "#40a02b",
      "#df8e1d",
      "#1e66f5",
      "#ea76cb",
      "#179299",
      "#bcc0cc",
    ]),
  },
  {
    id: "solarized-light",
    name: "Solarized Light",
    feel: "Classic, low glare",
    bg: "#fdf6e3",
    text: "#586e75",
    accent: "#268bd2",
    ansi: ansi([
      "#073642",
      "#dc322f",
      "#859900",
      "#b58900",
      "#268bd2",
      "#d33682",
      "#2aa198",
      "#eee8d5",
      "#002b36",
      "#cb4b16",
      "#586e75",
      "#657b83",
      "#839496",
      "#6c71c4",
      "#93a1a1",
      "#fdf6e3",
    ]),
  },
] as const satisfies readonly ThemeSpec[];

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

/** Every design token for a theme, as CSS colours. */
export function themeTokens(spec: ThemeSpec): Tokens {
  const [L, C, H] = hexToOklch(spec.bg);
  const [tL, tC, tH] = hexToOklch(spec.text);
  const [aL, aC, aH] = hexToOklch(spec.accent);
  const dark = isDark(spec);
  const clampL = (v: number) => Math.min(1, Math.max(0, v));
  const surface = dark
    ? [1, 2, 3, 4].map((i) => lch(clampL(L + 0.025 * i), C, H))
    : [-0.025, -0.05, 0.015, 0.03].map((d) => lch(clampL(L + d), C, H));
  // Text on the accent: whichever of a deep accent-tinted ink or near-white reads better.
  const ink = oklchToHex(0.2, aC * 0.15, aH);
  const white = oklchToHex(0.98, 0.01, aH);
  const fg =
    contrast(spec.accent, ink) >= contrast(spec.accent, white)
      ? lch(0.2, aC * 0.15, aH)
      : lch(0.98, 0.01, aH);
  const computed: Tokens = {
    bg: css(spec.bg),
    "surface-1": surface[0] as string,
    "surface-2": surface[1] as string,
    "surface-3": surface[2] as string,
    "surface-4": surface[3] as string,
    border: dark ? "oklch(1 0 0 / 0.08)" : lch(0.2, 0.02, H, 0.12),
    "border-strong": dark ? "oklch(1 0 0 / 0.14)" : lch(0.2, 0.02, H, 0.2),
    text: css(spec.text),
    "text-muted": lch(L + (tL - L) * 0.75, tC, tH),
    "text-faint": lch(L + (tL - L) * 0.55, tC, tH),
    accent: lch(aL, aC, aH),
    "accent-fg": fg,
    success: css(spec.ansi.green),
    warning: css(spec.ansi.yellow),
    danger: css(spec.ansi.red),
    scheme: dark ? "dark" : "light",
  };
  return { ...computed, ...spec.tokens };
}

/** Inline CSS variables for a theme: the window root, or a preview card. */
export function themeStyle(spec: ThemeSpec): string {
  const t = themeTokens(spec);
  return TOKENS.map((k) => `--${k}: ${t[k]}`).join("; ");
}

/** How the terminal's background differs from the theme's (Settings > Terminal > Background). */
export interface TerminalBackground {
  /** 0..100: lighter on a dark theme, a touch deeper on a light one. */
  lift: number;
  /** 30..100 */
  opacity: number;
}

/** The terminal's background: the theme's, lifted, as #rrggbb or #rrggbbaa when see-through. */
export function terminalBackground(
  spec: ThemeSpec,
  b: TerminalBackground = { lift: 0, opacity: 100 },
): string {
  const [L, C, H] = hexToOklch(spec.bg);
  // Blend the lightness toward a soft charcoal (dark themes) or a deeper paper (light ones).
  const target = isDark(spec) ? Math.max(L, 0.4) : Math.min(L, 0.82);
  const hex = b.lift ? oklchToHex(L + (target - L) * (b.lift / 100), C, H) : spec.bg;
  if (b.opacity >= 100) return hex;
  return `${hex}${Math.round((b.opacity / 100) * 255)
    .toString(16)
    .padStart(2, "0")}`;
}

/** The terminal's colours for a theme. */
export function terminalColors(spec: ThemeSpec, b?: TerminalBackground): TerminalColors {
  const [L] = hexToOklch(spec.bg);
  const [tL, tC, tH] = hexToOklch(spec.text);
  return {
    background: terminalBackground(spec, b),
    foreground: spec.text,
    cursor: spec.accent,
    faint: oklchToHex(L + (tL - L) * 0.55, tC, tH),
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
