/**
 * Translations (i18n): a typed message catalog per locale, `t()` with `{name}`
 * placeholders, and the chosen locale (system default or explicit), kept per device.
 * Missing translations fall back to English, so a partial catalog never shows a key.
 */
import { safeStorage } from "$lib/design/theme";
import { de } from "./de";
import { en, type MessageKey, type Messages } from "./en";

export const LOCALES = {
  en: { name: "English", messages: en as Messages },
  de: { name: "Deutsch", messages: de },
} as const;

export type Locale = keyof typeof LOCALES;
export type LocalePref = Locale | "system";

const STORAGE_KEY = "hopr.locale";

/** The first supported language in the system's list, else English. */
export function resolveLocale(pref: LocalePref, languages: readonly string[]): Locale {
  if (pref !== "system") return pref;
  for (const tag of languages) {
    const base = tag.toLowerCase().split("-")[0];
    if (base && Object.hasOwn(LOCALES, base)) return base as Locale;
  }
  return "en";
}

/** Fill `{name}` placeholders; unknown ones are left as written. */
export function format(message: string, vars?: Record<string, string | number>): string {
  if (!vars) return message;
  return message.replace(/\{(\w+)\}/g, (all, name: string) =>
    Object.hasOwn(vars, name) ? String(vars[name]) : all,
  );
}

function loadPref(): LocalePref {
  const v = safeStorage()?.getItem(STORAGE_KEY);
  return v === "system" || (v && Object.hasOwn(LOCALES, v)) ? (v as LocalePref) : "system";
}

function systemLanguages(): readonly string[] {
  return typeof navigator === "undefined" ? [] : navigator.languages;
}

class I18n {
  pref = $state<LocalePref>(loadPref());
  readonly locale = $derived(resolveLocale(this.pref, systemLanguages()));

  /** The system's language, as offered by "System (…)". */
  get systemLocale(): Locale {
    return resolveLocale("system", systemLanguages());
  }

  set(pref: LocalePref) {
    this.pref = pref;
    safeStorage()?.setItem(STORAGE_KEY, pref);
    document.documentElement.lang = resolveLocale(pref, systemLanguages());
  }

  t(key: MessageKey, vars?: Record<string, string | number>): string {
    const message = LOCALES[this.locale].messages[key] ?? en[key];
    return format(message, vars);
  }
}

export const i18n = new I18n();

/** Translate (reactive: components re-render when the locale changes). */
export const t = (key: MessageKey, vars?: Record<string, string | number>): string =>
  i18n.t(key, vars);
