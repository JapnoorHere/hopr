<script lang="ts">
import { WarningCircle } from "phosphor-svelte";
import { fade } from "svelte/transition";
import { motion } from "$lib/platform";
import { sessions, type TerminalSession } from "./session.svelte";
import { describeState, isBusy } from "./state";
import Terminal from "./Terminal.svelte";

let { session, active }: { session: TerminalSession; active: boolean } = $props();

const info = $derived(describeState(session.state));
const busy = $derived(isBusy(session.state));
const failed = $derived(session.state.kind === "failed" ? session.state.error : null);
const ended = $derived(session.state.kind === "closed");

function onStripKey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.preventDefault();
    void sessions.close(session);
  }
}
</script>

<div class="relative h-full w-full bg-bg py-2 pl-3">
  <Terminal {session} {active} />

  {#if busy}
    <div class="overlay" transition:fade={{ duration: motion(120) }}>
      <div class="card w-80 items-center text-center">
        <div class="flex items-center gap-2 font-mono text-sm" aria-hidden="true">
          <span class="text-muted">laptop</span>
          <span class="text-faint">›</span>
          <span class="hop-live text-accent">{session.label}</span>
        </div>
        <p class="text-sm text-fg" role="status">{info.label}</p>
        <p class="tabular text-xs text-faint">
          {session.target.username}@{session.target.host}:{session.target.port}
        </p>
        <button type="button" class="btn-ghost" onclick={() => sessions.close(session)}>Cancel</button>
      </div>
    </div>
  {:else if failed}
    <div class="overlay" transition:fade={{ duration: motion(120) }}>
      <div class="card w-96" role="alert">
        <div class="flex items-start gap-3">
          <span class="mt-0.5 text-danger"><WarningCircle size={20} /></span>
          <div class="min-w-0 flex-1">
            <p class="text-sm font-medium text-fg">{failed.message}</p>
            <p class="tabular mt-0.5 text-xs text-faint">
              {session.target.username}@{session.target.host}:{session.target.port}
            </p>
          </div>
        </div>
        {#if failed.detail}
          <details class="text-xs text-muted">
            <summary class="cursor-default select-none">Technical details</summary>
            <pre class="mt-2 font-mono break-all whitespace-pre-wrap select-text text-faint">{failed.detail}</pre>
          </details>
        {/if}
        <div class="flex justify-end gap-2">
          <button type="button" class="btn-ghost" onclick={() => sessions.close(session)}>Close</button>
          <button type="button" class="btn-ghost" onclick={() => sessions.showNew(session.target)}>
            Edit connection
          </button>
          {#if session.canRetry}
            <button type="button" class="btn-primary" onclick={() => sessions.retry(session)}>Retry</button>
          {/if}
        </div>
      </div>
    </div>
  {:else if ended}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="strip" class:down={info.tone === "down"} onkeydown={onStripKey}>
      <span class="flex-1 text-sm text-fg">{info.label}</span>
      <button type="button" class="btn-ghost" onclick={() => sessions.close(session)}>
        Close <span class="text-faint">Esc</span>
      </button>
      <!-- svelte-ignore a11y_autofocus -->
      <button type="button" class="btn-primary" autofocus onclick={() => sessions.retry(session)}>
        Reconnect <span class="opacity-70">↵</span>
      </button>
    </div>
  {/if}
</div>

<style>
  .overlay {
    position: absolute;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    background: color-mix(in oklab, var(--bg) 70%, transparent);
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 20px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  .hop-live {
    animation: breathe 1.4s var(--ease-out) infinite;
  }
  @keyframes breathe {
    50% {
      opacity: 0.45;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .hop-live {
      animation: none;
    }
  }
  .strip {
    position: absolute;
    left: 12px;
    right: 12px;
    bottom: 12px;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 8px 14px;
    background: var(--surface-3);
    border: 1px solid var(--border);
    border-left: 2px solid var(--text-faint);
    border-radius: var(--radius);
  }
  .strip.down {
    border-left-color: var(--danger);
  }
  :global(.btn-ghost),
  :global(.btn-primary) {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    font-size: 13px;
    border-radius: var(--radius-sm);
    transition: background var(--dur-fast) var(--ease-out);
  }
  :global(.btn-ghost) {
    color: var(--text-muted);
  }
  :global(.btn-ghost:hover) {
    color: var(--text);
    background: var(--surface-3);
  }
  :global(.btn-primary) {
    color: var(--accent-fg);
    background: var(--accent);
    font-weight: 500;
  }
  :global(.btn-primary:hover) {
    background: color-mix(in oklab, var(--accent) 88%, var(--text));
  }
  :global(.btn-primary:disabled) {
    opacity: 0.5;
  }
</style>
