<script module lang="ts">
import type { Component } from "svelte";

export interface SegmentOption<V extends string = string> {
  id: V;
  label: string;
  icon?: Component<{ size?: number }>;
  /** Shown in the danger colour when chosen (e.g. a production environment). */
  danger?: boolean;
}
</script>

<script lang="ts" generics="T extends string">
import { slidingPill } from "$lib/motion";

interface Props {
  /** Accessible name of the group. */
  label: string;
  options: readonly SegmentOption<T>[];
  value: T;
  onchange: (value: T) => void;
  /** Sized to its content (toolbars) instead of filling the row (forms, settings). */
  compact?: boolean;
}

let { label, options, value, onchange, compact = false }: Props = $props();
</script>

<!-- One choice from a few: a radio group with a pill that slides to the chosen option. -->
<div class="segmented" class:compact role="radiogroup" aria-label={label} use:slidingPill>
  {#each options as o (o.id)}
    <button
      type="button"
      role="radio"
      aria-checked={value === o.id}
      class:on={value === o.id}
      class:danger={o.danger}
      title={compact ? o.label : undefined}
      onclick={() => onchange(o.id)}
    >
      {#if o.icon}<o.icon size={14} />{/if}
      {o.label}
    </button>
  {/each}
</div>

<style>
  .segmented {
    display: flex;
    padding: 2px;
    background: color-mix(in oklab, var(--text) 4%, transparent);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  button {
    display: inline-flex;
    flex: 1;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 4px 0;
    font-size: 12px;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
  }
  button:hover,
  button.on {
    color: var(--text);
  }
  button.on.danger {
    color: var(--danger);
  }
  .compact {
    display: inline-flex;
    border-radius: var(--radius-sm);
  }
  .compact button {
    flex: none;
    height: 24px;
    padding: 0 9px;
    border-radius: calc(var(--radius-sm) - 2px);
  }
</style>
