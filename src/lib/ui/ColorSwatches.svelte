<script lang="ts">
/** Pick one colour from a few: round swatches, the chosen one ringed and ticked. */
import { Check } from "phosphor-svelte";

interface Props {
  colors: readonly string[];
  value: string;
  onchange: (color: string) => void;
  /** Accessible name of the group. */
  label?: string;
}

let { colors, value, onchange, label = "Colour" }: Props = $props();
</script>

<div class="swatches" role="radiogroup" aria-label={label}>
  {#each colors as c (c)}
    <button
      type="button"
      role="radio"
      aria-checked={value === c}
      aria-label="Colour {c}"
      class:on={value === c}
      style:background={c}
      onclick={() => onchange(c)}
    >
      {#if value === c}<Check size={12} weight="bold" />{/if}
    </button>
  {/each}
</div>

<style>
  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  button {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    color: var(--bg);
    border-radius: 999px;
    box-shadow: inset 0 0 0 1px var(--border);
    transition: transform var(--dur-fast) var(--ease-spring);
  }
  button:hover {
    transform: scale(1.1);
  }
  /* The gap ring takes the colour of whatever the swatches sit on. */
  button.on {
    box-shadow:
      0 0 0 2px var(--swatch-gap, var(--surface-2)),
      0 0 0 4px var(--text);
  }
</style>
