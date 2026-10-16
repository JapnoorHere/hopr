<script lang="ts">
import EmptyWorkspace from "$features/shell/EmptyWorkspace.svelte";
import Rail from "$features/shell/Rail.svelte";
import SettingsDrawer from "$features/shell/SettingsDrawer.svelte";
import StatusBar from "$features/shell/StatusBar.svelte";
import { shell } from "$features/shell/shell.state.svelte";
import TitleBar from "$features/shell/TitleBar.svelte";
import { sessions } from "$features/terminal/session.svelte";
import TerminalPane from "$features/terminal/TerminalPane.svelte";

function onkeydown(e: KeyboardEvent) {
  const mod = e.ctrlKey || e.metaKey;
  if (e.key === "Escape" && shell.closeTop()) {
    e.preventDefault();
  } else if (mod && e.key === ",") {
    e.preventDefault();
    shell.toggle("settings");
  } else if (mod && e.shiftKey && e.key.toLowerCase() === "n") {
    e.preventDefault();
    sessions.showNew();
  }
}
</script>

<svelte:window {onkeydown} />

<div class="flex h-full flex-col bg-bg text-fg">
  <TitleBar />
  <div class="flex min-h-0 flex-1">
    <Rail />
    <main class="relative flex min-w-0 flex-1">
      <!-- Every session stays mounted so background tabs keep their scrollback. -->
      {#each sessions.list as s (s.key)}
        <div class="absolute inset-0" class:invisible={s.key !== sessions.activeKey}>
          <TerminalPane session={s} active={s.key === sessions.activeKey} />
        </div>
      {/each}
      {#if !sessions.active}
        <EmptyWorkspace />
      {/if}
      {#if shell.drawer === "settings" && !shell.pinned}
        <SettingsDrawer />
      {/if}
    </main>
    {#if shell.drawer === "settings" && shell.pinned}
      <SettingsDrawer />
    {/if}
  </div>
  <StatusBar />
</div>
