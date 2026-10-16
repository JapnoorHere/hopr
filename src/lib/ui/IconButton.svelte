<script lang="ts">
import type { Snippet } from "svelte";
import Kbd from "./Kbd.svelte";

interface Props {
  label: string;
  shortcut?: string;
  active?: boolean;
  disabled?: boolean;
  tip?: "right" | "bottom" | "bottom-end" | "top";
  size?: number;
  onclick?: () => void;
  children: Snippet;
}

let {
  label,
  shortcut,
  active = false,
  disabled = false,
  tip = "bottom",
  size = 32,
  onclick,
  children,
}: Props = $props();
</script>

<button
  type="button"
  class="btn"
  class:active
  aria-label={label}
  {disabled}
  {onclick}
  data-tip={tip}
  style:width="{size}px"
  style:height="{size}px"
>
  {@render children()}
  <span class="tip" role="tooltip">
    {label}
    {#if shortcut}<Kbd keys={shortcut} />{/if}
  </span>
</button>

<style>
  .btn {
    position: relative;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    transition:
      background var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out);
  }
  .btn:hover:not(:disabled) {
    background: var(--surface-3);
    color: var(--text);
  }
  .btn.active {
    color: var(--accent);
    background: color-mix(in oklab, var(--accent) 13%, transparent);
  }
  .btn:disabled {
    opacity: 0.4;
  }
  .tip {
    position: absolute;
    z-index: 50;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    white-space: nowrap;
    pointer-events: none;
    font-size: 12px;
    color: var(--text);
    background: var(--surface-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    opacity: 0;
    transition: opacity var(--dur-fast) var(--ease-out);
  }
  .btn:hover .tip,
  .btn:focus-visible .tip {
    opacity: 1;
    transition-delay: 350ms;
  }
  [data-tip="right"] .tip {
    left: calc(100% + 8px);
    top: 50%;
    transform: translateY(-50%);
  }
  [data-tip="bottom"] .tip {
    top: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
  }
  [data-tip="bottom-end"] .tip {
    top: calc(100% + 6px);
    right: 0;
  }
  [data-tip="top"] .tip {
    bottom: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
  }
</style>
