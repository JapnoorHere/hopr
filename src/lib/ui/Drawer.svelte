<script lang="ts">
import { PushPin, PushPinSlash, X } from "phosphor-svelte";
import type { Snippet } from "svelte";
import { fly } from "svelte/transition";
import { motion } from "$lib/platform";
import IconButton from "./IconButton.svelte";

interface Props {
  title: string;
  pinned: boolean;
  onclose: () => void;
  onpin: () => void;
  children: Snippet;
}

let { title, pinned, onclose, onpin, children }: Props = $props();
</script>

<aside class="drawer" class:pinned aria-label={title} transition:fly={{ x: 16, duration: motion(180) }}>
  <header class="flex items-center gap-1 py-2 pr-2 pl-4">
    <h2 class="flex-1 text-sm font-medium">{title}</h2>
    {#if !pinned}<span class="mr-1 text-xs text-faint">Esc to close</span>{/if}
    <IconButton label={pinned ? "Unpin" : "Pin to side"} size={28} tip="bottom-end" onclick={onpin}>
      {#if pinned}<PushPinSlash size={15} />{:else}<PushPin size={15} />{/if}
    </IconButton>
    <IconButton label="Close" shortcut="Esc" size={28} tip="bottom-end" onclick={onclose}>
      <X size={15} />
    </IconButton>
  </header>
  <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
    {@render children()}
  </div>
</aside>

<style>
  .drawer {
    position: absolute;
    top: 8px;
    right: 8px;
    bottom: 8px;
    z-index: 20;
    display: flex;
    flex-direction: column;
    width: 340px;
    background: color-mix(in oklab, var(--surface-3) 84%, transparent);
    backdrop-filter: blur(var(--blur));
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  .drawer.pinned {
    position: relative;
    inset: auto;
    flex-shrink: 0;
    background: var(--surface-1);
    backdrop-filter: none;
    border-width: 0 0 0 1px;
    border-radius: 0;
  }
</style>
