<script lang="ts">
/** Files open in a local editor: each uploads on save until you stop editing it. */
import { PencilSimple, X } from "phosphor-svelte";
import { files } from "./files.svelte";
</script>

{#if files.edits.length}
  <div class="edits" aria-label="Editing locally">
    <span class="text-xs text-faint">Editing locally</span>
    {#each files.edits as e (e.id)}
      <span class="edit" title={e.remote}>
        <PencilSimple size={12} />
        <span class="max-w-40 truncate">{e.name}</span>
        <button type="button" aria-label="Stop editing {e.name}" title="Stop editing (deletes the local copy)" onclick={() => files.stopEdit(e.id)}>
          <X size={11} />
        </button>
      </span>
    {/each}
  </div>
{/if}

<style>
  .edits {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .edit {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 4px 2px 8px;
    font-size: 12px;
    color: var(--text);
    background: var(--surface-4);
    border-radius: 999px;
  }
  .edit button {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    color: var(--text-faint);
    border-radius: 999px;
  }
  .edit button:hover {
    color: var(--text);
    background: var(--surface-3);
  }
</style>
