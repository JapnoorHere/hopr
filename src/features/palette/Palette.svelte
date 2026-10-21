<script lang="ts">
import { ArrowRight, HardDrives, Lightning, TerminalWindow } from "phosphor-svelte";
import { tick } from "svelte";
import { flip } from "svelte/animate";
import { fade } from "svelte/transition";
import { library } from "$features/hosts/library.svelte";
import { sessions } from "$features/terminal/session.svelte";
import { parseTarget } from "$features/terminal/target";
import { fuzzy } from "$lib/fuzzy";
import { blurFade, sink, stagger } from "$lib/motion";
import { motion } from "$lib/platform";
import { shortcuts } from "$lib/shortcuts";
import Kbd from "$lib/ui/Kbd.svelte";
import { load, record, save, score } from "./frecency";
import { palette } from "./palette-state.svelte";

interface Item {
  id: string;
  kind: "command" | "host" | "tab" | "quick";
  title: string;
  detail?: string;
  keys?: string;
  run: () => void;
}

let query = $state(palette.initial);
let index = $state(0);
let input: HTMLInputElement | undefined = $state();
let uses = $state(load());
let list: HTMLDivElement | undefined = $state();
// The selection is one highlight that glides between rows.
let hi = $state<{ y: number; h: number } | null>(null);
$effect(() => {
  void index;
  void results.length;
  void tick().then(() => {
    const row = list?.querySelector<HTMLElement>(`[data-index="${index}"]`);
    hi = row ? { y: row.offsetTop, h: row.offsetHeight } : null;
  });
});

const mode = $derived(query.startsWith(">") ? "commands" : query.startsWith("@") ? "hosts" : "all");
const text = $derived(mode === "all" ? query : query.slice(1));

function allItems(): Item[] {
  const items: Item[] = [];
  if (mode !== "hosts") {
    for (const c of shortcuts.all()) {
      if (c.id === "palette") continue;
      items.push({
        id: `cmd:${c.id}`,
        kind: "command",
        title: c.title,
        detail: c.group,
        keys: shortcuts.keysOf(c)[0],
        run: c.run,
      });
    }
  }
  if (mode !== "commands") {
    for (const h of library.hosts) {
      items.push({
        id: `host:${h.id}`,
        kind: "host",
        title: h.label,
        detail: `${h.username}@${h.hostname}${h.port === 22 ? "" : `:${h.port}`}`,
        run: () => library.connect(h),
      });
    }
    sessions.tabs.forEach((t, i) => {
      const s = sessions.session(t.focused);
      if (!s) return;
      items.push({
        id: `tab:${t.key}`,
        kind: "tab",
        title: s.label,
        detail: `Open tab ${i + 1}`,
        run: () => sessions.activate(t.key),
      });
    });
  }
  return items;
}

const results = $derived.by(() => {
  const items = allItems();
  const ranked = items
    .map((it) => {
      const m = fuzzy(text, `${it.title} ${it.detail ?? ""}`);
      return m ? { it, rank: m.score + score(uses[it.id]) * 0.5 } : null;
    })
    .filter((x): x is { it: Item; rank: number } => x !== null)
    .sort((a, b) => b.rank - a.rank)
    .map((x) => x.it)
    .slice(0, 50);
  // A typed target that isn't a saved host becomes a quick-connect result.
  const target = mode !== "commands" ? parseTarget(text) : null;
  if (target?.username && text.includes("@")) {
    ranked.unshift({
      id: "quick",
      kind: "quick",
      title: `Connect to ${text.trim()}`,
      detail: "Quick connect",
      run: () =>
        sessions.open({
          kind: "quick",
          host: target.host,
          port: target.port ?? 22,
          username: target.username ?? "",
        }),
    });
  }
  return ranked;
});

$effect(() => {
  void results.length;
  index = 0;
});

$effect(() => {
  input?.focus();
  void library.refresh();
});

