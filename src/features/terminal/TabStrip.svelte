<script lang="ts">
import { Plus, TerminalWindow, X } from "phosphor-svelte";
import IconButton from "$lib/ui/IconButton.svelte";
import { sessions } from "./session.svelte";
import { describeState } from "./state";
</script>

<div class="flex min-w-0 items-end gap-0.5 pl-1" role="tablist" aria-label="Sessions">
  {#each sessions.list as s (s.key)}
    {@const info = describeState(s.state)}
    {@const active = s.key === sessions.activeKey}
    <div class="tab" class:active data-tone={info.tone} role="presentation" title={info.label}>
      <button
        type="button"
        role="tab"
        aria-selected={active}
        class="flex min-w-0 items-center gap-1.5 py-1.5 pr-1 pl-3"
        onclick={() => sessions.activate(s.key)}
      >
        <TerminalWindow size={14} weight="light" class="shrink-0 text-faint" />
        <span class="truncate text-xs">
          <span class="text-faint">laptop</span><span class="mx-1 text-faint">›</span><span
            class={active ? "text-fg" : "text-muted"}>{s.label}</span
          >
        </span>
      </button>
      <button type="button" class="close" aria-label="Close {s.label}" onclick={() => sessions.close(s)}>
        <X size={12} />
      </button>
    </div>
  {/each}
  <div class="mb-1 ml-1">
    <IconButton label="New connection" shortcut="Ctrl+Shift+N" size={26} onclick={() => sessions.showNew()}>
      <Plus size={14} />
    </IconButton>
  </div>
</div>

<style>
  .tab {
    position: relative;
    display: flex;
    align-items: center;
    max-width: 240px;
    min-width: 0;
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    transition: background var(--dur-fast) var(--ease-out);
  }
  .tab:hover {
    background: var(--surface-2);
  }
  .tab.active {
    background: var(--bg);
  }
  /* Status as light (§8.4 #1): a glow on the bottom edge, never a badge dot. */
  .tab::after {
    content: "";
    position: absolute;
    left: 8px;
    right: 8px;
    bottom: 0;
    height: 2px;
    border-radius: 2px;
    background: transparent;
  }
  .tab[data-tone="live"]::after {
    background: var(--success);
    box-shadow: 0 0 8px color-mix(in oklab, var(--success) 60%, transparent);
  }
  .tab[data-tone="busy"]::after {
    background: var(--accent);
    animation: pulse 1.4s var(--ease-out) infinite;
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
  @media (prefers-reduced-motion: reduce) {
    .tab::after {
      animation: none !important;
    }
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
