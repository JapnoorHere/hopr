<script lang="ts">
import { sessions } from "$features/terminal/session.svelte";
import { describeState } from "$features/terminal/state";
import { type AppInfo, commands, inTauri } from "$lib/ipc";
import { blurFade } from "$lib/motion";

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
  class="frame flex h-7 shrink-0 items-center gap-4 border-t border-border bg-surface-1 px-3 text-xs text-muted"
>
  <span class="flex min-w-0 items-center gap-2" role="status">
    <span
      class="dot size-1.5 shrink-0 rounded-full"
      class:ring-once={status?.tone === "live"}
      data-tone={status?.tone ?? "idle"}
    ></span>
    {#if active && status}
      {#key status.label}
        <span class="truncate" in:blurFade={{ y: 4, blur: 2, duration: 180 }}>{status.label}</span>
      {/key}
      <span class="tabular truncate text-faint">
        {active.address ?? `${active.origin} › ${active.label}`}
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
    transition: background var(--dur-slow) var(--ease-out);
  }
  .dot[data-tone="live"] {
    background: var(--success);
    color: var(--success);
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
