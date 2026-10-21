<script lang="ts">
import { getCurrentWindow } from "@tauri-apps/api/window";
import { sessions } from "$features/terminal/session.svelte";
import { vault } from "$features/vault/vault.svelte";
import Workspace from "$features/workspace/Workspace.svelte";
import { commands, inTauri } from "$lib/ipc";
import Kbd from "$lib/ui/Kbd.svelte";

let error = $state<string | null>(null);
let hotkey = $state("Ctrl+`");

// The vault lives in the backend; re-read it whenever this window is shown.
$effect(() => {
  if (!inTauri) return;
  void commands.desktopPrefs().then((r) => {
    if (r.quickHotkey) hotkey = r.quickHotkey;
  });
  let unlisten: (() => void) | undefined;
  void getCurrentWindow()
    .onFocusChanged(({ payload: focused }) => {
      if (focused) void vault.refresh();
    })
    .then((u) => {
      unlisten = u;
    });
  return () => unlisten?.();
});

// One local shell, opened once the vault is unlocked and kept across show/hide.
$effect(() => {
  if (!vault.unlocked || sessions.tabs.length > 0) return;
  void commands.shellsList().then((r) => {
    if (r.status !== "ok" || !r.data[0]) {
      error = "No local shell found";
      return;
    }
    if (sessions.tabs.length > 0) return;
    const shell = r.data[0];
    sessions.open({ kind: "local", shellId: shell.id, label: shell.name });
  });
});

// When the shell exits, drop it; the next show starts a fresh one.
$effect(() => {
  const s = sessions.active;
  if (s && s.state.kind === "closed") void sessions.close(s);
});
</script>

<div class="quick flex h-full flex-col bg-bg text-fg">
  <header
    class="flex h-7 shrink-0 items-center gap-2 border-b border-border bg-surface-1 px-3 text-xs text-faint"
  >
    <span class="text-muted">Quick terminal</span>
    <span class="flex-1"></span>
    <span><Kbd keys={hotkey} /> to hide</span>
    <button type="button" class="link" onclick={() => commands.mainShow()}>Open Hopr</button>
  </header>
  <main class="relative flex min-h-0 flex-1">
    {#if !vault.unlocked}
      <div class="m-auto flex flex-col items-center gap-3 text-center">
        <p class="text-sm text-muted">Hopr is locked.</p>
        <button type="button" class="btn-primary" onclick={() => commands.mainShow()}>
          Unlock in Hopr
        </button>
      </div>
    {:else if error}
      <p class="m-auto text-sm text-danger">{error}</p>
    {:else}
      {#each sessions.tabs as tab (tab.key)}
        <Workspace {tab} visible={tab.key === sessions.activeKey} />
      {/each}
    {/if}
  </main>
</div>

<style>
  .quick {
    border: 1px solid var(--border-strong);
    border-top: 0;
  }
  .link {
    color: var(--text-muted);
  }
  .link:hover {
    color: var(--text);
  }
</style>
