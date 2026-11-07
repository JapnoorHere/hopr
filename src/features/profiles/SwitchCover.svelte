<script lang="ts">
/**
 * While another profile opens: one calm screen over the window, so the reload behind it never
 * shows (no blank flash, no second start-up animation).
 */
import { fade } from "svelte/transition";
import HoprMark from "$lib/icons/HoprMark.svelte";
import { profileName, profiles } from "./profiles.svelte";

const target = $derived(profiles.list.find((p) => p.id === profiles.switching));
</script>

<div class="cover" role="status" aria-live="polite" in:fade={{ duration: 120 }}>
  <span class="text-accent"><HoprMark size={44} animated /></span>
  <p class="text-sm text-muted">Opening {target ? profileName(target) : "profile"}…</p>
</div>

<style>
  .cover {
    position: fixed;
    inset: 0;
    z-index: 300;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    background: var(--bg);
  }
</style>
