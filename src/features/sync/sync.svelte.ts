/**
 * Sync (§10.8, Y1-lite): status, turning it on, and syncing on unlock and every few
 * minutes. After a sync that changed something here, the library, snippets and tunnels
 * reload.
 */
import { library } from "$features/hosts/library.svelte";
import { snippets } from "$features/snippets/snippets.svelte";
import { tunnels } from "$features/tunnels/tunnels.svelte";
import { wrap } from "$features/vault/vault.svelte";
import { commands, inTauri, type SyncReport, type SyncStatus } from "$lib/ipc";

/** How often to sync while unlocked. */
const EVERY_MS = 5 * 60 * 1000;

type Outcome = { ok: true; data: SyncReport } | { ok: false; error: { message: string } };

class SyncStore {
  status = $state<SyncStatus | null>(null);
  last = $state<SyncReport | null>(null);
  busy = $state(false);
  error = $state<string | null>(null);
  #timer: ReturnType<typeof setInterval> | undefined;

  async refresh() {
    if (!inTauri) return;
    const r = wrap(await commands.syncStatus());
    if (r.ok) this.status = r.data;
  }

  async #after(r: Outcome): Promise<boolean> {
    this.busy = false;
    if (!r.ok) {
      this.error = r.error.message;
      await this.refresh();
      return false;
    }
    this.error = null;
    this.last = r.data;
    if (r.data.applied > 0 || r.data.deleted > 0) {
      await Promise.all([library.refresh(), snippets.refresh(), tunnels.refresh()]);
    }
    await this.refresh();
    return true;
  }

  async enable(folder: string, passphrase: string, deviceName: string): Promise<boolean> {
    this.busy = true;
    this.error = null;
    return this.#after(wrap(await commands.syncEnable(folder, passphrase, deviceName)));
  }

  async now(): Promise<boolean> {
    if (this.busy) return false;
    this.busy = true;
    return this.#after(wrap(await commands.syncNow()));
  }

  async disable() {
    await commands.syncDisable();
    this.last = null;
    this.stop();
    await this.refresh();
  }

  /** Sync now (if on) and then every few minutes; call when the vault unlocks. */
  async start() {
    await this.refresh();
    this.stop();
    if (!this.status?.enabled) return;
    void this.now();
    this.#timer = setInterval(() => {
      if (this.status?.enabled) void this.now();
    }, EVERY_MS);
  }

  stop() {
    clearInterval(this.#timer);
    this.#timer = undefined;
  }
}

export const syncStore = new SyncStore();
