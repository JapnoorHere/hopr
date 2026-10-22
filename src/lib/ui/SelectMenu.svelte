<script lang="ts">
/**
 * Hopr's own dropdown menu for every <select> in the app. Opening a select (click, or
 * Enter / Space / Alt+Down) shows this menu instead of the browser's; choosing sets the
 * select's value and fires input + change, so bind:value and onchange keep working.
 */
import { Check } from "phosphor-svelte";
import { tick } from "svelte";
import { blurFade, sink } from "$lib/motion";

interface Item {
  value: string;
  label: string;
  disabled: boolean;
  group: string | null;
}

let target = $state<HTMLSelectElement | null>(null);
let items = $state<Item[]>([]);
let active = $state(0);
let pos = $state({ left: 0, top: 0, width: 0, maxHeight: 280, above: false });
let list = $state<HTMLDivElement>();
let typed = "";
let typedAt = 0;

const enhanced = (el: EventTarget | null): el is HTMLSelectElement =>
  el instanceof HTMLSelectElement && !el.multiple && el.size <= 1 && !el.disabled;

function read(select: HTMLSelectElement): Item[] {
  return [...select.options].map((o) => ({
    value: o.value,
    label: o.label || o.text,
    disabled: o.disabled,
    group: o.parentElement instanceof HTMLOptGroupElement ? o.parentElement.label : null,
  }));
}

function place(select: HTMLSelectElement) {
  const r = select.getBoundingClientRect();
  const below = window.innerHeight - r.bottom - 12;
  const above = r.top - 12;
  const want = Math.min(300, items.length * 32 + 12);
  const up = below < Math.min(want, 160) && above > below;
  pos = {
    left: Math.min(r.left, window.innerWidth - Math.max(r.width, 180) - 8),
    top: up ? r.top - 4 : r.bottom + 4,
    width: Math.max(r.width, 180),
    maxHeight: Math.max(120, Math.min(300, up ? above : below)),
    above: up,
  };
}

