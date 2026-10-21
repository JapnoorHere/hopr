<script lang="ts">
import { updates } from "./updates.svelte";

const busy = $derived(updates.phase === "downloading" || updates.phase === "installing");
</script>

<div class="toast" role="status" aria-label="Update available">
  <span class="text-sm text-fg">
    {#if updates.phase === "downloading"}Downloading Hopr {updates.available?.version}…
    {:else if updates.phase === "installing"}Installing… Hopr will restart.
    {:else}Hopr {updates.available?.version} is available{/if}
  </span>
  {#if updates.message}<span class="text-xs text-danger">{updates.message}</span>{/if}
  <button type="button" class="btn-ghost" disabled={busy} onclick={() => (updates.dismissed = true)}>
    Later
  </button>
  <button type="button" class="btn-primary" disabled={busy} onclick={() => updates.install()}>
    Update and restart
  </button>
</div>

<style>
  .toast {
    position: fixed;
    right: 16px;
    bottom: 40px;
    z-index: 40;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 6px 6px 14px;
    background: var(--surface-3);
    border: 1px solid var(--border);
    border-left: 2px solid var(--accent);
    border-radius: var(--radius);
  }
</style>
