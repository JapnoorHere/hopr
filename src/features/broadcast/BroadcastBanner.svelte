<script lang="ts">
import { Broadcast } from "phosphor-svelte";
import { broadcast } from "./broadcast.svelte";
</script>

{#if broadcast.pending}
  <div class="banner warn" role="alertdialog" aria-label="Confirm broadcast">
    <Broadcast size={16} />
    <span class="flex-1 text-sm">
      Broadcast to {broadcast.pending.length} sessions, including production? Everything you type goes to all of them.
    </span>
    <button type="button" class="btn-ghost" onclick={() => broadcast.confirm(false)}>Cancel</button>
    <button type="button" class="btn-danger" onclick={() => broadcast.confirm(true)}>Broadcast</button>
  </div>
{:else if broadcast.active}
  <div class="banner" role="status" aria-label="Broadcasting">
    <Broadcast size={16} weight="fill" />
    <span class="flex-1 text-sm">
      Broadcasting to {broadcast.targets.length} sessions{broadcast.hasProd ? " (includes production)" : ""}: typing
      goes to all of them.
    </span>
    <button type="button" class="btn-ghost" onclick={() => broadcast.stop()}>Stop</button>
  </div>
{/if}

<style>
  .banner {
    position: absolute;
    top: 8px;
    left: 50%;
    z-index: 25;
    display: flex;
    align-items: center;
    gap: 10px;
    width: min(640px, calc(100% - 32px));
    padding: 6px 8px 6px 12px;
    color: var(--text);
    background: var(--surface-3);
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    transform: translateX(-50%);
    box-shadow: var(--shadow-pop);
  }
  .banner.warn {
    border-color: var(--danger);
  }
</style>
