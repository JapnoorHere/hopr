/** Run on hosts (§9.13 mode B, R3): pick hosts, run one command on all, collect results. */
import { wrap } from "$features/vault/vault.svelte";
import { commands, events, type GroupRunEvent, inTauri } from "$lib/ipc";
import { toCsv, toJson } from "./export";

export { toCsv, toJson };

class GroupRunStore {
  open = $state(false);
  selected = $state<string[]>([]);
  command = $state("");
  parallel = $state(4);
  stopOnFail = $state(false);
  runId = $state<string | null>(null);
  /** Results by host id. */
  rows = $state<Record<string, GroupRunEvent>>({});
  /** Command of the current results (for export). */
  ran = $state("");
  error = $state<string | null>(null);
  #listening = false;
  /** Events that arrived before their run id came back from the backend. */
  #early: GroupRunEvent[] = [];

  get results(): GroupRunEvent[] {
    return this.selected.flatMap((id) => this.rows[id] ?? []);
  }

  get running(): boolean {
    return this.results.some((r) => r.state === "queued" || r.state === "running");
  }

  async #listen() {
    if (this.#listening || !inTauri) return;
    this.#listening = true;
    await events.groupRunEvent.listen((e) => {
      if (e.payload.runId === this.runId) this.rows[e.payload.hostId] = e.payload;
      else if (this.runId === null) this.#early.push(e.payload);
    });
  }

  show(hostIds: string[] = []) {
    if (hostIds.length) this.selected = [...hostIds];
    this.open = true;
    void this.#listen();
  }

  async run(): Promise<boolean> {
    this.error = null;
    await this.#listen();
    this.rows = {};
    this.ran = this.command;
    this.runId = null;
    this.#early = [];
    const r = wrap(
      await commands.groupRun(this.selected, this.command, this.parallel, this.stopOnFail),
    );
    if (!r.ok) {
      this.error = r.error.message;
      return false;
    }
    this.runId = r.data;
    // Replay what arrived early, in order, so each host ends at its latest state (a
    // host that fails at once sends queued, running and failed before this point).
    for (const e of this.#early.splice(0)) {
      if (e.runId === r.data) this.rows[e.hostId] = e;
    }
    return true;
  }

  async cancel() {
    if (this.runId) await commands.groupRunCancel(this.runId);
  }
}

export const grouprun = new GroupRunStore();
