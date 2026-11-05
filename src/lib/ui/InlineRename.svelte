<script lang="ts">
/** A name you can rename in place: the pencil (or a double-click) turns it into a field. */
import { PencilSimple } from "phosphor-svelte";

interface Props {
  value: string;
  /** What is being renamed, for screen readers ("group Production"). */
  label: string;
  /** Save the new name; resolves to an error message, or null when it worked. */
  onsave: (next: string) => Promise<string | null>;
}

let { value, label, onsave }: Props = $props();
let editing = $state(false);
let draft = $state("");
let error = $state<string | null>(null);

function start() {
  draft = value;
  error = null;
  editing = true;
}

async function save() {
  if (!editing) return;
  const next = draft.trim();
  if (!next || next === value) {
    editing = false;
    return;
  }
  error = await onsave(next);
  if (!error) editing = false;
}

function onkeydown(e: KeyboardEvent) {
  if (e.key === "Enter") {
    e.preventDefault();
    void save();
  } else if (e.key === "Escape") {
    // Cancel the rename only; whatever holds the name stays open.
    e.preventDefault();
    e.stopPropagation();
    editing = false;
  }
}

function focus(node: HTMLInputElement) {
  node.focus();
  node.select();
}
</script>

{#if editing}
  <input
    class="input rename"
    bind:value={draft}
    aria-label="Rename {label}"
    aria-invalid={!!error}
    title={error ?? undefined}
    spellcheck="false"
    {onkeydown}
    onblur={save}
    use:focus
  />
{:else}
  <span class="name" role="presentation" ondblclick={start}>
    <span class="truncate">{value}</span>
    <button type="button" class="pen" aria-label="Rename {label}" title="Rename" onclick={start}>
      <PencilSimple size={12} />
    </button>
  </span>
{/if}

<style>
  .name {
    display: inline-flex;
    min-width: 0;
    align-items: center;
    gap: 4px;
  }
  .pen {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    color: var(--text-faint);
    border-radius: 4px;
    opacity: 0;
    transition: opacity var(--dur-fast) var(--ease-out);
  }
  .name:hover .pen,
  .pen:focus-visible {
    opacity: 1;
  }
  .pen:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .rename {
    width: auto;
    min-width: 140px;
    padding-top: 2px;
    padding-bottom: 2px;
    font-size: inherit;
    text-transform: none;
    letter-spacing: normal;
  }
</style>
