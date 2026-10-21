/**
 * Workspaces (R6): save and reopen layouts, autosave the last session, and an optional
 * startup workspace. Only the main window saves; torn-off windows are transient.
 */
import { jumpTrail } from "$features/hosts/chain";
import { library } from "$features/hosts/library.svelte";
import { sessions, type TabTarget } from "$features/terminal/session.svelte";
import { wrap } from "$features/vault/vault.svelte";
import { commands, inTauri, type Workspace } from "$lib/ipc";
import { parseWorkspace, type SavedWorkspace } from "./snapshot";

const AUTOSAVE_MS = 1500;

/** Refresh a saved target against the library; a deleted host drops its pane. */
export function resolveTarget(t: TabTarget): TabTarget | null {
  if (t.kind !== "host") return t;
  const h = library.host(t.hostId);
  if (!h) return null;
  return {
    kind: "host",
    hostId: h.id,
    label: h.label,
    environment: h.environment,
    trail: jumpTrail(h, library.hosts).map((x) => x.label),
  };
}

class WorkspacesStore {
  list = $state<Workspace[]>([]);
  /** The last session, offered for restore after the first unlock. */
  offer = $state<SavedWorkspace | null>(null);
  /** False until the startup decision is made (restore, offer or nothing). */
  ready = $state(false);

  #timer: ReturnType<typeof setTimeout> | undefined;
  #last = "";

  async refresh() {
    if (!inTauri) return;
    const r = await commands.workspacesList();
    if (r.status === "ok") this.list = r.data;
  }

  /** After the first unlock in the main window: open the startup workspace, or offer the
   *  last session. */
  async startup() {
    if (this.ready) return;
    await Promise.all([this.refresh(), library.refresh()]);
    const start = this.list.find((w) => w.startup);
    const last = this.list.find((w) => w.autosave);
    if (start && sessions.tabs.length === 0) {
      this.open(start);
    } else if (last && sessions.tabs.length === 0) {
      const ws = parseWorkspace(last.layoutJson);
      if (ws && ws.tabs.length > 0) this.offer = ws;
    }
    this.#last = JSON.stringify(sessions.snapshot());
    this.ready = true;
  }

  open(w: Workspace): number {
    const ws = parseWorkspace(w.layoutJson);
    if (!ws) return 0;
    this.offer = null;
    return sessions.restore(ws, resolveTarget);
  }

  restoreLast(): number {
    const ws = this.offer ?? parseWorkspace(this.list.find((w) => w.autosave)?.layoutJson ?? "");
    this.offer = null;
    return ws ? sessions.restore(ws, resolveTarget) : 0;
  }

  dismissOffer() {
    this.offer = null;
  }

  async save(name: string) {
    const res = wrap(await commands.workspaceSave(name, JSON.stringify(sessions.snapshot())));
    if (res.ok) await this.refresh();
    return res;
  }

  async remove(id: string) {
    await commands.workspaceDelete(id);
    await this.refresh();
  }

  async setStartup(id: string | null) {
    const res = wrap(await commands.workspaceSetStartup(id));
    if (res.ok) await this.refresh();
    return res;
  }

  /** The layout changed: autosave it shortly (only real changes are written). While the
   *  last session is still on offer, nothing is written, so it can't be overwritten by
   *  an empty layout. Opening a tab yourself withdraws the offer. */
  changed(json: string) {
    if (!this.ready || !inTauri) return;
    if (this.offer) {
      if (sessions.tabs.length === 0) return;
      this.offer = null;
    }
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => {
      if (json === this.#last) return;
      this.#last = json;
      void commands.workspaceAutosave(json);
    }, AUTOSAVE_MS);
  }
}

export const workspaces = new WorkspacesStore();
