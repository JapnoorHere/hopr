/** The open tabs: their split layouts, the session in front, closing, reopening and tearing off. */
import { SvelteMap } from "svelte/reactivity";
import {
  type Direction,
  insert,
  type Layout,
  neighbor,
  pane,
  panes,
  remove,
  replace,
  type Side,
  split,
} from "$features/workspace/layout";
import { loadTab, type SavedTab, type SavedWorkspace, saveTab } from "$features/workspace/snapshot";
import { commands } from "$lib/ipc";
import {
  type Adopted,
  forgetSession,
  type QuickDraft,
  setInFront,
  type TabTarget,
  TerminalSession,
} from "./terminal-session.svelte";

/** A tab: a split layout of panes (each a session) with one focused pane. */
export class Tab {
  readonly key: string = crypto.randomUUID();
  layout = $state<Layout>() as Layout;
  focused = $state("");
  /** Pane shown full-size, if any. */
  zoomed = $state<string | null>(null);

  constructor(first: string) {
    this.layout = pane(first);
    this.focused = first;
  }
}

const MAX_CLOSED = 10;

class SessionsStore {
  tabs = $state<Tab[]>([]);
  /** Active tab key; null shows the hosts screen. */
  activeKey = $state<string | null>(null);
  /** Prefill for the quick-connect form (e.g. "Edit connection" after a failure). */
  draft = $state<QuickDraft | null>(null);
  #sessions = new SvelteMap<string, TerminalSession>();
  /** Recently closed tabs' targets, newest last (Reopen closed tab). */
  #closed: TabTarget[] = [];

  get activeTab(): Tab | null {
    return this.tabs.find((t) => t.key === this.activeKey) ?? null;
  }

