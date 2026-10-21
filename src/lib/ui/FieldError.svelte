<script lang="ts">
import { WarningCircle } from "phosphor-svelte";
import { expand } from "$lib/motion";

/** A form field's problem, shown under it. Renders nothing when `message` is empty. */
let { message }: { message?: string | null } = $props();
</script>

{#if message}
  <p class="field-error" role="alert" transition:expand={{ duration: 160 }}>
    <WarningCircle size={13} weight="fill" class="shrink-0" />
    <span>{message}</span>
  </p>
{/if}

<style>
  .field-error {
    display: flex;
    align-items: flex-start;
    gap: 5px;
    font-size: 12px;
    line-height: 1.35;
    color: var(--danger);
  }
  .field-error :global(svg) {
    margin-top: 1px;
  }
</style>
