<script lang="ts">
/**
 * The sidebar. It sits on the left or the right, and can auto-hide: then it slides in when
 * the pointer touches that edge of the window (or keyboard focus moves into it) and slides
 * away again shortly after the pointer leaves.
 */
import {
  ArrowsLeftRight,
  ClockCounterClockwise,
  Code,
  FolderSimple,
  GearSix,
  HardDrives,
  Key,
  LockSimple,
  MagnifyingGlass,
  PushPinSimple,
  PushPinSimpleSlash,
} from "phosphor-svelte";
import { tick } from "svelte";
import { palette } from "$features/palette/palette-state.svelte";
import { sessions } from "$features/terminal/session.svelte";
import { vault } from "$features/vault/vault.svelte";
import { t } from "$lib/i18n/i18n.svelte";
import IconButton from "$lib/ui/IconButton.svelte";
import { layout } from "./layout.svelte";
import { shell } from "./shell.state.svelte";

/** How long the pointer may be away before an auto-hiding sidebar slides out. */
const HIDE_DELAY_MS = 350;

const side = $derived(layout.current.railSide);
const auto = $derived(layout.current.railAutoHide);
/** Tooltips point away from the window edge. */
const tip = $derived(side === "left" ? "right" : "left");

// One highlight that slides between items instead of jumping (motion plan, M3).
let nav = $state<HTMLElement>();
let pill = $state<{ x: number; y: number; w: number; h: number } | null>(null);
/** Auto-hide: shown while the pointer or focus is on it. */
let peek = $state(false);
let hideTimer: ReturnType<typeof setTimeout> | undefined;

/** Measure the active button: the pill matches its box exactly, whatever the density. */
function measure() {
  // An open drawer wins over the Hosts view behind it.
  const on = [...(nav?.querySelectorAll<HTMLElement>(".btn.active") ?? [])].at(-1);
  pill = on ? { x: on.offsetLeft, y: on.offsetTop, w: on.offsetWidth, h: on.offsetHeight } : null;
}

$effect(() => {
  void shell.drawer;
  void sessions.activeKey;
  void tick().then(measure);
});

// Anything that re-lays out the rail (density, window size, fonts loading) moves the
// buttons, so the pill follows instead of drifting out of line.
$effect(() => {
  if (!nav) return;
  const ro = new ResizeObserver(() => measure());
  ro.observe(nav);
  return () => ro.disconnect();
});

function show() {
  clearTimeout(hideTimer);
  peek = true;
}

function hideSoon() {
  clearTimeout(hideTimer);
  hideTimer = setTimeout(() => {
    // Keyboard focus inside keeps it out.
    if (!nav?.contains(document.activeElement)) peek = false;
  }, HIDE_DELAY_MS);
}

// Turning auto-hide off (or on) starts from a settled state.
$effect(() => {
  void auto;
  peek = false;
  return () => clearTimeout(hideTimer);
});
</script>

