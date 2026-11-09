<script lang="ts">
/**
 * Right-click on a tab, as in a browser: split, separate a split back into tabs, duplicate,
 * move to a new or another window, close others. Opens at the pointer, kept on screen.
 */
import {
  AppWindow,
  ArrowLineRight,
  ArrowSquareOut,
  CaretRight,
  Copy,
  Plus,
  Rows,
  SquareSplitHorizontal,
  SquareSplitVertical,
  X,
} from "phosphor-svelte";
import { tick } from "svelte";
import { commands, inTauri, type WindowEntry } from "$lib/ipc";
import { blurFade, sink } from "$lib/motion";
import { formatShortcut } from "$lib/platform";
import { sessions, type Tab } from "./session.svelte";

interface Props {
  tab: Tab;
  x: number;
  y: number;
  /** Offer moving to other windows (not in the quick terminal). */
  canMove: boolean;
  onclose: () => void;
}

let { tab, x, y, canMove, onclose }: Props = $props();

let panel = $state<HTMLElement>();
let left = $state(0);
let top = $state(0);
let windows = $state<WindowEntry[]>([]);
let sub = $state(false);

const index = $derived(sessions.tabs.indexOf(tab));
const count = $derived(sessions.panesOf(tab).length);
const others = $derived(sessions.tabs.length > 1);
const right = $derived(index < sessions.tabs.length - 1);

// Kept inside the window, flipped left/up near the right/bottom edge.
$effect(() => {
  void x;
  void y;
  void tick().then(() => {
    const r = panel?.getBoundingClientRect();
    if (!r) return;
    left = Math.max(4, Math.min(x, innerWidth - r.width - 4));
    top = y + r.height > innerHeight - 4 ? Math.max(4, y - r.height) : y;
    panel?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  });
});

$effect(() => {
  if (!canMove || !inTauri) return;
  void commands.windowList().then((w) => {
    windows = w;
  });
});

// Outside click, Esc, or the window losing focus closes it.
$effect(() => {
  const down = (e: PointerEvent) => {
    if (!panel?.contains(e.target as Node)) onclose();
  };
  const key = (e: KeyboardEvent) => {
    if (e.key !== "Escape") return;
    e.stopImmediatePropagation();
    onclose();
  };
  const blur = () => onclose();
  window.addEventListener("pointerdown", down, true);
  window.addEventListener("keydown", key, true);
  window.addEventListener("blur", blur);
  return () => {
    window.removeEventListener("pointerdown", down, true);
    window.removeEventListener("keydown", key, true);
    window.removeEventListener("blur", blur);
  };
});

/** Act, then close: the action reads `tab`, which goes away with the menu. */
function run(action: () => unknown) {
  const done = action();
  onclose();
  void done;
}

function onkeydown(e: KeyboardEvent) {
  const list = [
    ...(panel?.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)') ?? []),
  ];
  const i = list.indexOf(document.activeElement as HTMLElement);
  const next =
    e.key === "ArrowDown"
      ? list[(i + 1) % list.length]
      : e.key === "ArrowUp"
        ? list.at(i <= 0 ? -1 : i - 1)
        : undefined;
  if (e.key === "ArrowRight" && (document.activeElement as HTMLElement)?.dataset.sub) {
    sub = true;
    void tick().then(() => panel?.querySelector<HTMLElement>(".sub [role=menuitem]")?.focus());
  } else if (e.key === "ArrowLeft" && sub) {
    sub = false;
    panel?.querySelector<HTMLElement>("[data-sub]")?.focus();
  }
  if (!next) return;
  e.preventDefault();
  next.focus();
}

function title(w: WindowEntry): string {
  const t = w.title.replace(/^Hopr( — )?/, "");
  return w.label === "main" ? `Main window${t ? ` — ${t}` : ""}` : t || "Hopr window";
}
</script>

