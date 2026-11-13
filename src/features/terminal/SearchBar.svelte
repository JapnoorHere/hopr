<script lang="ts">
import { ArrowDown, ArrowUp, X } from "phosphor-svelte";
import { untrack } from "svelte";
import type { SearchHighlight } from "$lib/design/highlight";
import { rise, sink } from "$lib/motion";
import type { FastSearch, FindOptions } from "./fast-search";

interface Props {
  addon: FastSearch;
  decorations: SearchHighlight;
  /** Most matches highlighted at once; past it the count shows "N+". */
  limit: number;
  /** What to search for on open (the selection, or the last search). */
  initial: string;
  /** The query as it changes, so the next open can start from it. */
  onquery: (query: string) => void;
  onclose: () => void;
}

let { addon, decorations, limit, initial, onquery, onclose }: Props = $props();

/** Typing pauses this long before the scrollback is searched again. */
const TYPING_MS = 60;

// Read once: the bar is remounted ({#key}) to start from new text.
let query = $state(untrack(() => initial));
let caseSensitive = $state(false);
let wholeWord = $state(false);
let regex = $state(false);
let count = $state<{ index: number; total: number } | null>(null);
let invalid = $state(false);
let input: HTMLInputElement | undefined = $state();

const opts = $derived<FindOptions>({ caseSensitive, wholeWord, regex, decorations });
const missing = $derived(!!query && (invalid || count?.total === 0));
const label = $derived.by(() => {
  if (!query || !count) return "";
  if (invalid) return "Invalid pattern";
  if (count.total === 0) return "No results";
  if (count.index < 0) return `${count.total}`;
  return `${count.index + 1} / ${count.total}`;
});

$effect(() => {
  input?.focus();
  input?.select();
  const sub = addon.onDidChangeResults(({ resultIndex, resultCount }) => {
    count = { index: resultIndex, total: resultCount };
  });
  return () => sub.dispose();
});

function run(): void {
  invalid = false;
  if (!query) {
    addon.clearDecorations();
    count = null;
    return;
  }
  try {
    addon.search(query, opts);
  } catch {
    invalid = true; // an unfinished regular expression
    count = { index: -1, total: 0 };
  }
}

// Search as the query or options change: at once for the first character and option
// toggles, otherwise when typing pauses. The scan itself runs in small slices.
let timer: ReturnType<typeof setTimeout> | undefined;
let searched = "";
$effect(() => {
  const q = query;
  void opts;
  onquery(q);
  clearTimeout(timer);
  if (q.length <= 1 || q === searched) {
    searched = q;
    run();
    return;
  }
  timer = setTimeout(() => {
    searched = q;
    run();
  }, TYPING_MS);
  return () => clearTimeout(timer);
});

function step(back: boolean) {
  if (!query) return;
  clearTimeout(timer);
  invalid = false;
  try {
    if (back) addon.findPrevious(query, opts);
    else addon.findNext(query, opts);
  } catch {
    invalid = true;
  }
}

function onkeydown(e: KeyboardEvent) {
  if (e.key === "Enter" || e.key === "F3") {
    e.preventDefault();
    step(e.shiftKey);
  } else if (e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    onclose();
  }
}
</script>

<div class="bar" class:missing role="search" in:rise={{ y: -8 }} out:sink={{ y: -8, duration: 120 }}>
  <input
    bind:this={input}
    bind:value={query}
    {onkeydown}
    class="min-w-0 flex-1 bg-transparent text-sm text-fg outline-none placeholder:text-faint"
    placeholder="Find in output"
    spellcheck="false"
    aria-label="Find in terminal output"
    aria-invalid={missing}
  />
  <span class="count tabular shrink-0 text-xs" aria-live="polite">{label}</span>
  <button type="button" class="toggle" class:on={caseSensitive} aria-pressed={caseSensitive} title="Match case" onclick={() => (caseSensitive = !caseSensitive)}>Aa</button>
  <button type="button" class="toggle" class:on={wholeWord} aria-pressed={wholeWord} title="Whole word" onclick={() => (wholeWord = !wholeWord)}>W</button>
  <button type="button" class="toggle" class:on={regex} aria-pressed={regex} title="Regular expression" onclick={() => (regex = !regex)}>.*</button>
  <span class="sep" aria-hidden="true"></span>
  <button type="button" class="toggle" title="Previous (Shift+Enter)" aria-label="Previous match" disabled={!count?.total} onclick={() => step(true)}><ArrowUp size={13} /></button>
  <button type="button" class="toggle" title="Next (Enter)" aria-label="Next match" disabled={!count?.total} onclick={() => step(false)}><ArrowDown size={13} /></button>
  <button type="button" class="toggle" title="Close (Esc)" aria-label="Close search" onclick={onclose}><X size={13} /></button>
</div>

<style>
  .bar {
    position: absolute;
    top: 6px;
    right: 18px;
    z-index: 6;
    display: flex;
    width: min(440px, calc(100% - 36px));
    align-items: center;
    gap: 4px;
    padding: 4px 4px 4px 10px;
    background: var(--surface-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-pop);
    transition: border-color var(--dur-fast) var(--ease-out);
  }
  .bar:focus-within {
    border-color: color-mix(in oklab, var(--accent) 55%, var(--border-strong));
  }
  .bar.missing {
    border-color: color-mix(in oklab, var(--danger) 70%, transparent);
  }
  .count {
    min-width: 52px;
    text-align: right;
    color: var(--text-faint);
  }
  .missing .count {
    color: var(--danger);
  }
  .sep {
    width: 1px;
    height: 16px;
    margin: 0 2px;
    background: var(--border);
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
  .toggle:hover:not(:disabled) {
    color: var(--text);
    background: var(--surface-4);
  }
  .toggle:disabled {
    opacity: 0.4;
  }
  .toggle.on {
    color: var(--accent);
    background: color-mix(in oklab, var(--accent) 16%, transparent);
  }
</style>