function choose(item: Item | undefined) {
  if (!item) return;
  if (item.id !== "quick") {
    uses = record(uses, item.id);
    save(uses);
  }
  palette.close();
  item.run();
}

function onkeydown(e: KeyboardEvent) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    index = Math.min(results.length - 1, index + 1);
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    index = Math.max(0, index - 1);
  } else if (e.key === "Enter") {
    e.preventDefault();
    choose(results[index]);
  } else if (e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    palette.close();
  } else {
    return;
  }
  list?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest" });
}
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" transition:fade={{ duration: motion(100) }} onclick={() => palette.close()}>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    class="box"
    role="dialog"
    aria-label="Command palette"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    in:blurFade={{ y: -10, scale: 0.98, blur: 4, duration: 200 }}
    out:sink={{ y: -6, scale: 0.98, duration: 120 }}
  >
    <input
      bind:this={input}
      bind:value={query}
      {onkeydown}
      class="w-full bg-transparent px-4 py-3 text-base text-fg outline-none placeholder:text-faint"
      placeholder="Search commands, hosts and tabs. Use > for commands, @ for hosts, or type user@host"
      spellcheck="false"
      aria-label="Command palette"
      aria-controls="palette-results"
      aria-activedescendant={results[index] ? `palette-${index}` : undefined}
    />
    <div id="palette-results" class="relative max-h-96 overflow-y-auto border-t border-border p-1" role="listbox" bind:this={list}>
      {#if hi && results.length}
        <span class="hi" style:transform="translateY({hi.y}px)" style:height="{hi.h}px" aria-hidden="true"></span>
      {/if}
      {#each results as item, i (item.id)}
        <button
          type="button"
          id="palette-{i}"
          data-index={i}
          role="option"
          aria-selected={i === index}
          class="row"
          class:sel={i === index}
          onmousemove={() => (index = i)}
          onclick={() => choose(item)}
          in:blurFade={{ delay: stagger(i, 18, 8), y: 4, blur: 0, duration: 160 }}
          animate:flip={{ duration: motion(160) }}
        >
          <span class="shrink-0 text-faint">
            {#if item.kind === "host"}<HardDrives size={15} />{:else if item.kind === "tab"}<TerminalWindow size={15} />{:else if item.kind === "quick"}<ArrowRight size={15} />{:else}<Lightning size={15} />{/if}
          </span>
          <span class="min-w-0 flex-1 truncate text-sm text-fg">{item.title}</span>
          {#if item.detail}<span class="truncate text-xs text-faint">{item.detail}</span>{/if}
          {#if item.keys}<Kbd keys={item.keys} />{/if}
        </button>
      {:else}
        <p class="px-3 py-6 text-center text-sm text-muted">Nothing matches “{text}”.</p>
      {/each}
    </div>
    <footer class="flex gap-4 border-t border-border px-4 py-2 text-xs text-faint">
      <span>↑↓ to move</span><span>Enter to run</span><span>Esc to close</span>
    </footer>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: flex;
    justify-content: center;
    padding-top: 12vh;
    background: color-mix(in oklab, var(--bg) 40%, transparent);
  }
  .box {
    width: min(640px, calc(100vw - 32px));
    height: fit-content;
    overflow: hidden;
    background: color-mix(in oklab, var(--surface-4) 92%, transparent);
    backdrop-filter: blur(var(--blur));
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
  }
  .row {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    text-align: left;
    border-radius: var(--radius-sm);
  }
  .row {
    position: relative;
    z-index: 1;
  }
  .hi {
    position: absolute;
    top: 0;
    left: 4px;
    right: 4px;
    border-radius: var(--radius-sm);
    background: var(--surface-3);
    box-shadow: inset 2px 0 0 var(--accent);
    transition:
      transform var(--dur) var(--ease-spring),
      height var(--dur-fast) var(--ease-out);
    pointer-events: none;
  }
</style>
