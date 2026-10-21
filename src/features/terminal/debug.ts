/**
 * Dev-only hooks for the native E2E smoke test (scripts/e2e-smoke.mjs), which drives the
 * real WebView2 over the Chrome DevTools Protocol. Never bundled in production builds:
 * main.ts imports this only when import.meta.env.DEV is true.
 */

import { broadcast } from "$features/broadcast/broadcast.svelte";
import { grouprun } from "$features/grouprun/grouprun.svelte";
import { type HistoryTab, history } from "$features/history/history.svelte";
import { library } from "$features/hosts/library.svelte";
import { palette } from "$features/palette/palette-state.svelte";
import { files } from "$features/sftp/files.svelte";
import { transfers } from "$features/sftp/transfers.svelte";
import { snippets } from "$features/snippets/snippets.svelte";
import { syncStore } from "$features/sync/sync.svelte";
import { tunnels } from "$features/tunnels/tunnels.svelte";
import { vault } from "$features/vault/vault.svelte";
import { workspaces } from "$features/workspace/workspaces.svelte";
import { commands, type ExportFormat, type HostInput, type HostKeyDecision } from "$lib/ipc";
import { windowLabel } from "$lib/window";
import { prefs } from "./prefs.svelte";
import { sessions } from "./session.svelte";

