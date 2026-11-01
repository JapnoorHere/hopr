<script module lang="ts">
/** Open dialogs, newest last: only the top one answers Esc (a dialog opened from Settings). */
const open: symbol[] = [];
</script>

<script lang="ts">
/**
 * A dialog over a dimmed window. Owns what every modal needs: Esc and a click outside close
 * it, Tab stays inside, and focus goes back where it was when it closes. The panel itself
 * (its size, look and entrance) is the caller's child element.
 */
import type { Snippet } from "svelte";
import { fade } from "svelte/transition";
import { motion } from "$lib/platform";

interface Props {
  /** Accessible name of the dialog. */
  label: string;
  onclose: () => void;
  /** Esc runs this first; return true when it was used for something else (e.g. clearing a search). */
  onescape?: () => boolean;
  place?: "center" | "top";
  /** Stacking order: dialogs opened from other dialogs sit higher. */
  layer?: number;
  children: Snippet;
}

let { label, onclose, onescape, place = "center", layer = 50, children }: Props = $props();

let scrim = $state<HTMLElement>();
const self = Symbol("modal");

$effect(() => {
  const before = document.activeElement as HTMLElement | null;
  open.push(self);
  return () => {
    open.splice(open.indexOf(self), 1);
    before?.focus?.();
  };
});

function onkeydowncapture(e: KeyboardEvent) {
  if (e.key !== "Escape" || open.at(-1) !== self) return;
  e.preventDefault();
  e.stopImmediatePropagation();
  if (!onescape?.()) onclose();
}

/** Tab and Shift+Tab wrap around inside the dialog. */
function trapTab(e: KeyboardEvent) {
  if (e.key !== "Tab" || !scrim) return;
  const focusable = [
    ...scrim.querySelectorAll<HTMLElement>(
      'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [href], [tabindex]:not([tabindex="-1"])',
    ),
  ].filter((el) => el.offsetParent !== null);
  const first = focusable[0];
  const last = focusable.at(-1);
  if (!first || !last) return;
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault();
    first.focus();
  }
}
</script>

<svelte:window {onkeydowncapture} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
  class="scrim"
  class:top={place === "top"}
  style:z-index={layer}
  bind:this={scrim}
  onclick={(e) => e.target === e.currentTarget && onclose()}
  onkeydown={trapTab}
  transition:fade={{ duration: motion(140) }}
>
  <div class="contents" role="dialog" aria-modal="true" aria-label={label}>
    {@render children()}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 24px;
    background: color-mix(in oklab, var(--bg) 55%, transparent);
    backdrop-filter: blur(3px);
  }
  .scrim.top {
    place-items: start center;
    padding-top: 12vh;
  }
</style>