{#if auto}
  <!-- The edge the pointer touches to bring the sidebar in. -->
  <div
    class="hot"
    class:right={side === "right"}
    aria-hidden="true"
    onpointerenter={show}
  ></div>
{/if}
<nav
  class="rail frame relative flex w-12 shrink-0 flex-col items-center gap-1 bg-surface-1 py-2"
  class:right={side === "right"}
  class:auto
  class:peek
  aria-label={t("rail.primary")}
  bind:this={nav}
  onpointerenter={auto ? show : undefined}
  onpointerleave={auto ? hideSoon : undefined}
  onfocusin={auto ? show : undefined}
  onfocusout={auto ? hideSoon : undefined}
>
  {#if pill}
    <span
      class="pill"
      style:transform="translate({pill.x}px, {pill.y}px)"
      style:width="{pill.w}px"
      style:height="{pill.h}px"
      aria-hidden="true"
    ></span>
  {/if}
  <IconButton
    label={t("rail.hosts")}
    shortcut="Ctrl+Shift+N"
    {tip}
    active={sessions.activeKey === null}
    onclick={() => sessions.showHome()}
  >
    <HardDrives size={18} weight="light" />
  </IconButton>
  <IconButton label={t("rail.files")} shortcut="Ctrl+Shift+O" {tip} active={shell.drawer === "files"} onclick={() => shell.toggle("files")}>
    <FolderSimple size={18} weight="light" />
  </IconButton>
  <IconButton label={t("rail.snippets")} shortcut="Ctrl+Shift+S" {tip} active={shell.drawer === "snippets"} onclick={() => shell.toggle("snippets")}>
    <Code size={18} weight="light" />
  </IconButton>
  <IconButton label={t("rail.tunnels")} shortcut="Ctrl+Shift+U" {tip} active={shell.drawer === "tunnels"} onclick={() => shell.toggle("tunnels")}>
    <ArrowsLeftRight size={18} weight="light" />
  </IconButton>
  <IconButton label={t("rail.history")} shortcut="Ctrl+Shift+Y" {tip} active={shell.drawer === "history"} onclick={() => shell.toggle("history")}>
    <ClockCounterClockwise size={18} weight="light" />
  </IconButton>
  <IconButton label={t("rail.keys")} {tip} active={shell.drawer === "keys"} onclick={() => shell.toggle("keys")}>
    <Key size={18} weight="light" />
  </IconButton>
  <div class="flex-1"></div>
  <IconButton label={t("rail.palette")} shortcut="Ctrl+Shift+P" {tip} onclick={() => palette.show()}>
    <MagnifyingGlass size={18} weight="light" />
  </IconButton>
  <IconButton label={t("rail.lock")} shortcut="Ctrl+Shift+L" {tip} onclick={() => vault.lock()}>
    <LockSimple size={18} weight="light" />
  </IconButton>
  <IconButton
    label={t("rail.settings")}
    shortcut="Ctrl+,"
    {tip}
    active={shell.drawer === "settings"}
    onclick={() => shell.toggle("settings")}
  >
    <GearSix size={18} weight="light" />
  </IconButton>
  <IconButton
    label={auto ? "Keep the sidebar shown" : "Hide the sidebar until the pointer reaches the edge"}
    {tip}
    onclick={() => layout.update({ railAutoHide: !auto })}
  >
    {#if auto}<PushPinSimple size={16} weight="light" />{:else}<PushPinSimpleSlash size={16} weight="light" />{/if}
  </IconButton>
</nav>

<style>
  .rail {
    border-right: 1px solid var(--border);
    transition:
      transform var(--dur) var(--ease-out),
      box-shadow var(--dur) var(--ease-out);
  }
  /* On the right: after the content and the side panels. */
  .rail.right {
    order: 99;
    border-right: 0;
    border-left: 1px solid var(--border);
  }
  /* Auto-hide: it floats over the content, tucked out of sight until it peeks in. */
  .rail.auto {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    z-index: 40;
    transform: translateX(-100%);
  }
  .rail.auto.right {
    left: auto;
    right: 0;
    transform: translateX(100%);
  }
  .rail.auto.peek {
    transform: none;
    box-shadow: var(--shadow-pop);
  }
  .hot {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    z-index: 39;
    width: 6px;
  }
  .hot.right {
    left: auto;
    right: 0;
  }
  .pill {
    position: absolute;
    top: 0;
    left: 0;
    border-radius: var(--radius-sm);
    background: color-mix(in oklab, var(--accent) 13%, transparent);
    box-shadow: inset 2px 0 0 var(--accent);
    transition:
      transform var(--dur-spring) var(--ease-spring),
      height var(--dur) var(--ease-out);
    pointer-events: none;
  }
  /* The accent edge faces the content. */
  .rail.right .pill {
    box-shadow: inset -2px 0 0 var(--accent);
  }
  .rail :global(.btn) {
    z-index: 1;
  }
  :global(:root[data-motion="reduced"]) .rail {
    transition: none;
  }
</style>
