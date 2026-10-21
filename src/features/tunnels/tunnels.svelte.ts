/** Tunnels (§9.11): saved forwards, their live status, and a short traffic history. */

import { wrap } from "$features/vault/vault.svelte";
import {
  commands,
  events,
  inTauri,
  type Tunnel,
  type TunnelInput,
  type TunnelStatus,
  type TunnelView,
} from "$lib/ipc";
import { toast } from "$lib/ui/toast.svelte";
import { HISTORY } from "./format";

function idle(id: string): TunnelStatus {
  return {
    id,
    state: "stopped",
    error: null,
    boundPort: null,
    up: 0,
    down: 0,
    upRate: 0,
    downRate: 0,
    active: 0,
    total: 0,
  };
}

class TunnelsStore {
  list = $state<Tunnel[]>([]);
  status = $state<Record<string, TunnelStatus>>({});
  /** Bytes per second (up + down) for the last minute, per tunnel. */
  history = $state<Record<string, number[]>>({});
  /** Tunnel being edited: `"new"`, an id, or null. */
  editing = $state<string | null>(null);
  error = $state<string | null>(null);
  #listening = false;

  async #listen() {
    if (this.#listening || !inTauri) return;
    this.#listening = true;
    await events.tunnelStatsEvent.listen((e) => {
      for (const s of e.payload.tunnels) {
        this.status[s.id] = s;
        if (s.state === "stopped") continue;
        const h = this.history[s.id] ?? [];
        h.push(s.upRate + s.downRate);
        if (h.length > HISTORY) h.shift();
        this.history[s.id] = h;
      }
    });
  }

  async refresh(): Promise<void> {
    if (!inTauri) return;
    void this.#listen();
    const r = wrap(await commands.tunnelsList());
    if (!r.ok) {
      this.error = r.error.message;
      return;
    }
    this.list = r.data.map((v: TunnelView) => v.tunnel);
    for (const v of r.data) this.status[v.tunnel.id] = v.status;
  }

  statusOf(id: string): TunnelStatus {
    return this.status[id] ?? idle(id);
  }

  running(id: string): boolean {
    const s = this.statusOf(id).state;
    return s === "running" || s === "starting";
  }

  /** Start or stop; returns an error message, or null. */
  async toggle(id: string): Promise<string | null> {
    this.error = null;
    if (this.running(id)) {
      await commands.tunnelStop(id);
      this.status[id] = idle(id);
      return null;
    }
    this.status[id] = { ...idle(id), state: "starting" };
    const r = wrap(await commands.tunnelStart(id));
    if (!r.ok) {
      this.status[id] = { ...idle(id), state: "failed", error: r.error.message };
      toast.error(r.error.message);
      return r.error.message;
    }
    this.status[id] = r.data;
    return null;
  }

  async save(id: string | null, input: TunnelInput): Promise<string | null> {
    const r = wrap(await commands.tunnelSave(id, input));
    if (!r.ok) return r.error.message;
    toast.success(id ? "Tunnel saved" : "Tunnel added");
    await this.refresh();
    this.editing = null;
    return null;
  }

  async remove(id: string) {
    const r = wrap(await commands.tunnelDelete(id));
    if (!r.ok) {
      this.error = r.error.message;
      toast.error(r.error.message);
    } else toast.success("Tunnel deleted");
    delete this.status[id];
    await this.refresh();
  }

  /** Start "on unlock" tunnels. */
  async autostart() {
    if (!inTauri) return;
    void this.#listen();
    await commands.tunnelsAutostart();
    await this.refresh();
  }
}

export const tunnels = new TunnelsStore();
