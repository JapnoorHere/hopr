<script lang="ts">
import { X } from "phosphor-svelte";
import type { Snippet } from "svelte";
import { blurFade, sink, springIn } from "$lib/motion";
import IconButton from "./IconButton.svelte";

interface Props {
  title: string;
  onclose: () => void;
  /** Starting width; people can drag the left edge, and their width is remembered. */
  width?: string;
  children: Snippet;
}

let { title, onclose, width = "340px", children }: Props = $props();

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

/**
 * Leaving: lift the drawer out of the layout where it stands, so a drawer opened in its place
 * docks straight away instead of the two sitting side by side while this one fades.
 */
function leave(node: Element, o: Parameters<typeof sink>[1]) {
  const el = node as HTMLElement;
  const r = el.getBoundingClientRect();
  Object.assign(el.style, {
    position: "fixed",
    top: `${r.top}px`,
    left: `${r.left}px`,
    width: `${r.width}px`,
    height: `${r.height}px`,
    zIndex: "30",
    pointerEvents: "none",
  });
  return sink(node, o);
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
  class:resizing
  aria-label={title}
  style:width={stored ? `${stored}px` : width}
  in:springIn={{ x: 28 }}
  out:leave={{ x: 20, duration: 140 }}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="grip" title="Drag to resize · double-click to reset" onpointerdown={startResize} ondblclick={resetWidth}></div>
  <header class="flex items-center gap-1 py-2 pr-2 pl-4">
    <h2 class="min-w-0 flex-1 truncate text-sm font-medium">{title}</h2>
    <IconButton label="Close" shortcut="Esc" size={28} tip="bottom-end" onclick={onclose}>
      <X size={15} />
    </IconButton>
  </header>
  <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-4" in:blurFade={{ delay: 70, y: 6, blur: 2, duration: 220 }}>
    {@render children()}
  </div>
</aside>

<style>
  /* Docked beside the content: full height, flush to the window edge. */
  .drawer {
    position: relative;
    display: flex;
    flex-shrink: 0;
    flex-direction: column;
    max-width: 70vw;
    background: var(--surface-1);
    border-left: 1px solid var(--border);
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
  }
</style>
