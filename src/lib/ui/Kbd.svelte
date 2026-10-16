<script lang="ts">
import { formatShortcut } from "$lib/platform";

let { keys }: { keys: string } = $props();
</script>

<kbd
  class="rounded-sm border border-border-strong bg-surface-2 px-1.5 font-mono text-[11px] leading-[18px] text-muted"
  >{formatShortcut(keys)}</kbd
>
