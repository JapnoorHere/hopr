<script lang="ts">
/** What a tab drag will do, drawn over the window: the half of a pane it would take, and
 *  a chip with the tab's name following the pointer once it leaves the strip. */
import { TerminalWindow } from "phosphor-svelte";
import { fade } from "svelte/transition";
import { motion } from "$lib/platform";
import { tabDrag } from "./tabdrag.svelte";

const zone = $derived(tabDrag.target.kind === "pane" ? tabDrag.target.zone : null);
const away = $derived(tabDrag.tab !== null && tabDrag.target.kind !== "strip");
</script>

{#if tabDrag.tab}
  {#if zone}
    <div
      class="zone"
      style:left="{zone.left}px"
      style:top="{zone.top}px"
      style:width="{zone.width}px"
      style:height="{zone.height}px"
      transition:fade={{ duration: motion(120) }}
    ></div>
  {/if}
  {#if away}
    <div
      class="chip"
      style:transform="translate({tabDrag.x + 14}px, {tabDrag.y + 10}px)"
      transition:fade={{ duration: motion(100) }}
    >
      <TerminalWindow size={13} weight="light" />
      <span class="truncate">{tabDrag.label}</span>
      {#if zone}<span class="hint">Split</span>{:else if tabDrag.target.kind === "outside"}<span class="hint">New window</span>{/if}
    </div>
  {/if}
{/if}

<style>
  .zone {
    position: fixed;
    z-index: 60;
    pointer-events: none;
    border: 2px solid color-mix(in oklab, var(--accent) 80%, transparent);
    border-radius: var(--radius-lg);
    background: color-mix(in oklab, var(--accent) 16%, transparent);
    box-shadow: inset 0 0 24px color-mix(in oklab, var(--accent) 22%, transparent);
    /* Slides between halves and panes instead of jumping. */
    transition:
      left var(--dur) var(--ease-out),
      top var(--dur) var(--ease-out),
      width var(--dur) var(--ease-out),
      height var(--dur) var(--ease-out);
  }
  .chip {
    position: fixed;
    top: 0;
    left: 0;
    z-index: 61;
    display: flex;
    max-width: 260px;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    font-size: 12px;
    color: var(--text);
    pointer-events: none;
    background: var(--surface-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-pop);
  }
  .hint {
    padding: 0 5px;
    font-size: 10px;
    font-weight: 600;
    color: var(--accent-fg);
    background: var(--accent);
    border-radius: 4px;
  }
  @media (prefers-reduced-motion: reduce) {
    .zone {
      transition: none;
    }
  }
</style>
