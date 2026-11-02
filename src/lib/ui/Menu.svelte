<script module lang="ts">
export interface MenuTriggerProps {
  "aria-haspopup": "menu";
  "aria-expanded": boolean;
  onclick: (e: MouseEvent) => void;
}
</script>

<script lang="ts">
/**
 * A button that opens a list of actions. Owns what every menu needs: closing on an outside
 * click or Esc (focus goes back to the button), arrow keys between items, and the panel.
 * Items are `<button role="menuitem" class="menu-item">` (styles in app.css).
 */
import type { Snippet } from "svelte";
import { blurFade, sink } from "$lib/motion";

interface Props {
  open: boolean;
  onopenchange: (open: boolean) => void;
  /** Accessible name of the menu. */
  label: string;
  /** Which edge of the button the panel lines up with. */
  align?: "start" | "end";
  width?: string;
  /** Close after an item is chosen (off when an item asks to confirm inside the menu). */
  closeOnSelect?: boolean;
  trigger: Snippet<[MenuTriggerProps]>;
  children: Snippet;
}

let {
  open,
  onopenchange,
  label,
  align = "end",
  width = "200px",
  closeOnSelect = true,
  trigger,
  children,
}: Props = $props();

let root = $state<HTMLElement>();
let panel = $state<HTMLElement>();

const items = () => [
  ...(panel?.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)') ?? []),
];

function close(refocus = false) {
  onopenchange(false);
  if (refocus) root?.querySelector<HTMLElement>('[aria-haspopup="menu"]')?.focus();
}

const triggerProps = $derived<MenuTriggerProps>({
  "aria-haspopup": "menu",
  "aria-expanded": open,
  onclick: (e) => {
    e.stopPropagation();
    if (open) close();
    else onopenchange(true);
  },
});

// While open, the whole window is watched: a click elsewhere or Esc closes this menu first.
$effect(() => {
  if (!open) return;
  const down = (e: PointerEvent) => {
    if (!root?.contains(e.target as Node)) close();
  };
  const key = (e: KeyboardEvent) => {
    if (e.key !== "Escape") return;
    e.stopImmediatePropagation();
    close(true);
  };
  window.addEventListener("pointerdown", down, true);
  window.addEventListener("keydown", key, true);
  return () => {
    window.removeEventListener("pointerdown", down, true);
    window.removeEventListener("keydown", key, true);
  };
});

function onkeydown(e: KeyboardEvent) {
  const list = items();
  const i = list.indexOf(document.activeElement as HTMLElement);
  const next =
    e.key === "ArrowDown"
      ? list[(i + 1) % list.length]
      : e.key === "ArrowUp"
        ? list.at(i <= 0 ? -1 : i - 1)
        : e.key === "Home"
          ? list[0]
          : e.key === "End"
            ? list.at(-1)
            : undefined;
  if (!next) return;
  // Handled here: a list around the menu (the hosts) must not move its own focus too.
  e.preventDefault();
  e.stopPropagation();
  next.focus();
}

function onclick(e: MouseEvent) {
  if (closeOnSelect && (e.target as Element).closest('[role="menuitem"]')) close();
}
</script>

<div class="relative" bind:this={root}>
  {@render trigger(triggerProps)}
  {#if open}
    <!-- svelte-ignore a11y_click_events_have_key_events (items are buttons; keys are handled on the menu) -->
    <div
      class="menu"
      class:start={align === "start"}
      style:width
      role="menu"
      tabindex="-1"
      aria-label={label}
      bind:this={panel}
      {onkeydown}
      {onclick}
      in:blurFade={{ y: -4, scale: 0.97, blur: 2, duration: 160 }}
      out:sink={{ y: -4, scale: 0.97, duration: 100 }}
    >
      {@render children()}
    </div>
  {/if}
</div>

<style>
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: 70;
    display: flex;
    flex-direction: column;
    padding: 4px;
    background: color-mix(in oklab, var(--surface-3) 97%, transparent);
    backdrop-filter: blur(var(--blur));
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-pop);
    transform-origin: top right;
  }
  .menu.start {
    right: auto;
    left: 0;
    transform-origin: top left;
  }
</style>
