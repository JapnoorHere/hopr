/** Appearance model: theme, density and motion. Pure functions so they test without a DOM framework. */

import {
  type CustomTheme,
  isThemeId,
  parseCustom,
  type ThemeId,
  TOKENS,
  themeSpec,
  themeTokens,
} from "./themes";

export { THEMES, type ThemeId } from "./themes";

export const DENSITIES = { compact: 0.8, comfortable: 1, spacious: 1.25 } as const;
export type Density = keyof typeof DENSITIES;

/** How much the interface moves. `system` follows Windows' "Animation effects" setting. */
export const MOTIONS = ["system", "full", "reduced", "off"] as const;
export type MotionPref = (typeof MOTIONS)[number];
/** What the page actually uses: `reduced` keeps fades and colour changes, drops movement and loops. */
export type MotionLevel = Exclude<MotionPref, "system">;

/** Window backdrop: a solid frame, Windows 11 Mica (desktop tint) or Acrylic (blurs what's behind). */
export const BACKDROPS = ["solid", "mica", "acrylic"] as const;
export type Backdrop = (typeof BACKDROPS)[number];

export interface Appearance {
  /** A built-in theme id, or "custom". */
  theme: ThemeId | "custom";
  /** The custom theme's colours, kept even while a built-in theme is in use. */
  custom: CustomTheme | null;
  density: Density;
  motion: MotionPref;
  backdrop: Backdrop;
}

export const DEFAULT_APPEARANCE: Appearance = {
  theme: "ember",
  custom: null,
  density: "comfortable",
  motion: "system",
  backdrop: "solid",
};

const STORAGE_KEY = "hopr.appearance";

function isDensity(v: unknown): v is Density {
  return typeof v === "string" && Object.hasOwn(DENSITIES, v);
}

function isMotion(v: unknown): v is MotionPref {
  return MOTIONS.some((m) => m === v);
}

/** The system's reduced-motion setting, when it can be read. */
export function systemPrefersReduced(): boolean {
  try {
    return globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
  } catch {
    return false;
  }
}

export function resolveMotion(pref: MotionPref, systemReduced: boolean): MotionLevel {
  if (pref !== "system") return pref;
  return systemReduced ? "reduced" : "full";
}

/** Validate untrusted input (storage, imports); invalid fields fall back to defaults. */
export function parseAppearance(raw: unknown): Appearance {
  if (typeof raw !== "object" || raw === null) return { ...DEFAULT_APPEARANCE };
  const r = raw as Record<string, unknown>;
  const custom = parseCustom(r.custom);
  return {
    theme:
      isThemeId(r.theme) || (r.theme === "custom" && custom)
        ? (r.theme as ThemeId | "custom")
        : DEFAULT_APPEARANCE.theme,
    custom,
    density: isDensity(r.density) ? r.density : DEFAULT_APPEARANCE.density,
    motion: isMotion(r.motion) ? r.motion : DEFAULT_APPEARANCE.motion,
    backdrop: BACKDROPS.some((b) => b === r.backdrop)
      ? (r.backdrop as Backdrop)
      : DEFAULT_APPEARANCE.backdrop,
  };
}

export function applyAppearance(a: Appearance, root: HTMLElement = document.documentElement) {
  root.dataset.theme = a.theme;
  const tokens = themeTokens(themeSpec(a.theme, a.custom));
  for (const k of TOKENS) root.style.setProperty(`--${k}`, tokens[k]);
  root.style.setProperty("--density", String(DENSITIES[a.density]));
  root.dataset.motion = resolveMotion(a.motion, systemPrefersReduced());
  root.dataset.backdrop = a.backdrop;
}

/** Storage can be missing or throw (private mode, blocked site data). */
export function safeStorage(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

export function loadAppearance(storage: Storage | null): Appearance {
  try {
    const raw = storage?.getItem(STORAGE_KEY);
    return raw ? parseAppearance(JSON.parse(raw)) : { ...DEFAULT_APPEARANCE };
  } catch {
    return { ...DEFAULT_APPEARANCE };
  }
}

export function saveAppearance(a: Appearance, storage: Storage | null) {
  try {
    storage?.setItem(STORAGE_KEY, JSON.stringify(a));
  } catch {
    // Non-critical convenience; ignore quota or access errors.
  }
}
