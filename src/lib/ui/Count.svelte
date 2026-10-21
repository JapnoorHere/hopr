<script lang="ts">
import { cubicOut } from "svelte/easing";
import { Tween } from "svelte/motion";
import { motion } from "$lib/platform";

/** A number that rolls to its new value instead of flickering (status bar, counters, speeds). */
let {
  value,
  format = (n: number) => String(Math.round(n)),
  duration = 420,
}: { value: number; format?: (n: number) => string; duration?: number } = $props();

// svelte-ignore state_referenced_locally
const shown = new Tween(value, { easing: cubicOut });

$effect(() => {
  void shown.set(value, { duration: motion(duration) });
});
</script>

<span class="tabular">{format(shown.current)}</span>
