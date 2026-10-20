<script lang="ts">
import { ArrowSquareOut, Plus, TerminalWindow, X } from "phosphor-svelte";
import { flip } from "svelte/animate";
import { cubicOut } from "svelte/easing";
import type { TransitionConfig } from "svelte/transition";
import { crumbs } from "$features/hosts/chain";
import { inTauri } from "$lib/ipc";
import { wiggle } from "$lib/motion";
import { motion, motionLevel } from "$lib/platform";

/** A background tab wiggles when its bell rings. */
function ringing(node: HTMLElement, _at: number) {
  return {
    update(at: number) {
      if (at && !node.classList.contains("active")) wiggle(node);
    },
  };
}

import IconButton from "$lib/ui/IconButton.svelte";
import { sessions } from "./session.svelte";
import { describeState } from "./state";

// Tabs with a session to show, so each tab is a direct child of the keyed list (flip).
const rows = $derived(
  sessions.tabs.flatMap((tab) => {
    const s = sessions.session(tab.focused);
    return s ? [{ tab, s }] : [];
  }),
);

/** A tab grows to its width when it opens and collapses when it closes. */
function grow(node: HTMLElement, { out = false } = {}): TransitionConfig {
  const level = motionLevel();
  if (level === "off") return { duration: 0 };
  const width = node.offsetWidth;
  if (level === "reduced") return { duration: 100, css: (t) => `opacity: ${t}` };
  return {
    duration: out ? 140 : 200,
    easing: cubicOut,
    css: (t) => `max-width: ${t * width}px; opacity: ${t}; overflow: hidden`,
  };
}
</script>

<div class="flex min-w-0 items-center gap-1 pl-1" role="tablist" aria-label="Sessions">
  {#each rows as { tab, s } (tab.key)}
    {@const count = sessions.panesOf(tab).length}
    {@const info = describeState(s.state)}
    {@const parts = crumbs([s.origin, ...s.hops])}
    {@const active = tab.key === sessions.activeKey}
    <div
      class="tab"
      class:active
      class:prod={s.environment === "prod"}
      data-tone={info.tone}
      role="presentation"
      title={info.label}
      in:grow
      out:grow={{ out: true }}
      animate:flip={{ duration: motion(200) }}
      use:ringing={s.bellAt}
    >
      <button
        type="button"
        role="tab"
        aria-selected={active}
        class="flex min-w-0 items-center gap-1.5 py-1.5 pr-1 pl-3"
        onclick={() => sessions.activate(tab.key)}
      >
        <TerminalWindow size={14} weight="light" class="shrink-0 text-faint" />
        <span class="truncate text-xs">
          {#each parts as part, i (i)}
            {#if i > 0}<span class="mx-1 text-faint">›</span>{/if}
            {#if i === parts.length - 1}
              <span class={active ? "text-fg" : "text-muted"}>{part}</span>
            {:else}
              <span class="text-faint">{part}</span>
            {/if}
          {/each}
        </span>
        {#if count > 1}<span class="tabular shrink-0 rounded px-1 text-[10px] text-faint" title="{count} panes">{count}</span>{/if}
        {#if sessions.panesOf(tab).some((p) => p.recording)}
          <span class="rec" title="Recording (output only)" aria-label="Recording">REC</span>
        {/if}
      </button>
      {#if inTauri}
        <button
          type="button"
          class="close"
          aria-label="Move {s.label} to a new window"
          title="Move to a new window (Ctrl+Shift+M)"
          onclick={() => sessions.tearOff(tab)}
        >
          <ArrowSquareOut size={12} />
        </button>
      {/if}
      <button type="button" class="close" aria-label="Close {s.label}" onclick={() => sessions.closeTab(tab)}>
        <X size={12} />
      </button>
    </div>
  {/each}
  <div class="mb-1 ml-1">
    <IconButton label="New tab" shortcut="Ctrl+Shift+T" size={26} onclick={() => sessions.showHome()}>
      <Plus size={14} />
    </IconButton>
  </div>
</div>

<style>
  .rec {
    flex-shrink: 0;
    padding: 0 4px;
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--danger);
    border: 1px solid currentColor;
    border-radius: 3px;
  }
  .tab {
    position: relative;
    display: flex;
    align-items: center;
    max-width: 240px;
    min-width: 0;
    /* Tabs are pills (terminal revamp T2). */
    border-radius: var(--radius);
    border: 1px solid transparent;
    transition:
      background var(--dur-fast) var(--ease-out),
      border-color var(--dur-fast) var(--ease-out);
  }
  .tab:hover {
    background: var(--surface-2);
  }
  .tab.active {
    background: var(--bg);
    border-color: var(--border);
  }
  .tab.prod {
    box-shadow: inset 2px 0 0 var(--danger);
  }
  /* Status as light (§8.4 #1): a glow on the bottom edge, never a badge dot. */
  .tab::after {
    content: "";
    position: absolute;
    left: 10px;
    right: 10px;
    bottom: 1px;
    height: 2px;
    border-radius: 2px;
    background: transparent;
  }
  /* Connected: the line sweeps in from the left once. */
  .tab[data-tone="live"]::after {
    background: var(--success);
    box-shadow: 0 0 8px color-mix(in oklab, var(--success) 60%, transparent);
    transform-origin: left;
    animation: tab-sweep var(--dur-story) var(--ease-out) 1;
  }
  /* Busy: a light runs along the line. */
  .tab[data-tone="busy"]::after {
    background: linear-gradient(90deg, transparent, var(--accent), transparent) 0 0 / 60% 100% no-repeat;
    animation: tab-run 1.1s var(--ease-in-out) infinite;
  }
  @keyframes tab-sweep {
    from {
      transform: scaleX(0);
    }
  }
  @keyframes tab-run {
    from {
      background-position: -150% 0;
    }
    to {
      background-position: 250% 0;
    }
  }
  .tab[data-tone="warn"]::after {
    background: var(--warning);
    animation: pulse 1.6s var(--ease-out) infinite;
  }
  .tab[data-tone="down"]::after {
    background: var(--danger);
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
  :global(:root[data-motion="reduced"]) .tab::after {
    animation: none !important;
  }
  :global(:root[data-motion="reduced"]) .tab[data-tone="busy"]::after {
    background: var(--accent);
  }
  .close {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    margin-right: 6px;
    color: var(--text-faint);
    border-radius: 4px;
    opacity: 0;
  }
  .tab:hover .close,
  .tab.active .close,
  .close:focus-visible {
    opacity: 1;
  }
  .close:hover {
    color: var(--text);
    background: var(--surface-3);
  }
</style>
