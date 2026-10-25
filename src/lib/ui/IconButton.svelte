<script lang="ts">
import type { Snippet } from "svelte";
import Kbd from "./Kbd.svelte";

interface Props {
  label: string;
  shortcut?: string;
  active?: boolean;
  disabled?: boolean;
  tip?: "right" | "bottom" | "bottom-start" | "bottom-end" | "top";
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
  onpointerup={(e) => e.currentTarget.blur()}
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
      color var(--dur-fast) var(--ease-out),
      transform var(--dur-instant) var(--ease-out);
  }
  /* Press feedback: a quick dip, so every click is felt. */
  .btn:active:not(:disabled) {
    transform: scale(0.9);
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
  /* A mouse click drops focus (pointerup), so this ring only shows for the keyboard: a thin
     line inside the button rather than a thick box around it. */
  .btn:focus-visible {
    outline: 1.5px solid color-mix(in oklab, var(--accent) 75%, transparent);
    outline-offset: -1.5px;
    background: var(--surface-3);
    color: var(--text);
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
    translate: var(--tip-from, 0 3px);
    transition:
      opacity var(--dur-fast) var(--ease-out),
      translate var(--dur) var(--ease-spring);
  }
  .btn:hover .tip,
  .btn:focus-visible .tip {
    opacity: 1;
    translate: 0 0;
    transition-delay: 350ms;
  }
  [data-tip="right"] {
    --tip-from: -4px 0;
  }
  [data-tip="top"] {
    --tip-from: 0 -3px;
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
  [data-tip="bottom-start"] .tip {
    top: calc(100% + 6px);
    left: 0;
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
