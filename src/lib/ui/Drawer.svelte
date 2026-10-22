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
  /** Starting width; people can drag the left edge, and their width is remembered. */
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

// Resizable: drag the left edge. The width is kept per drawer (by title) on this device.
// svelte-ignore state_referenced_locally
const KEY = `hopr.drawer.width.${title}`;
const MIN = 300;
let stored = $state<number | null>(null);
try {
  const v = Number(localStorage.getItem(KEY));
  if (v >= MIN) stored = v;
} catch {
  // No storage: the default width is used.
}
const maxWidth = () => Math.max(MIN, Math.min(820, window.innerWidth * 0.7));
let resizing = $state(false);

function startResize(e: PointerEvent) {
  e.preventDefault();
  const handle = e.currentTarget as HTMLElement;
  const aside = handle.parentElement;
  if (!aside) return;
  handle.setPointerCapture(e.pointerId);
  resizing = true;
  const right = aside.getBoundingClientRect().right;
  const move = (m: PointerEvent) => {
    stored = Math.round(Math.min(maxWidth(), Math.max(MIN, right - m.clientX)));
  };
  const up = () => {
    resizing = false;
    handle.removeEventListener("pointermove", move);
    handle.removeEventListener("pointerup", up);
    try {
      if (stored) localStorage.setItem(KEY, String(stored));
    } catch {
      // Not remembered; it still applies now.
    }
  };
  handle.addEventListener("pointermove", move);
  handle.addEventListener("pointerup", up);
}

function resetWidth() {
  stored = null;
  try {
    localStorage.removeItem(KEY);
  } catch {
    // Nothing to forget.
  }
}
</script>

<aside
  class="drawer"
  class:pinned
  class:beneath
  class:resizing
  aria-label={title}
  style:width={stored ? `${stored}px` : width}
  style:z-index={pinned ? undefined : layer}
  in:springIn={{ x: 28 }}
  out:sink={{ x: 20, duration: 160 }}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="grip" title="Drag to resize · double-click to reset" onpointerdown={startResize} ondblclick={resetWidth}></div>
  <header class="flex items-center gap-1 py-2 pr-2 pl-4">
    <h2 class="min-w-0 flex-1 truncate text-sm font-medium">{title}</h2>
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
    max-width: calc(100% - 16px);
    background: color-mix(in oklab, var(--surface-3) 96%, transparent);
    box-shadow: var(--shadow-drawer);
    backdrop-filter: blur(var(--blur));
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    transition:
      transform var(--dur-slow) var(--ease-out),
      filter var(--dur-slow) var(--ease-out);
  }
  .drawer.beneath {
    transform: translateX(-28px) scale(0.96);
    filter: brightness(0.75);
  }
  /* Pinned: docked beside the content as a flat side panel. */
  .drawer.pinned {
    position: relative;
    inset: auto;
    flex-shrink: 0;
    max-width: 70vw;
    background: var(--surface-1);
    backdrop-filter: none;
    box-shadow: none;
    border-width: 0 0 0 1px;
    border-radius: 0;
  }
  /* The left edge: drag to resize. */
  .grip {
    position: absolute;
    top: 0;
    bottom: 0;
    left: -3px;
    z-index: 2;
    width: 7px;
    cursor: col-resize;
  }
  .grip::after {
    content: "";
    position: absolute;
    top: 50%;
    left: 2px;
    width: 3px;
    height: 40px;
    border-radius: 999px;
    background: var(--accent);
    opacity: 0;
    transform: translateY(-50%) scaleY(0.6);
    transition:
      opacity var(--dur) var(--ease-out),
      transform var(--dur) var(--ease-spring);
  }
  .grip:hover::after,
  .resizing .grip::after {
    opacity: 1;
    transform: translateY(-50%) scaleY(1);
  }
  .resizing {
    user-select: none;
    transition: none;
  }
</style>
