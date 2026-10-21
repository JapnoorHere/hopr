/** Transfer queue (S4/S5): mirrors the backend's jobs from progress events. */
import { commands, events, inTauri, type Transfer } from "$lib/ipc";

const ACTIVE = new Set(["queued", "running", "paused"]);

class TransfersStore {
  list = $state<Transfer[]>([]);
  open = $state(false);
  #started = false;

  async init() {
    if (this.#started || !inTauri) return;
    this.#started = true;
    this.list = await commands.transfersList();
    await events.transfer.listen((e) => this.#upsert(e.payload));
  }

  #upsert(t: Transfer) {
    const i = this.list.findIndex((x) => x.id === t.id);
    if (i === -1) this.list = [...this.list, t];
    else this.list[i] = t;
  }

  get active(): Transfer[] {
    return this.list.filter((t) => ACTIVE.has(t.status));
  }

  /** Across unfinished transfers: count, overall percent and combined speed. */
  get summary(): { count: number; percent: number; speed: number; failed: number } {
    const a = this.active;
    const total = a.reduce((n, t) => n + t.size, 0);
    const done = a.reduce((n, t) => n + t.done, 0);
    return {
      count: a.length,
      percent: total > 0 ? Math.round((done / total) * 100) : 0,
      speed: a.reduce((n, t) => n + (t.status === "running" ? t.speed : 0), 0),
      failed: this.list.filter((t) => t.status === "failed").length,
    };
  }

  pause(t: Transfer) {
    return commands.transferPause(t.id);
  }

  resume(t: Transfer) {
    return commands.transferResume(t.id);
  }

  cancel(t: Transfer) {
    return commands.transferCancel(t.id);
  }

  async clear() {
    await commands.transfersClear();
    this.list = this.list.filter((t) => ACTIVE.has(t.status));
  }
}

export const transfers = new TransfersStore();
