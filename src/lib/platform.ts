export const isMac =
  typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);

/** Render a canonical "Ctrl+Shift+K" shortcut for the current platform. */
export function formatShortcut(shortcut: string, mac: boolean = isMac): string {
  if (!mac) return shortcut;
  return shortcut
    .replace(/Ctrl\+/g, "⌘")
    .replace(/Shift\+/g, "⇧")
    .replace(/Alt\+/g, "⌥");
}

/** Transition duration that collapses to 0 under prefers-reduced-motion. */
export function motion(ms: number): number {
  try {
    return globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ? 0 : ms;
  } catch {
    return ms;
  }
}
