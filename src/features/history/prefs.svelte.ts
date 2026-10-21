/** History and recording preferences (kept in the database by the backend). */
import { wrap } from "$features/vault/vault.svelte";
import { commands, type HistorySettings, inTauri } from "$lib/ipc";

const DEFAULTS: HistorySettings = {
  enabled: true,
  ignoreSpace: true,
  exclude: [],
  recordAll: false,
  notifyAfterSecs: 10,
};

class HistoryPrefs {
  current = $state<HistorySettings>({ ...DEFAULTS });
  #loaded = false;

  async load(): Promise<void> {
    if (!inTauri || this.#loaded) return;
    this.#loaded = true;
    this.current = await commands.historySettings();
  }

  /** Save; returns an error message, or null. */
  async save(next: HistorySettings): Promise<string | null> {
    const res = wrap(await commands.historySetSettings(next));
    if (!res.ok) return res.error.message;
    this.current = next;
    return null;
  }
}

export const historyPrefs = new HistoryPrefs();