async function open(select: HTMLSelectElement) {
  items = read(select);
  if (!items.length) return;
  target = select;
  active = Math.max(0, select.selectedIndex);
  select.dataset.menuOpen = "";
  place(select);
  await tick();
  list?.querySelector<HTMLElement>(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" });
}

function close(refocus = true) {
  const t = target;
  target = null;
  if (t) {
    delete t.dataset.menuOpen;
    if (refocus) t.focus();
  }
}

function choose(i: number) {
  const t = target;
  const it = items.at(i);
  if (!t || !it || it.disabled) return;
  if (t.value !== it.value) {
    t.value = it.value;
    t.dispatchEvent(new Event("input", { bubbles: true }));
    t.dispatchEvent(new Event("change", { bubbles: true }));
  }
  close();
}

function move(step: number) {
  if (!items.length) return;
  let i = active;
  for (let n = 0; n < items.length; n++) {
    i = (i + step + items.length) % items.length;
    if (!items.at(i)?.disabled) break;
  }
  active = i;
  list?.querySelector<HTMLElement>(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" });
}

/** Type a few letters to jump to the first item that starts with them. */
function typeahead(key: string) {
  const now = Date.now();
  typed = now - typedAt < 700 ? typed + key.toLowerCase() : key.toLowerCase();
  typedAt = now;
  const i = items.findIndex((it) => !it.disabled && it.label.toLowerCase().startsWith(typed));
  if (i >= 0) {
    active = i;
    list?.querySelector<HTMLElement>(`[data-i="${i}"]`)?.scrollIntoView({ block: "nearest" });
  }
}

function onPointerDown(e: PointerEvent) {
  if (target) {
    // A click outside the menu closes it; a click on another select then opens that one.
    const inside = e.target instanceof Node && list?.contains(e.target);
    if (inside) return;
    const same = e.target === target;
    close(false);
    if (same) {
      e.preventDefault();
      return;
    }
  }
  if (e.button === 0 && enhanced(e.target)) {
    e.preventDefault();
    e.target.focus();
    void open(e.target);
  }
}

// The browser opens its own list on mousedown; stop that for every select.
function onMouseDown(e: MouseEvent) {
  if (enhanced(e.target)) e.preventDefault();
}

function onKeyDown(e: KeyboardEvent) {
  if (target) {
    const stop = () => {
      e.preventDefault();
      e.stopImmediatePropagation();
    };
    if (e.key === "Escape" || e.key === "Tab") {
      stop();
      close();
    } else if (e.key === "ArrowDown") {
      stop();
      move(1);
    } else if (e.key === "ArrowUp") {
      stop();
      move(-1);
    } else if (e.key === "Home") {
      stop();
      active = items.length - 1;
      move(1);
    } else if (e.key === "End") {
      stop();
      active = 0;
      move(-1);
    } else if (e.key === "Enter" || e.key === " ") {
      stop();
      choose(active);
    } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey) {
      stop();
      typeahead(e.key);
    }
    return;
  }
  if (
    enhanced(e.target) &&
    (e.key === "Enter" || e.key === " " || e.key === "F4" || (e.altKey && e.key === "ArrowDown"))
  ) {
    e.preventDefault();
    e.stopImmediatePropagation();
    void open(e.target);
  }
}
</script>

<svelte:window
  onpointerdowncapture={onPointerDown}
  onmousedowncapture={onMouseDown}
  onkeydowncapture={onKeyDown}
  onresize={() => close(false)}
  onblur={() => close(false)}
/>
<svelte:document
  onscrollcapture={(e) => {
    if (target && !(e.target instanceof Node && list?.contains(e.target))) close(false);
  }}
/>

{#if target}
  <div
    class="menu"
    role="listbox"
    tabindex="-1"
    aria-label={target.getAttribute("aria-label") ?? "Options"}
    bind:this={list}
    style:left="{pos.left}px"
    style:top="{pos.top}px"
    style:min-width="{pos.width}px"
    style:max-height="{pos.maxHeight}px"
    class:above={pos.above}
    in:blurFade={{
      y: pos.above ? 6 : -6,
      scale: 0.98,
      blur: 2,
      duration: 160,
      base: pos.above ? "translateY(-100%)" : "",
    }}
    out:sink={{ y: pos.above ? 4 : -4, scale: 0.98, duration: 100, base: pos.above ? "translateY(-100%)" : "" }}
  >
    {#each items as it, i (i)}
      {#if it.group && it.group !== items.at(i - 1)?.group}
        <p class="group">{it.group}</p>
      {/if}
      <button
        type="button"
        role="option"
        class="item"
        class:active={i === active}
        class:chosen={target?.value === it.value}
        aria-selected={target?.value === it.value}
        disabled={it.disabled}
        data-i={i}
        tabindex="-1"
        onpointermove={() => (active = i)}
        onclick={() => choose(i)}
      >
        <span class="min-w-0 flex-1 truncate">{it.label}</span>
        {#if target?.value === it.value}<Check size={13} weight="bold" class="shrink-0 text-accent" />{/if}
      </button>
    {/each}
  </div>
{/if}

<style>
  .menu {
    position: fixed;
    z-index: 70;
    display: flex;
    flex-direction: column;
    max-width: min(420px, calc(100vw - 16px));
    padding: 4px;
    overflow-y: auto;
    background: color-mix(in oklab, var(--surface-3) 97%, transparent);
    backdrop-filter: blur(var(--blur));
    border: 1px solid var(--border-strong, var(--border));
    border-radius: var(--radius);
    box-shadow: var(--shadow-pop);
  }
  .menu.above {
    transform: translateY(-100%);
  }
  .group {
    padding: 8px 10px 4px;
    font-family: var(--font-label);
    font-size: 10.5px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-faint);
  }
  .item {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 8px;
    min-height: 30px;
    padding: 5px 10px;
    font-size: 13px;
    text-align: left;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
    transition:
      background var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out);
  }
  .item.active {
    color: var(--text);
    background: var(--surface-4, var(--surface-2));
  }
  .item.chosen {
    color: var(--text);
    font-weight: 500;
  }
  .item:disabled {
    opacity: 0.4;
  }
</style>
