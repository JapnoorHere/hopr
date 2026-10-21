/** History drawer state (§9.18, §9.19): commands, connections, recordings, replay. */

import { jumpTrail } from "$features/hosts/chain";
import { library } from "$features/hosts/library.svelte";
import { sessions, type TerminalSession } from "$features/terminal/session.svelte";
import { wrap } from "$features/vault/vault.svelte";
import {
  type CommandRecord,
  type ConnectionRecord,
  commands,
  inTauri,
  type Recording,
} from "$lib/ipc";
import { toast } from "$lib/ui/toast.svelte";

export type HistoryTab = "commands" | "connections" | "recordings";

/** `null` = every host, "" = quick connect and local shells, else a host id. */
export type HostFilter = string | null;

const LIMIT = 500;

function sameTarget(s: TerminalSession, rec: CommandRecord): boolean {
  const t = s.target;
  if (rec.hostId) return t.kind === "host" && t.hostId === rec.hostId;
  return t.kind !== "host" && s.label === rec.hostLabel;
}

/** Type into a session once it is ready and its first output (the prompt) has settled. */
function typeWhenReady(s: TerminalSession, text: string) {
  let lastBytes = -1;
  let tries = 0;
  const tick = () => {
    if (++tries > 120 || s.state.kind === "failed" || s.state.kind === "closed") return;
    if (s.state.kind === "ready" && s.id && s.bytesIn > 0 && s.bytesIn === lastBytes) {
      s.write(new TextEncoder().encode(text));
      return;
    }
    lastBytes = s.bytesIn;
    setTimeout(tick, 400);
  };
  setTimeout(tick, 400);
}

class HistoryStore {
  tab = $state<HistoryTab>("commands");
  query = $state("");
  host = $state<HostFilter>(null);
  commands = $state<CommandRecord[]>([]);
  connections = $state<ConnectionRecord[]>([]);
  recordings = $state<Recording[]>([]);
  error = $state<string | null>(null);
  notice = $state<string | null>(null);
  /** Recording open in the replay overlay. */
  replaying = $state<string | null>(null);
  /** The replay's position, mirrored for the dev E2E hooks. */
  player = $state<{ pos: number; duration: number; playing: boolean; speed: number } | null>(null);
  /** The replay screen's text (dev E2E hooks). */
  playerText: (() => string) | null = null;

  /** An action failed: say so in a toast (the list's own load errors stay in `error`). */
  #fail(message: string) {
    toast.error(message);
  }

  #done(message: string) {
    this.notice = message;
    toast.success(message);
  }

  async refresh(): Promise<void> {
    if (!inTauri) return;
    this.error = null;
    if (this.tab === "commands") {
      const r = wrap(await commands.historyList(this.query, this.host, LIMIT));
      if (r.ok) this.commands = r.data;
      else this.error = r.error.message;
    } else if (this.tab === "connections") {
      const r = wrap(await commands.connectionsList(this.host || null, LIMIT));
      if (r.ok) this.connections = r.data;
      else this.error = r.error.message;
    } else {
      const r = wrap(await commands.recordingsList());
      if (r.ok) this.recordings = r.data;
      else this.error = r.error.message;
    }
  }

  async show(tab: HistoryTab) {
    this.tab = tab;
    await this.refresh();
  }

  async remove(rec: CommandRecord) {
    const r = wrap(await commands.historyDelete(rec.id));
    if (r.ok) this.commands = this.commands.filter((c) => c.id !== rec.id);
    else this.#fail(r.error.message);
  }

  async clear() {
    const r = wrap(await commands.historyClear());
    if (!r.ok) {
      this.#fail(r.error.message);
      return;
    }
    this.commands = [];
    this.connections = [];
    this.#done(`Cleared ${r.data} command${r.data === 1 ? "" : "s"}`);
  }

  async copy(rec: CommandRecord) {
    await navigator.clipboard.writeText(rec.command).catch(() => undefined);
    this.#done("Command copied");
  }

  /** Run a command again: in the focused pane if it's on the same host, else in a new tab
   *  to that host (typed once the shell is up). */
  runAgain(rec: CommandRecord) {
    const active = sessions.active;
    if (active && sameTarget(active, rec) && active.state.kind === "ready") {
      active.write(new TextEncoder().encode(`${rec.command}\r`));
      return;
    }
    const host = rec.hostId ? library.host(rec.hostId) : undefined;
    if (!host) {
      this.#fail("Open a session to that host first, then run it again");
      return;
    }
    const s = sessions.open({
      kind: "host",
      hostId: host.id,
      label: host.label,
      environment: host.environment,
      trail: jumpTrail(host, library.hosts).map((x) => x.label),
    });
    typeWhenReady(s, `${rec.command}\r`);
  }

  async deleteRecording(name: string) {
    const r = wrap(await commands.recordingDelete(name));
    if (r.ok) {
      this.recordings = this.recordings.filter((x) => x.name !== name);
      toast.success("Recording deleted");
    } else this.#fail(r.error.message);
  }
}

export const history = new HistoryStore();
