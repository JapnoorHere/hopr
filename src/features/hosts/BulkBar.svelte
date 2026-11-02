<script lang="ts">
/** Actions for the selected hosts: move, tag, set environment, pin, export, delete. */
import type { SvelteSet } from "svelte/reactivity";
import type { Environment } from "$lib/ipc";
import { rise, sink } from "$lib/motion";
import { library } from "./library.svelte";

interface Props {
  selected: SvelteSet<string>;
  onexport: (ids: string[]) => void;
}

let { selected, onexport }: Props = $props();
let tag = $state("");

async function bulk(edit: Partial<Parameters<typeof library.bulkEdit>[0]>) {
  const res = await library.bulkEdit({
    ids: [...selected],
    moveGroup: false,
    groupId: null,
    addTags: [],
    removeTags: [],
    environment: null,
    favorite: null,
    ...edit,
  });
  if (!res.ok) library.flash(res.error.message, "error");
  else library.flash(`Updated ${res.data} host${res.data === 1 ? "" : "s"}`);
}

/** A select used as an action: run it, then show the prompt again. */
function take(e: Event): string {
  const el = e.currentTarget as HTMLSelectElement;
  const v = el.value;
  el.value = "";
  return v;
}

function moveTo(e: Event) {
  const v = take(e);
  if (v) void bulk({ moveGroup: true, groupId: v === "__none" ? null : v });
}

function setEnv(e: Event) {
  const v = take(e) as Environment | "";
  if (v) void bulk({ environment: v });
}

function addTag(e: SubmitEvent) {
  e.preventDefault();
  const tags = tag
    .split(",")
    .map((t) => t.trim())
    .filter(Boolean);
  if (!tags.length) return;
  tag = "";
  void bulk({ addTags: tags });
}

async function remove() {
  const ids = [...selected];
  selected.clear();
  await library.remove(ids);
}
</script>

<div class="bulk" role="toolbar" aria-label="Selected hosts" in:rise={{ y: 12 }} out:sink={{ y: 12 }}>
  <span class="text-sm text-fg">{selected.size} selected</span>
  <select class="input bulk-input" onchange={moveTo} aria-label="Move to group">
    <option value="">Move to…</option>
    <option value="__none">No group</option>
    {#each library.groups as g (g.id)}
      <option value={g.id}>{g.name}</option>
    {/each}
  </select>
  <form class="flex" onsubmit={addTag}>
    <input class="input bulk-input" bind:value={tag} placeholder="Add tag" aria-label="Add tag" spellcheck="false" />
  </form>
  <select class="input bulk-input" onchange={setEnv} aria-label="Set environment">
    <option value="">Environment…</option>
    <option value="none">None</option>
    <option value="dev">Dev</option>
    <option value="staging">Staging</option>
    <option value="prod">Prod</option>
  </select>
  <button type="button" class="btn-ghost" onclick={() => bulk({ favorite: true })}>Pin</button>
  <button type="button" class="btn-ghost" onclick={() => onexport([...selected])}>Export</button>
  <button type="button" class="btn-danger" onclick={remove}>Delete</button>
  <button type="button" class="btn-ghost" onclick={() => selected.clear()}>Clear</button>
</div>

<style>
  .bulk {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    background: var(--surface-2);
    border-top: 1px solid var(--border);
  }
  .bulk-input {
    width: auto;
    min-width: 120px;
    padding-top: 4px;
    padding-bottom: 4px;
  }
</style>
