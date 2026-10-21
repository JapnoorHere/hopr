/**
 * Live broadcast (§9.13 mode A, R2): what is typed in one of the chosen panes is typed
 * into all of them. Starting it on a production host asks first.
 */
import { sessions, type TerminalSession } from "$features/terminal/session.svelte";

class Broadcast {
  /** Session keys that receive each other's keystrokes. */
  targets = $state<string[]>([]);
  /** Targets waiting for the production confirmation. */
  pending = $state<string[] | null>(null);

  get active(): boolean {
    return this.targets.length > 1;
  }

  includes(key: string): boolean {
    return this.active && this.targets.includes(key);
  }

  get hasProd(): boolean {
    return (this.pending ?? this.targets).some((k) => sessions.session(k)?.environment === "prod");
  }

  /** Broadcast between these sessions (asks first when one is production). */
  start(keys: string[]): boolean {
    const live = keys.filter((k) => sessions.session(k)?.state.kind === "ready");
    if (live.length < 2) return false;
    if (live.some((k) => sessions.session(k)?.environment === "prod")) {
      this.pending = live;
      return true;
    }
    this.targets = live;
    return true;
  }

  confirm(ok: boolean) {
    if (ok && this.pending) this.targets = this.pending;
    this.pending = null;
  }

  stop() {
    this.targets = [];
    this.pending = null;
  }

  /** The panes of the active tab. */
  startTab(): boolean {
    const tab = sessions.activeTab;
    return tab ? this.start(sessions.panesOf(tab).map((s) => s.key)) : false;
  }

  /** Every open session. */
  startAll(): boolean {
    return this.start(sessions.list.map((s) => s.key));
  }

  /** Typed into `from`: type it into the other targets too. */
  mirror(from: TerminalSession, data: Uint8Array) {
    if (!this.includes(from.key)) return;
    for (const key of this.targets) {
      if (key === from.key) continue;
      const s = sessions.session(key);
      if (s?.state.kind === "ready") s.write(data);
    }
  }
}

export const broadcast = new Broadcast();
