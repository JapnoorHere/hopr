<script lang="ts" module>
// Floating drawers stack in the order they open: one opened from inside another (Manage
// keys from the host editor) sits on top of it instead of underneath.
let open = $state(0);
</script>

<script lang="ts">
import { PushPin, PushPinSlash, X } from "phosphor-svelte";
import { onDestroy, type Snippet } from "svelte";
import { blurFade, sink, springIn } from "$lib/motion";
import IconButton from "./IconButton.svelte";

interface Props {
  title: string;
  pinned: boolean;
  onclose: () => void;
  onpin?: () => void;
  /** CSS width (floating only). */
  width?: string;
  children: Snippet;
}

let { title, pinned, onclose, onpin, width = "340px", children }: Props = $props();
const layer = 20 + open++;
onDestroy(() => {
  open--;
});
// A drawer with another floating one opened over it steps back (dimmed, slightly smaller).
const beneath = $derived(!pinned && layer - 20 < open - 1);
</script>

<aside
  class="drawer"
  class:pinned
  class:beneath
  aria-label={title}
  style:width
  style:z-index={pinned ? undefined : layer}
  in:springIn={{ x: 28 }}
  out:sink={{ x: 20, duration: 160 }}
>
  <header class="flex items-center gap-1 py-2 pr-2 pl-4">
    <h2 class="flex-1 text-sm font-medium">{title}</h2>
    {#if !pinned}<span class="mr-1 text-xs text-faint">Esc to close</span>{/if}
    {#if onpin}<IconButton label={pinned ? "Unpin" : "Pin to side"} size={28} tip="bottom-end" onclick={onpin}>
      {#if pinned}<PushPinSlash size={15} />{:else}<PushPin size={15} />{/if}
    </IconButton>{/if}
    <IconButton label="Close" shortcut="Esc" size={28} tip="bottom-end" onclick={onclose}>
      <X size={15} />
    </IconButton>
  </header>
  <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-4" in:blurFade={{ delay: 70, y: 6, blur: 2, duration: 220 }}>
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
    background: color-mix(in oklab, var(--surface-3) 96%, transparent);
    box-shadow: var(--shadow-drawer);
    backdrop-filter: blur(var(--blur));
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  .drawer {
    transition:
      transform var(--dur-slow) var(--ease-out),
      filter var(--dur-slow) var(--ease-out);
  }
  .drawer.beneath {
    transform: translateX(-28px) scale(0.96);
    filter: brightness(0.75);
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
