/** Appearance model: theme + density. Pure functions so they test without a DOM framework. */

export const THEMES = [
  { id: "ember", name: "Ember", feel: "Cozy, distinctive" },
  { id: "moss", name: "Moss", feel: "Hacker, refined" },
  { id: "tide", name: "Tide", feel: "Calm, techy" },
  { id: "paper", name: "Paper", feel: "A rare light theme" },
] as const;

export type ThemeId = (typeof THEMES)[number]["id"];

export const DENSITIES = { compact: 0.8, comfortable: 1, spacious: 1.25 } as const;
export type Density = keyof typeof DENSITIES;

export interface Appearance {
  theme: ThemeId;
  density: Density;
}

export const DEFAULT_APPEARANCE: Appearance = { theme: "ember", density: "comfortable" };

const STORAGE_KEY = "hopr.appearance";

function isThemeId(v: unknown): v is ThemeId {
  return THEMES.some((t) => t.id === v);
}

function isDensity(v: unknown): v is Density {
  return typeof v === "string" && Object.hasOwn(DENSITIES, v);
}

/** Validate untrusted input (storage, imports); invalid fields fall back to defaults. */
export function parseAppearance(raw: unknown): Appearance {
  if (typeof raw !== "object" || raw === null) return { ...DEFAULT_APPEARANCE };
  const r = raw as Record<string, unknown>;
  return {
    theme: isThemeId(r.theme) ? r.theme : DEFAULT_APPEARANCE.theme,
    density: isDensity(r.density) ? r.density : DEFAULT_APPEARANCE.density,
  };
}

export function applyAppearance(a: Appearance, root: HTMLElement = document.documentElement) {
  root.dataset.theme = a.theme;
  root.style.setProperty("--density", String(DENSITIES[a.density]));
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
