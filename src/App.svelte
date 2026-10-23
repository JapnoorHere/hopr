<script lang="ts">
import { getCurrentWindow } from "@tauri-apps/api/window";
import { untrack } from "svelte";
import BroadcastBanner from "$features/broadcast/BroadcastBanner.svelte";
import { grouprun } from "$features/grouprun/grouprun.svelte";
import RunOnHosts from "$features/grouprun/RunOnHosts.svelte";
import HistoryDrawer from "$features/history/HistoryDrawer.svelte";
import { history } from "$features/history/history.svelte";
import { historyPrefs } from "$features/history/prefs.svelte";
import Replay from "$features/history/Replay.svelte";
import HostEditor from "$features/hosts/HostEditor.svelte";
import HostsView from "$features/hosts/HostsView.svelte";
import ImportDrawer from "$features/hosts/ImportDrawer.svelte";
import { library } from "$features/hosts/library.svelte";
import DeployKeyDrawer from "$features/keys/DeployKeyDrawer.svelte";
import KeysDrawer from "$features/keys/KeysDrawer.svelte";
import Palette from "$features/palette/Palette.svelte";
import { palette } from "$features/palette/palette-state.svelte";
import AuditDrawer from "$features/settings/AuditDrawer.svelte";
import UpdateToast from "$features/settings/UpdateToast.svelte";
import { updates } from "$features/settings/updates.svelte";
import DualPane from "$features/sftp/DualPane.svelte";
import FilesDrawer from "$features/sftp/FilesDrawer.svelte";
import { files } from "$features/sftp/files.svelte";
import TransferPill from "$features/sftp/TransferPill.svelte";
import { transfers } from "$features/sftp/transfers.svelte";
import Rail from "$features/shell/Rail.svelte";
import SettingsDrawer from "$features/shell/SettingsDrawer.svelte";
import StatusBar from "$features/shell/StatusBar.svelte";
import { shell } from "$features/shell/shell.state.svelte";
import TitleBar from "$features/shell/TitleBar.svelte";
import SnippetsDrawer from "$features/snippets/SnippetsDrawer.svelte";
import { snippets } from "$features/snippets/snippets.svelte";
import { syncStore } from "$features/sync/sync.svelte";
import { sessions } from "$features/terminal/session.svelte";
import TunnelsDrawer from "$features/tunnels/TunnelsDrawer.svelte";
import { tunnels } from "$features/tunnels/tunnels.svelte";
import Onboarding from "$features/vault/Onboarding.svelte";
import UnlockScreen from "$features/vault/UnlockScreen.svelte";
import { vault } from "$features/vault/vault.svelte";
import { registerAppCommands } from "$features/workspace/commands";
import Workspace from "$features/workspace/Workspace.svelte";
import { workspaces } from "$features/workspace/workspaces.svelte";
import { i18n } from "$lib/i18n/i18n.svelte";
import { commands, events, inTauri } from "$lib/ipc";
import { matches, shortcuts } from "$lib/shortcuts";
import SelectMenu from "$lib/ui/SelectMenu.svelte";
import Toaster from "$lib/ui/Toaster.svelte";
import { isMainWindow, isTornOffWindow } from "$lib/window";

const onboarding = $derived(!!vault.status && (!vault.status.exists || vault.showRecovery));

$effect(() => registerAppCommands());

// Screen readers and hyphenation follow the chosen language.
$effect(() => {
  document.documentElement.lang = i18n.locale;
});

// Transfers keep running and reporting while the drawer is closed.
$effect(() => {
  if (vault.unlocked) void transfers.init();
});
$effect(() => {
  if (vault.unlocked) void historyPrefs.load();
});
// Sync on unlock and every few minutes; stop when locked (main window only).
$effect(() => {
  if (!isMainWindow) return;
  if (vault.unlocked) void syncStore.start();
  else syncStore.stop();
});
// Tunnels marked "start when Hopr unlocks" (main window only; they run in the backend).
$effect(() => {
  if (vault.unlocked && isMainWindow) void tunnels.autostart();
});

// Main window: open the startup workspace or offer the last session, then autosave.
$effect(() => {
  if (isMainWindow && vault.unlocked) void workspaces.startup();
});
$effect(() => {
  if (!isMainWindow) return;
  workspaces.changed(JSON.stringify(sessions.snapshot()));
});

// Each saved workspace is a palette command.
$effect(() =>
  shortcuts.register(
    workspaces.list
      .filter((w) => !w.autosave)
      .map((w) => ({
        id: `workspace.open.${w.id}`,
        title: `Open workspace: ${w.name}`,
        group: "Workspaces",
        run: () => {
          workspaces.open(w);
        },
      })),
  ),
);

// Each snippet is a palette command that runs it in the focused terminal.
$effect(() => {
  if (vault.unlocked) void snippets.refresh();
});
$effect(() =>
  shortcuts.register(
    snippets.list.map((s) => ({
      id: `snippet.${s.id}`,
      title: `Snippet: ${s.folder ? `${s.folder} / ` : ""}${s.name}`,
      group: "Snippets",
      when: () => sessions.active?.state.kind === "ready",
      run: () => {
        shell.drawer = "snippets";
        void snippets.run(s);
      },
    })),
  ),
);

// Main window: check for updates a little after start, then every 6 hours (never forced).
$effect(() => {
  if (!isMainWindow || !inTauri) return;
  const first = setTimeout(() => void updates.check(true), 15_000);
  const every = setInterval(() => void updates.check(true), 6 * 60 * 60 * 1000);
  return () => {
    clearTimeout(first);
    clearInterval(every);
  };
});

