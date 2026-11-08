<script lang="ts">
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CaretDown, GearSix, LockSimple, Plus } from "phosphor-svelte";
import { sessions } from "$features/terminal/session.svelte";
import TabStrip from "$features/terminal/TabStrip.svelte";
import { vault } from "$features/vault/vault.svelte";
import {
  localShells,
  openLocalShell,
  quickShell,
  registerAppCommands,
  setQuickShell,
} from "$features/workspace/commands";
import Workspace from "$features/workspace/Workspace.svelte";
import HoprMark from "$lib/icons/HoprMark.svelte";
import { commands, inTauri, type ShellProfile } from "$lib/ipc";
import { shortcuts } from "$lib/shortcuts";
import IconButton from "$lib/ui/IconButton.svelte";
import Kbd from "$lib/ui/Kbd.svelte";
import Menu from "$lib/ui/Menu.svelte";
import SettingsDrawer from "./SettingsDrawer.svelte";
import { shell } from "./shell.state.svelte";
import WindowControls from "./WindowControls.svelte";

let error = $state<string | null>(null);
let hotkey = $state("Ctrl+`");
let shells = $state<ShellProfile[]>([]);
let picking = $state(false);
/** The shell new tabs open: the one picked last. */
let chosen = $state(quickShell());
/** A tab has been open: closing the last one then closes the window. */
let started = $state(false);

// Tabs, panes and terminal keys, as in the main window (with the user's own bindings).
$effect(() => registerAppCommands());

// The vault lives in the backend; re-read it whenever this window is shown.
$effect(() => {
  if (!inTauri) return;
  void commands.desktopPrefs().then((r) => {
    if (r.quickHotkey) hotkey = r.quickHotkey;
  });
  void localShells().then((list) => {
    shells = list;
  });
  const win = getCurrentWindow();
  const unlisten: (() => void)[] = [];
  void win
    .onFocusChanged(({ payload: focused }) => {
      if (focused) void vault.refresh();
    })
    .then((u) => unlisten.push(u));
  // Closing the window ends its shells (hiding with the hotkey keeps them).
  void win
    .onCloseRequested(async () => {
      await Promise.all(sessions.tabs.map((t) => sessions.closeTab(t)));
    })
    .then((u) => unlisten.push(u));
  return () => {
    for (const u of unlisten) u();
  };
});

// Settings closes when Hopr locks (and can't open while it's locked).
$effect(() => {
  if (!vault.unlocked && shell.drawer === "settings") shell.closeTop();
});

// The first tab opens straight away (local shells don't need Hopr unlocked); closing the last
// one closes the window.
$effect(() => {
  if (sessions.tabs.length > 0) {
    started = true;
    return;
  }
  if (started) {
    if (inTauri) void getCurrentWindow().close();
    return;
  }
  void openLocalShell(quickShell()).then((ok) => {
    if (!ok) error = "No local shell found";
  });
});

// A shell that exits closes its pane (and its tab with its last pane).
$effect(() => {
  for (const tab of sessions.tabs) {
    for (const s of sessions.panesOf(tab)) {
      if (s.state.kind === "closed") void sessions.close(s);
    }
  }
});

function open(shell: ShellProfile) {
  setQuickShell(shell.id);
  chosen = shell.id;
  picking = false;
  void openLocalShell(shell.id);
}

function onkeydown(e: KeyboardEvent) {
  const cmd = shortcuts.find(e);
  if (cmd) {
    e.preventDefault();
    cmd.run();
  }
}

const preferred = $derived(shells.find((s) => s.id === chosen) ?? shells[0]);
</script>

<svelte:window {onkeydown} />

{#snippet newTab()}
  <div class="mb-1 ml-1 flex items-center">
    <IconButton
      label={preferred ? `New ${preferred.name} tab` : "New tab"}
      shortcut="Ctrl+Shift+T"
      size={26}
      tip="bottom-start"
      onclick={() => void openLocalShell(chosen)}
    >
      <Plus size={14} />
    </IconButton>
    {#if shells.length > 1}
      <Menu open={picking} onopenchange={(o) => (picking = o)} label="Open a shell" align="start" width="220px">
        {#snippet trigger(props)}
          <button type="button" class="caret" aria-label="Choose a shell" title="Choose a shell" {...props}>
            <CaretDown size={11} />
          </button>
        {/snippet}
        {#each shells as shell (shell.id)}
          <button type="button" role="menuitem" class="menu-item" onclick={() => open(shell)}>
            <span class="flex-1 truncate">{shell.name}</span>
            {#if shell.id === preferred?.id}<span class="text-[10px] text-faint">default</span>{/if}
          </button>
        {/each}
      </Menu>
    {/if}
  </div>
{/snippet}

<div class="flex h-full flex-col bg-bg text-fg">
  <header
    class="flex h-10 shrink-0 items-stretch border-b border-border bg-surface-1 text-xs text-faint"
    data-tauri-drag-region
  >
    <div class="grid w-12 shrink-0 place-items-center text-accent" data-tauri-drag-region title="Hopr quick terminal">
      <HoprMark size={20} />
    </div>
    <TabStrip tearOff={false} {newTab} />
    <div class="min-w-0 flex-1" data-tauri-drag-region></div>
    <span class="flex shrink-0 items-center gap-1 px-3" data-tauri-drag-region>
      <Kbd keys={hotkey} /> to hide
    </span>
    <button
      type="button"
      class="link flex shrink-0 items-center gap-1.5 px-3"
      title={vault.unlocked ? undefined : "Hopr is locked; these shells keep running"}
      onclick={() => commands.mainShow()}
    >
      {#if vault.unlocked}Open Hopr{:else}<LockSimple size={13} /> Unlock Hopr{/if}
    </button>
    <!-- Some settings need the vault: Settings is there while Hopr is unlocked. -->
    {#if vault.unlocked}
      <div class="flex shrink-0 items-center pr-1">
        <IconButton label="Settings" shortcut="Ctrl+," size={28} tip="bottom-end" onclick={() => shell.toggle("settings")}>
          <GearSix size={15} />
        </IconButton>
      </div>
    {/if}
    <WindowControls />
  </header>
  <!-- Locking Hopr never touches the shells here: they stay mounted and keep running. -->
  <main class="relative flex min-h-0 flex-1">
    {#if error}
      <p class="m-auto text-sm text-danger">{error}</p>
    {:else}
      {#each sessions.tabs as tab (tab.key)}
        <Workspace {tab} visible={tab.key === sessions.activeKey} />
      {/each}
    {/if}
  </main>
</div>

{#if shell.drawer === "settings"}<SettingsDrawer />{/if}

<style>
  .link {
    color: var(--text-muted);
  }
  .link:hover {
    color: var(--text);
  }
  .caret {
    display: grid;
    place-items: center;
    width: 16px;
    height: 26px;
    border-radius: var(--radius);
    color: var(--text-faint);
  }
  .caret:hover,
  .caret[aria-expanded="true"] {
    background: var(--surface-2);
    color: var(--text);
  }
</style>
