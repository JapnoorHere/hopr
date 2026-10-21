<script lang="ts">
import { chordOf, shortcuts } from "$lib/shortcuts";
import Kbd from "$lib/ui/Kbd.svelte";

let version = $state(0);
let recording = $state<string | null>(null);
let filter = $state("");

const rows = $derived.by(() => {
  void version;
  const q = filter.toLowerCase();
  return shortcuts
    .all()
    .filter((c) => !q || c.title.toLowerCase().includes(q))
    .map((c) => ({ c, keys: shortcuts.keysOf(c), custom: shortcuts.isCustom(c.id) }));
});
const conflicts = $derived.by(() => {
  void version;
  return new Set(shortcuts.conflicts().flatMap(([, ids]) => ids));
});

function record(e: KeyboardEvent, id: string) {
  e.preventDefault();
  e.stopPropagation();
  if (e.key === "Escape") {
    recording = null;
    return;
  }
  if (e.key === "Backspace") {
    shortcuts.setKeys(id, []);
  } else {
    const chord = chordOf(e);
    if (!chord) return; // only a modifier so far
    shortcuts.setKeys(id, [chord]);
  }
  recording = null;
  version++;
}

function reset(id: string) {
  shortcuts.setKeys(id, null);
  version++;
}
</script>

<section class="mt-5 flex flex-col gap-2">
  <h3 class="label-caps text-faint">Keyboard</h3>
  <input class="input" bind:value={filter} placeholder="Filter commands" aria-label="Filter commands" />
  <p class="text-xs text-faint">Click a shortcut, then press the new keys. Backspace unbinds, Esc cancels.</p>
  <div class="flex flex-col">
    {#each rows as { c, keys, custom } (c.id)}
      <div class="flex items-center gap-2 py-1">
        <span class="min-w-0 flex-1 truncate text-sm text-fg">{c.title}</span>
        {#if conflicts.has(c.id)}<span class="text-xs text-danger" title="Another command uses the same keys">conflict</span>{/if}
        {#if recording === c.id}
          <!-- svelte-ignore a11y_autofocus -->
          <button type="button" class="rec" autofocus onkeydown={(e) => record(e, c.id)} onblur={() => (recording = null)}>
            Press keys…
          </button>
        {:else}
          <button type="button" class="key" onclick={() => (recording = c.id)} aria-label="Change shortcut for {c.title}">
            {#if keys.length}<Kbd keys={keys[0] ?? ""} />{:else}<span class="text-xs text-faint">Unbound</span>{/if}
          </button>
        {/if}
        {#if custom}
          <button type="button" class="btn-ghost px-2 py-0.5 text-xs" onclick={() => reset(c.id)}>Reset</button>
        {/if}
      </div>
    {/each}
  </div>
</section>

<style>
  .key,
  .rec {
    min-width: 96px;
    padding: 2px 6px;
    text-align: right;
    border-radius: var(--radius-sm);
  }
  .key:hover {
    background: var(--surface-3);
  }
  .rec {
    font-size: 12px;
    color: var(--accent);
    background: var(--surface-3);
    outline: 1px solid var(--accent);
  }
</style>
