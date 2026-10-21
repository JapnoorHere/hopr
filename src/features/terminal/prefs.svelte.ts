/** Reactive terminal preferences, saved per device. */
import { safeStorage } from "$lib/design/theme";
import { FONT_SIZE, parsePrefs, type TerminalPrefs } from "./prefs";

const KEY = "hopr.terminal";

class PrefsStore {
  current = $state<TerminalPrefs>(parsePrefs(read()));

  update(patch: Partial<TerminalPrefs>) {
    this.current = parsePrefs({ ...this.current, ...patch });
    try {
      safeStorage()?.setItem(KEY, JSON.stringify(this.current));
    } catch {
      // Storage unavailable: the change still applies for this run.
    }
  }

  zoom(step: number) {
    const size = step === 0 ? 14 : this.current.fontSize + step;
    this.update({ fontSize: Math.min(FONT_SIZE.max, Math.max(FONT_SIZE.min, size)) });
  }
}

function read(): unknown {
  try {
    return JSON.parse(safeStorage()?.getItem(KEY) ?? "null");
  } catch {
    return null;
  }
}

export const prefs = new PrefsStore();
