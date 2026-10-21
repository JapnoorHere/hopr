<script lang="ts">
import { files } from "./files.svelte";
</script>

{#if files.clash}
  <div class="clash" role="alertdialog" aria-label="Already exists">
    <span class="flex-1 text-sm text-fg">
      {files.clash.names.length === 1
        ? `${files.clash.names[0]} already exists there.`
        : `${files.clash.names.length} items already exist there.`}
      <span class="text-faint">Applies to all of them.</span>
    </span>
    <button type="button" class="btn-ghost" onclick={() => files.answerClash(null)}>Cancel</button>
    <button type="button" class="btn-ghost" onclick={() => files.answerClash("skip")}>Skip</button>
    <button type="button" class="btn-ghost" onclick={() => files.answerClash("rename")}>Keep both</button>
    <button type="button" class="btn-primary" onclick={() => files.answerClash("overwrite")}>Replace</button>
  </div>
{/if}

<style>
  .clash {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 6px 6px 6px 12px;
    background: var(--surface-4);
    border-left: 2px solid var(--warning);
    border-radius: var(--radius);
  }
</style>
