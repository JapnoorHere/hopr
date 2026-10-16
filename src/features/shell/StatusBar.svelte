<script lang="ts">
import { sessions } from "$features/terminal/session.svelte";
import { describeState } from "$features/terminal/state";
import { type AppInfo, commands, inTauri } from "$lib/ipc";

let info = $state<AppInfo | null>(null);
const active = $derived(sessions.active);
const status = $derived(active ? describeState(active.state) : null);

$effect(() => {
  if (!inTauri) return;
  commands.appInfo().then((i) => {
    info = i;
  });
});
</script>

<footer
  class="flex h-7 shrink-0 items-center gap-4 border-t border-border bg-surface-1 px-3 text-xs text-muted"
>
  <span class="flex min-w-0 items-center gap-2" role="status">
    <span class="dot size-1.5 shrink-0 rounded-full" data-tone={status?.tone ?? "idle"}></span>
    {#if active && status}
      <span class="truncate">{status.label}</span>
      <span class="tabular truncate text-faint">
        {active.target.username}@{active.target.host}:{active.target.port}
      </span>
    {:else}
      No active session
    {/if}
  </span>
  <span class="flex-1"></span>
  <span class="tabular text-faint">
    Hopr {info ? `${info.version}${info.debug ? " · dev" : ""}` : "web preview"}
  </span>
</footer>

<style>
  .dot {
    background: var(--text-faint);
  }
  .dot[data-tone="live"] {
    background: var(--success);
  }
  .dot[data-tone="busy"] {
    background: var(--accent);
  }
  .dot[data-tone="warn"] {
    background: var(--warning);
  }
  .dot[data-tone="down"] {
    background: var(--danger);
  }
</style>
