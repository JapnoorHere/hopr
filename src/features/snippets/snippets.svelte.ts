/**
 * Snippets (§9.12): the library, named secrets, and running a snippet in the focused
 * terminal: ask for its variables, confirm when its run mode says so or the host is
 * production, then type it (the backend fills in `{{secret:…}}`).
 */

import { library } from "$features/hosts/library.svelte";
import { sessions, type TerminalSession } from "$features/terminal/session.svelte";
import { wrap } from "$features/vault/vault.svelte";
import { commands, inTauri, type Snippet, type SnippetInput } from "$lib/ipc";
import { flash } from "$lib/motion";
import { toast } from "$lib/ui/toast.svelte";
import { type Builtins, type Field, fields, render, today, usesClipboard } from "./template";

/** A run waiting for the user: variables to fill and/or a confirmation. */
export interface PendingRun {
  snippet: Snippet;
  session: TerminalSession;
  fields: Field[];
  values: Record<string, string>;
  /** Rendered text, once variables are filled (set when confirming). */
  text: string | null;
  /** Why it needs a confirmation (null = it doesn't). */
  confirm: string | null;
}

function builtinsFor(s: TerminalSession, clipboard: string): Builtins {
  const t = s.target;
  if (t.kind === "quick") return { host: t.host, user: t.username, date: today(), clipboard };
  if (t.kind === "host") {
    const h = library.host(t.hostId);
    return { host: h?.hostname ?? t.label, user: h?.username ?? "", date: today(), clipboard };
  }
  return { host: "localhost", user: "", date: today(), clipboard };
}

class SnippetsStore {
  list = $state<Snippet[]>([]);
  secrets = $state<string[]>([]);
  query = $state("");
  /** Snippet being edited: "new", an id, or null. */
  editing = $state<string | null>(null);
  /** Prefill for a new snippet (e.g. "Save as snippet" from history). */
  draftBody = $state("");
  pending = $state<PendingRun | null>(null);
  error = $state<string | null>(null);
  notice = $state<string | null>(null);

  get shown(): Snippet[] {
    const q = this.query.trim().toLowerCase();
    if (!q) return this.list;
    return this.list.filter((s) =>
      [s.name, s.folder, s.description, s.body, ...s.tags].some((f) => f.toLowerCase().includes(q)),
    );
  }

  /** Failures and confirmations also go to a toast; the fields stay for tests. */
  #fail(message: string) {
    this.error = message;
    toast.error(message);
  }

  #done(message: string) {
    this.notice = message;
    toast.success(message);
  }

  async refresh() {
    if (!inTauri) return;
    const [l, s] = await Promise.all([commands.snippetsList(), commands.secretsList()]);
    const lr = wrap(l);
    if (lr.ok) this.list = lr.data;
    else this.#fail(lr.error.message);
    const sr = wrap(s);
    if (sr.ok) this.secrets = sr.data;
  }

  async save(id: string | null, input: SnippetInput): Promise<string | null> {
    const r = wrap(await commands.snippetSave(id, input));
    if (!r.ok) return r.error.message;
    this.editing = null;
    this.draftBody = "";
    toast.success(id ? `Saved “${r.data.name}”` : `Added “${r.data.name}”`);
    await this.refresh();
    return null;
  }

  async remove(id: string) {
    const name = this.list.find((s) => s.id === id)?.name ?? "Snippet";
    const r = wrap(await commands.snippetDelete(id));
    if (!r.ok) this.#fail(r.error.message);
    else toast.success(`“${name}” deleted`);
    await this.refresh();
  }

  async setSecret(name: string, value: string): Promise<string | null> {
    const r = wrap(await commands.secretSet(name, value));
    if (!r.ok) return r.error.message;
    toast.success(`Secret “${name}” saved`);
    await this.refresh();
    return null;
  }

  async removeSecret(name: string) {
    await commands.secretDelete(name);
    toast.success(`Secret “${name}” deleted`);
    await this.refresh();
  }

  /** Start a new snippet from some text (history's "Save as snippet"). */
  newFrom(body: string) {
    this.draftBody = body;
    this.editing = "new";
  }

  /** Run a snippet in the focused terminal (or `session`). */
  async run(snippet: Snippet, session: TerminalSession | null = sessions.active) {
    this.error = null;
    this.notice = null;
    if (session?.state.kind !== "ready") {
      this.#fail("Open a terminal first; snippets run in the focused one");
      return;
    }
    const f = fields(snippet.body);
    this.pending = {
      snippet,
      session,
      fields: f,
      values: Object.fromEntries(f.map((x) => [x.key, x.default])),
      text: null,
      confirm: null,
    };
    if (f.length === 0) await this.fill();
  }

  /** Variables are filled: render, then confirm or type. */
  async fill() {
    const p = this.pending;
    if (!p) return;
    const clip = usesClipboard(p.snippet.body)
      ? await navigator.clipboard.readText().catch(() => "")
      : "";
    const text = render(p.snippet.body, p.values, builtinsFor(p.session, clip));
    const confirm =
      p.snippet.runMode === "confirm"
        ? "Run this?"
        : p.session.environment === "prod"
          ? `Run this on ${p.session.label} (production)?`
          : null;
    if (confirm) {
      this.pending = { ...p, text, confirm };
      return;
    }
    await this.send(p, text);
  }

  async confirm(ok: boolean) {
    const p = this.pending;
    if (!p) return;
    if (!ok || p.text === null) {
      this.pending = null;
      return;
    }
    await this.send(p, p.text);
  }

  async send(p: PendingRun, text: string) {
    this.pending = null;
    if (!p.session.id) return;
    // "Paste" types without Enter; the others run it.
    const enter = p.snippet.runMode !== "paste";
    const r = wrap(await commands.sessionType(p.session.id, text, enter));
    if (!r.ok) {
      this.#fail(r.error.message);
      return;
    }
    this.#done(`${enter ? "Ran" : "Typed"} “${p.snippet.name}” in ${p.session.label}`);
    // The tab it went to lights up once, so you see where it ran.
    flash(document.querySelector(".tab.active"));
  }
}

export const snippets = new SnippetsStore();