  /** The focused pane's session in the active tab. */
  get active(): TerminalSession | null {
    const tab = this.activeTab;
    return tab ? (this.#sessions.get(tab.focused) ?? null) : null;
  }

  /** Every open session, in tab and pane order. */
  get list(): TerminalSession[] {
    return this.tabs.flatMap((t) => this.panesOf(t));
  }

  panesOf(tab: Tab): TerminalSession[] {
    return panes(tab.layout).flatMap((k) => this.#sessions.get(k) ?? []);
  }

  session(key: string): TerminalSession | undefined {
    return this.#sessions.get(key);
  }

  tabOf(session: TerminalSession): Tab | undefined {
    return this.tabs.find((t) => panes(t.layout).includes(session.key));
  }

  open(target: TabTarget, password: string | null = null): TerminalSession {
    const session = new TerminalSession(target, password);
    this.#sessions.set(session.key, session);
    const tab = new Tab(session.key);
    this.tabs.push(tab);
    this.activeKey = tab.key;
    this.draft = null;
    return session;
  }

  activate(key: string) {
    this.activeKey = key;
  }

  /** Show the hosts screen, optionally with the quick-connect form prefilled. */
  showHome(prefill: QuickDraft | null = null) {
    this.draft = prefill;
    this.activeKey = null;
  }

  /** Split the focused pane, opening the same target beside (`row`) or below (`column`) it. */
  split(dir: "row" | "column") {
    const tab = this.activeTab;
    const from = this.active;
    if (!tab || !from) return;
    const session = new TerminalSession(from.target);
    this.#sessions.set(session.key, session);
    tab.layout = split(tab.layout, from.key, dir, session.key);
    tab.zoomed = null;
    tab.focused = session.key;
  }

  focus(session: TerminalSession) {
    const tab = this.tabOf(session);
    if (!tab) return;
    tab.focused = session.key;
    this.activeKey = tab.key;
  }

  /** Move focus to the neighboring pane. */
  move(dir: Direction) {
    const tab = this.activeTab;
    if (!tab) return;
    const next = neighbor(tab.layout, tab.focused, dir);
    if (next) {
      tab.focused = next;
      if (tab.zoomed) tab.zoomed = next;
    }
  }

  toggleZoom() {
    const tab = this.activeTab;
    if (!tab || panes(tab.layout).length < 2) return;
    tab.zoomed = tab.zoomed ? null : tab.focused;
  }

  /** Next/previous tab, wrapping around. */
  cycle(step: number) {
    if (this.tabs.length === 0) return;
    const i = this.tabs.findIndex((t) => t.key === this.activeKey);
    const n = this.tabs.length;
    this.activeKey = this.tabs[(((i === -1 ? 0 : i + step) % n) + n) % n]?.key ?? null;
  }

  /** Tab by 1-based position; 9 always means the last tab. */
  goto(n: number) {
    const tab = n === 9 ? this.tabs.at(-1) : this.tabs[n - 1];
    if (tab) this.activeKey = tab.key;
  }

  reopenClosed() {
    const target = this.#closed.pop();
    if (target) this.open(target);
  }

  get canReopen(): boolean {
    return this.#closed.length > 0;
  }

  /** Reconnect in place. Saved hosts and local shells need nothing; quick connect reuses a
   *  password still held, or lets the server prompt. */
  retry(old: TerminalSession) {
    const tab = this.tabOf(old);
    const next = new TerminalSession(old.target, old.takePassword());
    this.#sessions.set(next.key, next);
    void old.close();
    this.#sessions.delete(old.key);
    if (!tab) {
      const t = new Tab(next.key);
      this.tabs.push(t);
      this.activeKey = t.key;
      return;
    }
    tab.layout = replace(tab.layout, old.key, next.key);
    if (tab.focused === old.key) tab.focused = next.key;
    if (tab.zoomed === old.key) tab.zoomed = next.key;
  }

  /** Close one pane; the tab goes when its last pane does. */
  async close(session: TerminalSession) {
    const tab = this.tabOf(session);
    this.#sessions.delete(session.key);
    if (tab) {
      const layout = remove(tab.layout, session.key);
      if (layout) {
        tab.layout = layout;
        if (tab.zoomed === session.key) tab.zoomed = null;
        if (tab.focused === session.key) tab.focused = panes(layout)[0] ?? "";
      } else {
        this.#forget(tab, session.target);
      }
    }
    await session.close();
  }

  /** Close a whole tab and every pane in it. */
  async closeTab(tab: Tab) {
    const members = this.panesOf(tab);
    for (const s of members) this.#sessions.delete(s.key);
    this.#forget(tab, members.find((s) => s.key === tab.focused)?.target ?? members[0]?.target);
    await Promise.all(members.map((s) => s.close()));
  }

  /** Tabs and splits as a saveable workspace (targets only, never secrets). */
  snapshot(): SavedWorkspace {
    const tabs: SavedTab[] = [];
    let active = 0;
    for (const tab of this.tabs) {
      const saved = saveTab(
        tab.layout,
        tab.focused,
        tab.zoomed,
        (k) => this.#sessions.get(k)?.target,
      );
      if (!saved) continue;
      if (tab.key === this.activeKey) active = tabs.length;
      tabs.push(saved);
    }
    return { version: 1, tabs, active };
  }

  /** Open a saved workspace's tabs after the current ones. `resolve` refreshes a target
   *  (current host label, trail) or drops it (host deleted). */
  restore(ws: SavedWorkspace, resolve: (t: TabTarget) => TabTarget | null): number {
    let opened = 0;
    let activate: string | null = null;
    ws.tabs.forEach((saved, i) => {
      const loaded = loadTab(saved, (target) => {
        const t = resolve(target);
        if (!t) return null;
        const s = new TerminalSession(t);
        this.#sessions.set(s.key, s);
        return s.key;
      });
      if (!loaded) return;
      const tab = new Tab(loaded.focused);
      tab.layout = loaded.layout;
      tab.focused = loaded.focused;
      tab.zoomed = loaded.zoomed;
      this.tabs.push(tab);
      opened++;
      if (i === ws.active || activate === null) activate = tab.key;
    });
    if (activate) this.activeKey = activate;
    return opened;
  }

  /** Move a tab, live, to a new window: output is held while the window opens, then the
   *  new window replays the scrollback and takes over the sessions. */
  tearOff(tab: Tab): Promise<boolean> {
    return this.#handOff(tab, (h) => commands.windowTearOff(h));
  }

  /** A tab dragged out of this window and dropped at a screen point (CSS pixels): it joins
   *  the Hopr window whose tab bar is there, or opens in a new window at that spot. */
  dropOutside(tab: Tab, x: number, y: number): Promise<boolean> {
    return this.#handOff(tab, (h) => commands.windowTabDrop(h, x, y));
  }

  /** Move a tab to position `to` in the strip. */
  moveTab(tab: Tab, to: number) {
    const from = this.tabs.indexOf(tab);
    const at = Math.max(0, Math.min(this.tabs.length - 1, to));
    if (from === -1 || from === at) return;
    this.tabs.splice(from, 1);
    this.tabs.splice(at, 0, tab);
  }

  /** Drop a whole tab beside a pane of another tab: its panes join that tab as a split. */
  mergeInto(tab: Tab, target: TerminalSession, side: Side) {
    const into = this.tabOf(target);
    if (!into || into === tab) return;
    // Its panes are drawn by the other tab's terminals from now on.
    for (const s of this.panesOf(tab)) s.carryScreen();
    into.layout = insert(into.layout, target.key, side, tab.layout);
    into.zoomed = null;
    into.focused = tab.focused;
    this.tabs.splice(this.tabs.indexOf(tab), 1);
    this.activeKey = into.key;
  }

  /** Hold a tab's output, package it (targets, live ids, scrollback) and give it to `send`;
   *  once another window has it, it leaves this one. */
  async #handOff(
    tab: Tab,
    send: (handoff: string) => Promise<{ status: "ok" } | { status: "error" }>,
  ): Promise<boolean> {
    const members = this.panesOf(tab);
    const live = members.filter((s) => s.id);
    for (const s of live) s.flushAcks();
    const held = await commands.sessionDetach(live.map((s) => s.id as string));
    if (held.status === "error") return false;
    // Let output already in flight to this window land before taking the snapshot.
    await new Promise((r) => setTimeout(r, 60));
    const byKey = new Map(members.map((s) => [s.key, s]));
    const saved = saveTab(tab.layout, tab.focused, tab.zoomed, (k) => byKey.get(k)?.target);
    if (!saved) return false;
    const keys = panes(tab.layout).filter((k) => byKey.has(k));
    const adopted = keys.map((k) => {
      const s = byKey.get(k);
      return s?.id ? { id: s.id, state: s.state, scrollback: s.serialize() } : null;
    });
    const res = await send(JSON.stringify({ tab: saved, adopted }));
    if (res.status === "error") {
      // Nothing moved: send the output back here.
      for (const s of live) void s.reattach();
      return false;
    }
    for (const s of members) {
      this.#sessions.delete(s.key);
      if (s.id) forgetSession(s.id);
    }
    this.#forget(tab, undefined);
    return true;
  }

  /** In a torn-off window: take over the tab handed to it. */
  adopt(handoff: string): boolean {
    let data: { tab: SavedTab; adopted: (Adopted | null)[] };
    try {
      data = JSON.parse(handoff);
    } catch {
      return false;
    }
    const loaded = loadTab(data.tab, (target, i) => {
      const s = new TerminalSession(target, null, data.adopted[i] ?? null);
      this.#sessions.set(s.key, s);
      return s.key;
    });
    if (!loaded) return false;
    const tab = new Tab(loaded.focused);
    tab.layout = loaded.layout;
    tab.focused = loaded.focused;
    tab.zoomed = loaded.zoomed;
    this.tabs.push(tab);
    this.activeKey = tab.key;
    return true;
  }

  #forget(tab: Tab, target: TabTarget | undefined) {
    const i = this.tabs.indexOf(tab);
    if (i === -1) return;
    this.tabs.splice(i, 1);
    if (target) {
      this.#closed.push(target);
      if (this.#closed.length > MAX_CLOSED) this.#closed.shift();
    }
    if (this.activeKey === tab.key) {
      this.activeKey = this.tabs[Math.min(i, this.tabs.length - 1)]?.key ?? null;
    }
  }
}

export const sessions = new SessionsStore();
setInFront((s) => sessions.active === s);
