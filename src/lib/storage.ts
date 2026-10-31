/**
 * Browser storage in one place. Storage can be missing or throw (private mode, blocked site
 * data, a full quota), so every read falls back and every write is best-effort: a setting
 * that can't be saved still applies for this run.
 *
 * Keys that belong to a profile (its theme, layout, recents) go through `scoped(key)`;
 * device-wide ones (fonts, shortcuts, language, panel widths) use the plain key. The original
 * profile keeps the plain keys, so nothing saved before profiles existed is lost.
 */

let scope = "";

export function setStorageScope(profileId: string) {
  scope = profileId === "default" ? "" : profileId;
}

export function scoped(key: string): string {
  return scope ? `${key}@${scope}` : key;
}

export function safeStorage(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

export function readText(key: string, storage = safeStorage()): string | null {
  try {
    return storage?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

export function writeText(key: string, value: string, storage = safeStorage()) {
  try {
    storage?.setItem(key, value);
  } catch {
    // Best-effort: see the note at the top.
  }
}

export function removeKey(key: string, storage = safeStorage()) {
  try {
    storage?.removeItem(key);
  } catch {
    // Nothing to forget.
  }
}

/** Read JSON through `parse`; anything missing, broken or rejected by `parse` gives `fallback`. */
export function readJson<T>(
  key: string,
  parse: (raw: unknown) => T,
  fallback: T,
  storage = safeStorage(),
): T {
  const raw = readText(key, storage);
  if (raw === null) return fallback;
  try {
    return parse(JSON.parse(raw));
  } catch {
    return fallback;
  }
}

export function writeJson(key: string, value: unknown, storage = safeStorage()) {
  writeText(key, JSON.stringify(value), storage);
}
