<script lang="ts">
/** The open preview: text or an image, with edit and download at hand. */
import { files } from "./files.svelte";
</script>

{#if files.preview}
  {@const p = files.preview}
  <div class="overlay" role="dialog" aria-label="Preview">
    <div class="flex items-center gap-2">
      <h3 class="min-w-0 flex-1 truncate text-sm font-medium text-fg">{p.entry.name}</h3>
      <button type="button" class="btn-ghost" onclick={() => files.edit(p.entry)}>Edit locally</button>
      <button type="button" class="btn-ghost" onclick={() => files.download([p.entry.path])}>Download</button>
      <button type="button" class="btn-ghost" onclick={() => (files.preview = null)}>Close</button>
    </div>
    {#if p.data.kind === "text"}
      <pre class="code">{p.data.text}</pre>
      {#if p.data.truncated}<p class="text-xs text-faint">Showing the first 256 KB.</p>{/if}
    {:else if p.data.kind === "image"}
      <img class="max-h-full max-w-full self-center" src={p.data.dataUrl} alt={p.entry.name} />
    {:else}
      <p class="text-sm text-muted">No preview for this file. Download it or edit it locally.</p>
    {/if}
  </div>
{/if}

<style>
  .overlay {
    position: absolute;
    inset: 48px 12px 12px;
    z-index: 25;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px;
    background: var(--surface-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  .code {
    min-height: 0;
    flex: 1;
    overflow: auto;
    padding: 10px;
    font-family: var(--font-mono);
    font-size: 12px;
    white-space: pre;
    color: var(--text);
    background: var(--bg);
    border-radius: var(--radius-sm);
  }
</style>