export function installDebugHooks() {
  Object.assign(window, {
    __hopr: {
      vault: () => vault.status,
      /** Create (first run) or unlock the vault. Returns the recovery key when created. */
      async ensureUnlocked(password: string): Promise<string | null> {
        await vault.refresh();
        if (!vault.status?.exists) {
          const r = await vault.create(password);
          vault.showRecovery = false;
          return r.ok ? r.data : null;
        }
        if (!vault.unlocked) await vault.unlock(password, false);
        return null;
      },
      lock: () => vault.lock(),
      unlock: (password: string) => vault.unlock(password, false),
      connect(host: string, port: number, username: string, password: string | null) {
        sessions.open({ kind: "quick", host, port, username }, password);
      },
      async saveHost(input: HostInput) {
        const r = await library.save(null, input);
        return r.ok ? r.data : r.error;
      },
      connectHost(id: string) {
        const h = library.host(id);
        if (h) library.connect(h);
      },
      host: (id: string) => library.host(id) ?? null,
      hosts: () =>
        library.hosts.map((h) => ({ id: h.id, label: h.label, tags: h.tags, auth: h.auth })),
      // Plain copies: CDP serializes Svelte state proxies as `{}`.
      reach: (id: string) => {
        const r = library.reach[id];
        return r ? { ...r } : null;
      },
      ping: (ids: string[]) => library.ping(ids),
      keys: () => library.keys.map((k) => ({ id: k.id, label: k.label, publicKey: k.publicKey })),
      generateKey: async (label: string) => {
        const r = await commands.keyGenerate(label, "ed25519", "e2e");
        await library.refresh();
        return r.status === "ok" ? r.data.id : null;
      },
      exportHosts: async (format: ExportFormat, ids: string[]) => {
        const r = await library.exportHosts(format, ids);
        return r.ok ? r.data : null;
      },
      audit: async (limit = 50) => {
        const r = await commands.auditList(limit);
        return r.status === "ok" ? r.data : [];
      },
      setClipboardClear: (seconds: number) => commands.vaultSetClipboardClear(seconds),
      label: windowLabel,
      sessionIds: () => sessions.list.map((s) => s.id),
      bytesIn: () => sessions.list.map((s) => s.bytesIn),
      serialize: () => sessions.active?.serialize() ?? "",
      tearOff: () => (sessions.activeTab ? sessions.tearOff(sessions.activeTab) : false),
      split: (dir: "row" | "column") => sessions.split(dir),
      openLocal: async () => {
        const r = await commands.shellsList();
        const s = r.status === "ok" ? r.data[0] : undefined;
        if (s) sessions.open({ kind: "local", shellId: s.id, label: s.name });
        return !!s;
      },
      workspaces: async () => {
        await workspaces.refresh();
        return workspaces.list.map((w) => ({ ...w }));
      },
      saveWorkspace: async (name: string) => (await workspaces.save(name)).ok,
      openWorkspace: async (name: string) => {
        await workspaces.refresh();
        const w = workspaces.list.find((x) => x.name === name);
        return w ? workspaces.open(w) : 0;
      },
      restoreLast: () => workspaces.restoreLast(),
      offer: () => (workspaces.offer ? workspaces.offer.tabs.length : 0),
      filesOpen: (hostId: string, at: string | null = null) => files.open(hostId, at),
      filesGo: (path: string) => files.go(path),
      files: () => ({
        cwd: files.cwd,
        names: files.entries.map((e) => e.name),
        error: files.error,
        notice: files.notice,
      }),
      transfers: () => transfers.list.map((t) => ({ ...t })),
      setDual: (on: boolean) => {
        files.dual = on;
      },
      clash: () => (files.clash ? [...files.clash.names] : null),
      closeAll: async () => {
        for (const t of [...sessions.tabs]) await sessions.closeTab(t);
      },
      refreshLibrary: () => library.refresh(),
      answerHostKey: (d: HostKeyDecision) => sessions.active?.answerHostKey(d),
      answerPrompt: (responses: string[] | null, remember = false) =>
        sessions.active?.answerPrompt(responses, remember),
      state: () => sessions.active?.state ?? null,
      // Tail only: reading 10k lines of scrollback every poll would distort throughput numbers.
      screen: (lines = 200) => sessions.active?.snapshot(lines) ?? "",
      type(text: string) {
        sessions.active?.write(new TextEncoder().encode(text));
      },
      count: () => sessions.list.length,
      tabs: () => sessions.tabs.length,
      panes: () => (sessions.activeTab ? sessions.panesOf(sessions.activeTab).length : 0),
      focused: () => sessions.active?.key ?? null,
      zoomed: () => sessions.activeTab?.zoomed ?? null,
      paletteOpen: () => palette.open,
      fontSize: () => prefs.current.fontSize,
      hops: () => [...(sessions.active?.hops ?? [])],
      closeActive: () => (sessions.active ? sessions.close(sessions.active) : undefined),
      // Shell integration, history and recordings (4c).
      sessionInfo: () => {
        const s = sessions.active;
        if (!s) return null;
        return {
          id: s.id,
          cwd: s.cwd,
          integrated: s.integrated,
          recording: s.recording,
          hasPrompts: s.hasPrompts,
          viewportY: s.viewportY,
          lastCommand: s.lastCommand ? { ...s.lastCommand } : null,
        };
      },
      exitMarks: () =>
        [...document.querySelectorAll<HTMLElement>(".hopr-exit")].map((e) => e.dataset.exit ?? ""),
      jumpPrompt: (dir: -1 | 1) => sessions.active?.jumpPrompt(dir),
      toggleRecording: () => sessions.active?.toggleRecording() ?? "no session",
      history: async (tab: HistoryTab, query = "") => {
        history.query = query;
        history.host = null;
        await history.show(tab);
        if (tab === "commands") return history.commands.map((c) => ({ ...c }));
        if (tab === "connections") return history.connections.map((c) => ({ ...c }));
        return history.recordings.map((r) => ({ ...r }));
      },
      replay: (name: string | null) => {
        history.replaying = name;
      },
      player: () => (history.player ? { ...history.player } : null),
      tunnels: async () => {
        await tunnels.refresh();
        return tunnels.list.map((t) => ({ ...t, status: { ...tunnels.statusOf(t.id) } }));
      },
      tunnelStatus: (id: string) => ({ ...tunnels.statusOf(id) }),
      snippets: async () => {
        await snippets.refresh();
        return { list: snippets.list.map((s) => ({ ...s })), secrets: [...snippets.secrets] };
      },
      runSnippet: async (name: string) => {
        const s = snippets.list.find((x) => x.name === name);
        if (s) await snippets.run(s);
        return snippets.pending
          ? { fields: snippets.pending.fields.map((f) => f.key), confirm: snippets.pending.confirm }
          : null;
      },
      snippetNotice: () => ({ notice: snippets.notice, error: snippets.error }),
      broadcast: () => ({
        active: broadcast.active,
        targets: broadcast.targets.length,
        pending: broadcast.pending?.length ?? 0,
      }),
      broadcastTab: () => broadcast.startTab(),
      broadcastStop: () => broadcast.stop(),
      screens: (lines = 30) =>
        sessions.activeTab
          ? sessions.panesOf(sessions.activeTab).map((s) => s.snapshot(lines))
          : [],
      groupRun: async (hostIds: string[], command: string, parallel = 4, stopOnFail = false) => {
        grouprun.show(hostIds);
        grouprun.command = command;
        grouprun.parallel = parallel;
        grouprun.stopOnFail = stopOnFail;
        return grouprun.run();
      },
      groupResults: () => grouprun.results.map((r) => ({ ...r })),
      syncStatus: async () => {
        await syncStore.refresh();
        return syncStore.status
          ? { ...syncStore.status, devices: syncStore.status.devices.map((d) => ({ ...d })) }
          : null;
      },
      syncNow: async () => {
        const ok = await syncStore.now();
        return { ok, error: syncStore.error, last: syncStore.last ? { ...syncStore.last } : null };
      },
      closeGroupRun: () => {
        grouprun.open = false;
      },
      playerText: () => history.playerText?.() ?? "",
    },
  });
}
