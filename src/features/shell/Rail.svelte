<script lang="ts">
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
} from "phosphor-svelte";
import { tick } from "svelte";
import { palette } from "$features/palette/palette-state.svelte";
import { sessions } from "$features/terminal/session.svelte";
import { vault } from "$features/vault/vault.svelte";
import { t } from "$lib/i18n/i18n.svelte";
import IconButton from "$lib/ui/IconButton.svelte";
import { shell } from "./shell.state.svelte";

// One highlight that slides between items instead of jumping (motion plan, M3).
let nav = $state<HTMLElement>();
let pill = $state<{ y: number; h: number } | null>(null);
$effect(() => {
  void shell.drawer;
  void sessions.activeKey;
  void tick().then(() => {
    // An open drawer wins over the Hosts view behind it.
    const on = [...(nav?.querySelectorAll<HTMLElement>(".btn.active") ?? [])].at(-1);
    pill = on ? { y: on.offsetTop, h: on.offsetHeight } : null;
  });
});
</script>

<nav
  class="rail frame relative flex w-12 shrink-0 flex-col items-center gap-1 border-r border-border bg-surface-1 py-2"
  aria-label={t("rail.primary")}
  bind:this={nav}
>
  {#if pill}
    <span class="pill" style:transform="translateY({pill.y}px)" style:height="{pill.h}px" aria-hidden="true"></span>
  {/if}
  <IconButton
    label={t("rail.hosts")}
    shortcut="Ctrl+Shift+N"
    tip="right"
    active={sessions.activeKey === null}
    onclick={() => sessions.showHome()}
  >
    <HardDrives size={18} weight="light" />
  </IconButton>
  <IconButton label={t("rail.files")} shortcut="Ctrl+Shift+O" tip="right" active={shell.drawer === "files"} onclick={() => shell.toggle("files")}>
    <FolderSimple size={18} weight="light" />
  </IconButton>
  <IconButton label={t("rail.snippets")} shortcut="Ctrl+Shift+S" tip="right" active={shell.drawer === "snippets"} onclick={() => shell.toggle("snippets")}>
    <Code size={18} weight="light" />
  </IconButton>
  <IconButton label={t("rail.tunnels")} shortcut="Ctrl+Shift+U" tip="right" active={shell.drawer === "tunnels"} onclick={() => shell.toggle("tunnels")}>
    <ArrowsLeftRight size={18} weight="light" />
  </IconButton>
  <IconButton label={t("rail.history")} shortcut="Ctrl+Shift+Y" tip="right" active={shell.drawer === "history"} onclick={() => shell.toggle("history")}>
    <ClockCounterClockwise size={18} weight="light" />
  </IconButton>
  <IconButton label={t("rail.keys")} tip="right" active={shell.drawer === "keys"} onclick={() => shell.toggle("keys")}>
    <Key size={18} weight="light" />
  </IconButton>
  <!-- Files, Tunnels and Snippets join the rail as their phases ship. -->
  <div class="flex-1"></div>
  <IconButton label={t("rail.palette")} shortcut="Ctrl+Shift+P" tip="right" onclick={() => palette.show()}>
    <MagnifyingGlass size={18} weight="light" />
  </IconButton>
  <IconButton label={t("rail.lock")} shortcut="Ctrl+Shift+L" tip="right" onclick={() => vault.lock()}>
    <LockSimple size={18} weight="light" />
  </IconButton>
  <IconButton
    label={t("rail.settings")}
    shortcut="Ctrl+,"
    tip="right"
    active={shell.drawer === "settings"}
    onclick={() => shell.toggle("settings")}
  >
    <GearSix size={18} weight="light" />
  </IconButton>
</nav>

<style>
  .pill {
    position: absolute;
    top: 0;
    left: 8px;
    width: 32px;
    border-radius: var(--radius-sm);
    background: color-mix(in oklab, var(--accent) 13%, transparent);
    box-shadow: inset 2px 0 0 var(--accent);
    transition:
      transform var(--dur-spring) var(--ease-spring),
      height var(--dur) var(--ease-out);
    pointer-events: none;
  }
  .rail :global(.btn) {
    z-index: 1;
  }
  .rail :global(.btn.active) {
    background: transparent;
  }
</style>
