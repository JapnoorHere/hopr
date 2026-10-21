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

/** The motion level in effect: Settings > Motion, which by default follows the system. */
export function motionLevel(): "full" | "reduced" | "off" {
  const set = globalThis.document?.documentElement.dataset.motion;
  if (set === "full" || set === "reduced" || set === "off") return set;
  try {
    return globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ? "reduced" : "full";
  } catch {
    return "full";
  }
}

/** Transition duration: 0 when motion is reduced or off (those fades are not essential). */
export function motion(ms: number): number {
  return motionLevel() === "full" ? ms : 0;
}
