<script lang="ts">
/** An on/off switch. Put it inside a <label> with its text, or give it `label`. */
interface Props {
  checked: boolean;
  onchange: (checked: boolean) => void;
  /** Accessible name, when there is no surrounding label. */
  label?: string;
  disabled?: boolean;
}

let { checked, onchange, label, disabled = false }: Props = $props();
</script>

<button
  type="button"
  role="switch"
  class="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span></span>
</button>

<style>
  .switch {
    position: relative;
    width: 32px;
    height: 18px;
    flex-shrink: 0;
    background: var(--surface-3);
    border-radius: 999px;
    transition: background var(--dur-fast) var(--ease-out);
  }
  .switch span {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    background: var(--text-muted);
    border-radius: 50%;
    transition:
      transform var(--dur-fast) var(--ease-out),
      background var(--dur-fast) var(--ease-out);
  }
  .switch[aria-checked="true"] {
    background: color-mix(in oklab, var(--accent) 40%, transparent);
  }
  .switch[aria-checked="true"] span {
    background: var(--accent);
    transform: translateX(14px);
  }
  .switch:disabled {
    opacity: 0.5;
  }
</style>