{#snippet item(label: string, icon: typeof X | null, action: () => unknown, keys = "", danger = false)}
  <button type="button" role="menuitem" class="menu-item" class:danger onclick={() => run(action)}>
    <span class="icon">{#if icon}{@const Icon = icon}<Icon size={14} />{/if}</span>
    <span class="flex-1">{label}</span>
    {#if keys}<span class="keys">{formatShortcut(keys)}</span>{/if}
  </button>
{/snippet}

<!-- svelte-ignore a11y_no_noninteractive_element_interactions (keys move between items) -->
<div
  class="tab-menu"
  role="menu"
  tabindex="-1"
  aria-label="Tab actions"
  style:left="{left}px"
  style:top="{top}px"
  bind:this={panel}
  {onkeydown}
  oncontextmenu={(e) => e.preventDefault()}
  in:blurFade={{ y: -4, scale: 0.97, blur: 2, duration: 140 }}
  out:sink={{ y: -4, scale: 0.97, duration: 90 }}
>
  {@render item("New tab", Plus, () => sessions.showHome(), "Ctrl+Shift+T")}
  {@render item("Duplicate", Copy, () => sessions.duplicate(tab))}
  <hr class="menu-sep" />
  {@render item("Split right", SquareSplitHorizontal, () => sessions.splitTab(tab, "row"), "Ctrl+Shift+D")}
  {@render item("Split down", SquareSplitVertical, () => sessions.splitTab(tab, "column"), "Ctrl+Shift+E")}
  {#if count > 1}
    {@render item("Move focused pane to new tab", ArrowLineRight, () => {
      const s = sessions.session(tab.focused);
      if (s) sessions.paneToTab(s);
    })}
    {@render item(`Separate ${count} panes into tabs`, Rows, () => sessions.separate(tab))}
  {/if}
  {#if canMove && inTauri}
    <hr class="menu-sep" />
    {@render item("Move to new window", ArrowSquareOut, () => sessions.tearOff(tab), "Ctrl+Shift+M")}
    {#if windows.length}
      <div class="relative" role="none" onpointerenter={() => (sub = true)} onpointerleave={() => (sub = false)}>
        <button type="button" role="menuitem" class="menu-item w-full" data-sub="1" aria-haspopup="menu" aria-expanded={sub} onclick={() => (sub = !sub)}>
          <span class="icon"><AppWindow size={14} /></span>
          <span class="flex-1">Move to another window</span>
          <CaretRight size={12} class="text-faint" />
        </button>
        {#if sub}
          <div class="tab-menu sub" role="menu" aria-label="Windows" transition:blurFade={{ x: -4, scale: 0.98, blur: 2, duration: 120 }}>
            {#each windows as w (w.label)}
              <button type="button" role="menuitem" class="menu-item" onclick={() => run(() => sessions.sendTo(tab, w.label))}>
                <span class="truncate">{title(w)}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  {/if}
  <hr class="menu-sep" />
  {@render item("Close", X, () => sessions.closeTab(tab))}
  {#if others}
    {@render item("Close other tabs", null, () => sessions.closeOthers(tab))}
  {/if}
  {#if right}
    {@render item("Close tabs to the right", null, () => sessions.closeRight(tab))}
  {/if}
</div>

<style>
  .tab-menu {
    position: fixed;
    z-index: 80;
    display: flex;
    flex-direction: column;
    width: 280px;
    padding: 4px;
    background: color-mix(in oklab, var(--surface-3) 97%, transparent);
    backdrop-filter: blur(var(--blur));
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-pop);
    transform-origin: top left;
  }
  .tab-menu.sub {
    position: absolute;
    top: -5px;
    left: calc(100% + 4px);
    width: 280px;
    white-space: nowrap;
  }
  .tab-menu :global(.menu-item) {
    white-space: nowrap;
  }
  .icon {
    display: grid;
    width: 14px;
    place-items: center;
    color: var(--text-muted);
  }
  .keys {
    font-size: 11px;
    color: var(--text-faint);
  }
</style>