// Main window: tray "Recent hosts" and picking one there.
$effect(() => {
  if (!isMainWindow || !inTauri || !vault.unlocked) return;
  const recent = library.hosts
    .filter((h) => h.lastConnectedAt)
    .sort((a, b) => (b.lastConnectedAt ?? "").localeCompare(a.lastConnectedAt ?? ""))
    .slice(0, 8)
    .map((h) => ({ id: h.id, label: h.label }));
  void commands.traySetRecent(recent);
});
$effect(() => {
  if (!isMainWindow || !inTauri) return;
  let unlisten: (() => void) | undefined;
  void events.trayConnectEvent
    .listen(async (e) => {
      if (!library.loaded) await library.refresh();
      const h = library.host(e.payload.hostId);
      if (h && vault.unlocked) library.connect(h);
    })
    .then((u) => {
      unlisten = u;
    });
  return () => unlisten?.();
});

// Torn-off window: take over the tab handed to it; closing the window closes its tabs,
// and the window goes away with its last tab.
let adopted = $state(false);
$effect(() => {
  if (!isTornOffWindow || !inTauri) return;
  void commands.windowHandoffTake().then((h) => {
    if (h) sessions.adopt(h);
    adopted = true;
  });
  let unlisten: (() => void) | undefined;
  void getCurrentWindow()
    .onCloseRequested(async () => {
      await Promise.all(sessions.tabs.map((t) => sessions.closeTab(t)));
    })
    .then((u) => {
      unlisten = u;
    });
  return () => unlisten?.();
});
$effect(() => {
  if (isTornOffWindow && adopted && sessions.tabs.length === 0) void getCurrentWindow().close();
});

// A drawer opened while the host editor is up (Manage keys) stacks on top of it.
$effect(() => {
  const open = shell.drawer !== null;
  untrack(() => {
    shell.overEditor = open && (library.editing !== null || library.deploying !== null);
  });
});

function inTerminal(e: KeyboardEvent): boolean {
  return e.target instanceof Element && e.target.closest(".xterm") !== null;
}

function onkeydown(e: KeyboardEvent) {
  if (!vault.unlocked || onboarding) return;
  if (e.key === "Escape") {
    if (palette.open) palette.close();
    else if (history.replaying) history.replaying = null;
    else if (grouprun.open) grouprun.open = false;
    else if (shell.overEditor && shell.closeTop()) shell.overEditor = false;
    else if (library.editing !== null) library.editing = null;
    else if (library.deploying) library.deploying = null;
    else if (!shell.closeTop()) return;
    e.preventDefault();
    return;
  }
  // Ctrl+K belongs to the shell (kill line) inside a terminal; elsewhere it opens the palette.
  if (matches(e, "Ctrl+K") && !inTerminal(e)) {
    e.preventDefault();
    palette.show();
    return;
  }
  const cmd = shortcuts.find(e);
  if (cmd) {
    e.preventDefault();
    cmd.run();
  }
}
</script>

<svelte:window {onkeydown} />

{#snippet drawer()}
  {#if shell.drawer === "settings"}
    <SettingsDrawer />
  {:else if shell.drawer === "keys"}
    <KeysDrawer />
  {:else if shell.drawer === "import"}
    <ImportDrawer />
  {:else if shell.drawer === "files"}
    <FilesDrawer />
  {:else if shell.drawer === "audit"}
    <AuditDrawer />
  {:else if shell.drawer === "history"}
    <HistoryDrawer />
  {:else if shell.drawer === "tunnels"}
    <TunnelsDrawer />
  {:else if shell.drawer === "snippets"}
    <SnippetsDrawer />
  {/if}
{/snippet}

<div class="app-root flex h-full flex-col bg-bg text-fg">
  <TitleBar />
  <div class="relative flex min-h-0 flex-1">
    {#if onboarding}
      <Onboarding />
    {:else if vault.status}
      {#if vault.everUnlocked}
        <Rail />
        <main class="relative flex min-w-0 flex-1">
          <!-- Every session stays mounted so background tabs keep their scrollback. -->
          {#each sessions.tabs as tab (tab.key)}
            <Workspace {tab} visible={tab.key === sessions.activeKey} />
          {/each}
          {#if files.dual}<DualPane />{/if}
          {#if grouprun.open}<RunOnHosts />{/if}
          <BroadcastBanner />
          {#if history.replaying}{#key history.replaying}<Replay name={history.replaying} />{/key}{/if}
          {#if !sessions.activeTab}
            <HostsView />
          {/if}
        </main>
        <!-- Side panels dock to the right of the content, full height. -->
        {#if library.editing !== null}
          {#key library.editing}
            <HostEditor id={library.editing === "new" ? null : library.editing} />
          {/key}
        {:else if library.deploying}
          <DeployKeyDrawer hostId={library.deploying.hostId} keyId={library.deploying.keyId} />
        {/if}
        {@render drawer()}
      {/if}
      {#if !vault.unlocked}
        <UnlockScreen />
      {/if}
    {/if}
  </div>
  {#if vault.unlocked && !onboarding}<StatusBar />{/if}
  {#if palette.open && vault.unlocked}<Palette />{/if}
  {#if isMainWindow && updates.available && !updates.dismissed}<UpdateToast />{/if}
  {#if vault.unlocked && !onboarding}<TransferPill />{/if}
  <Toaster />
  <SelectMenu />
</div>
