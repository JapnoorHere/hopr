<script lang="ts">
import type { ISearchOptions, SearchAddon } from "@xterm/addon-search";
import { ArrowDown, ArrowUp, X } from "phosphor-svelte";
import { rise, sink } from "$lib/motion";

interface Props {
  addon: SearchAddon;
  decorations: NonNullable<ISearchOptions["decorations"]>;
  onclose: () => void;
}

let { addon, decorations, onclose }: Props = $props();

let query = $state("");
let caseSensitive = $state(false);
let wholeWord = $state(false);
let regex = $state(false);
let count = $state<{ index: number; total: number } | null>(null);
let input: HTMLInputElement | undefined = $state();

const opts = $derived<ISearchOptions>({ caseSensitive, wholeWord, regex, decorations });

$effect(() => {
  input?.focus();
  const sub = addon.onDidChangeResults(({ resultIndex, resultCount }) => {
    count = { index: resultIndex, total: resultCount };
  });
  return () => sub.dispose();
});

// Re-run the search as the query or options change.
$effect(() => {
  if (!query) {
    addon.clearDecorations();
    count = null;
    return;
  }
  try {
    addon.findNext(query, { ...opts, incremental: true });
  } catch {
    count = { index: -1, total: 0 }; // invalid regex
  }
});

function step(back: boolean) {
  if (!query) return;
  if (back) addon.findPrevious(query, opts);
  else addon.findNext(query, opts);
}

function onkeydown(e: KeyboardEvent) {
  if (e.key === "Enter") {
    e.preventDefault();
    step(e.shiftKey);
  } else if (e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    onclose();
  }
}
</script>

<div class="bar" role="search" in:rise={{ y: -8 }} out:sink={{ y: -8, duration: 120 }}>
  <input
    bind:this={input}
    bind:value={query}
    {onkeydown}
    class="min-w-0 flex-1 bg-transparent text-sm text-fg outline-none placeholder:text-faint"
    placeholder="Find in output"
    spellcheck="false"
    aria-label="Find in terminal output"
  />
  <span class="tabular shrink-0 text-xs text-faint" aria-live="polite">
    {#if count && query}{count.total === 0 ? "No results" : `${count.index + 1} / ${count.total}`}{/if}
  </span>
  <button type="button" class="toggle" class:on={caseSensitive} aria-pressed={caseSensitive} title="Match case" onclick={() => (caseSensitive = !caseSensitive)}>Aa</button>
  <button type="button" class="toggle" class:on={wholeWord} aria-pressed={wholeWord} title="Whole word" onclick={() => (wholeWord = !wholeWord)}>W</button>
  <button type="button" class="toggle" class:on={regex} aria-pressed={regex} title="Regular expression" onclick={() => (regex = !regex)}>.*</button>
  <button type="button" class="toggle" title="Previous (Shift+Enter)" aria-label="Previous match" onclick={() => step(true)}><ArrowUp size={13} /></button>
  <button type="button" class="toggle" title="Next (Enter)" aria-label="Next match" onclick={() => step(false)}><ArrowDown size={13} /></button>
  <button type="button" class="toggle" title="Close (Esc)" aria-label="Close search" onclick={onclose}><X size={13} /></button>
</div>

<style>
  .bar {
    position: absolute;
    top: 6px;
    right: 18px;
    z-index: 6;
    display: flex;
    width: min(420px, calc(100% - 36px));
    align-items: center;
    gap: 4px;
    padding: 4px 4px 4px 10px;
    background: var(--surface-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
  }
  .toggle {
    display: grid;
    min-width: 24px;
    height: 24px;
    place-items: center;
    padding: 0 4px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
    border-radius: 4px;
  }
  .toggle:hover {
    color: var(--text);
    background: var(--surface-4);
  }
  .toggle.on {
    color: var(--accent);
    background: color-mix(in oklab, var(--accent) 16%, transparent);
  }
</style>
