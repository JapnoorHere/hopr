/** Reactive terminal preferences, saved per device. */
import { readJson, writeJson } from "$lib/storage";
import { FONT_SIZE, parsePrefs, type TerminalPrefs } from "./prefs";

const KEY = "hopr.terminal";

class PrefsStore {
  current = $state<TerminalPrefs>(readJson(KEY, parsePrefs, parsePrefs(null)));

  update(patch: Partial<TerminalPrefs>) {
    this.current = parsePrefs({ ...this.current, ...patch });
    writeJson(KEY, this.current);
  }

  zoom(step: number) {
    const size = step === 0 ? 14 : this.current.fontSize + step;
    this.update({ fontSize: Math.min(FONT_SIZE.max, Math.max(FONT_SIZE.min, size)) });
  }
}

export const prefs = new PrefsStore();
