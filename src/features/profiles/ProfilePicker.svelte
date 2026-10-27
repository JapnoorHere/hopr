<script lang="ts">
/**
 * "Choose a profile": shown when Hopr opens (like a browser's profile picker) and from the
 * switcher. Each tile is drawn in that profile's own theme on the plain background. Picking the open profile carries
 * on; another one restarts Hopr into it.
 */
import { DotsThree, Plus, X } from "phosphor-svelte";
import { fade } from "svelte/transition";
import { sessions } from "$features/terminal/session.svelte";
import { lookSpec, themeStyle } from "$lib/design/themes";
import HoprMark from "$lib/icons/HoprMark.svelte";
import type { Profile } from "$lib/ipc";
import { blurFade, stagger } from "$lib/motion";
import { motion } from "$lib/platform";
import { toast } from "$lib/ui/toast.svelte";
import ProfileAvatar from "./ProfileAvatar.svelte";
import ProfileDialog from "./ProfileDialog.svelte";
import { profileName, profiles } from "./profiles.svelte";

/** Opened from the switcher (can be dismissed), rather than at start-up. */
let { dismissible = false }: { dismissible?: boolean } = $props();

let editing = $state<Profile | null | "new">(null);
/** Switching away with tabs open asks first. */
let confirming = $state<Profile | null>(null);
let grid = $state<HTMLDivElement>();

async function pick(p: Profile) {
  if (p.id !== profiles.current && sessions.tabs.length > 0 && confirming?.id !== p.id) {
    confirming = p;
    return;
  }
  const r = await profiles.open(p.id);
  if (!r.ok) toast.error(r.error.message);
}

function close() {
  if (dismissible) profiles.picking = false;
}

function onkey(e: KeyboardEvent) {
  if (editing) return;
  const tiles = [...(grid?.querySelectorAll<HTMLElement>("[data-tile]") ?? [])];
  const at = tiles.indexOf(document.activeElement as HTMLElement);
  if (e.key === "Escape") {
    if (confirming) confirming = null;
    else close();
    e.preventDefault();
    e.stopImmediatePropagation();
  } else if (["ArrowRight", "ArrowDown"].includes(e.key)) {
    tiles[(at + 1) % tiles.length]?.focus();
    e.preventDefault();
  } else if (["ArrowLeft", "ArrowUp"].includes(e.key)) {
    tiles[(at - 1 + tiles.length) % tiles.length]?.focus();
    e.preventDefault();
  } else if (/^[1-9]$/.test(e.key) && !(e.target instanceof HTMLInputElement)) {
    const p = profiles.list[Number(e.key) - 1];
    if (p) void pick(p);
  }
}

$effect(() => {
  // Start on the profile used last.
  grid?.querySelector<HTMLElement>(`[data-id="${profiles.current}"]`)?.focus();
});
</script>

<svelte:window onkeydowncapture={onkey} />

<div class="picker" role="dialog" aria-modal="true" aria-label="Choose a profile" transition:fade={{ duration: motion(180) }}>
  {#if dismissible}
    <button type="button" class="dismiss" aria-label="Close" title="Close (Esc)" onclick={close}><X size={16} /></button>
  {/if}

  <div class="inner">
    <span class="text-accent" in:blurFade={{ y: 6 }}><HoprMark size={34} animated /></span>
    <h1 class="mt-4 text-2xl font-semibold text-fg" in:blurFade={{ y: 8, delay: 40 }}>Choose a profile</h1>
    <p class="mt-1 text-sm text-muted" in:blurFade={{ y: 8, delay: 80 }}>
      Each profile keeps its own hosts, keys, history and password.
    </p>

    <div class="tiles" bind:this={grid} role="listbox" aria-label="Profiles">
      {#each profiles.list as p, i (p.id)}
        <div class="tile-wrap" in:blurFade={{ y: 10, delay: 120 + stagger(i), duration: 260 }}>
          <button
            type="button"
            role="option"
            aria-selected={p.id === profiles.current}
            class="tile"
            class:current={p.id === profiles.current}
            class:opening={profiles.switching === p.id}
            data-tile
            data-id={p.id}
            style={themeStyle(lookSpec(p.look))}
            onclick={() => pick(p)}
            title={i < 9 ? `Open (${i + 1})` : "Open"}
          >
            <span class="stripe"></span>
            <ProfileAvatar profile={p} size={64} />
            <span class="name" class:unnamed={!p.name}>{profileName(p)}</span>
            <span class="meta">
              {#if profiles.switching === p.id}Opening…{:else if p.id === profiles.current}Last used{:else}&nbsp;{/if}
            </span>
          </button>
          <button
            type="button"
            class="more"
            aria-label="Edit {profileName(p)}"
            title="Edit"
            onclick={() => (editing = p)}
          >
            <DotsThree size={16} weight="bold" />
          </button>
        </div>
      {/each}
      <div class="tile-wrap" in:blurFade={{ y: 10, delay: 120 + stagger(profiles.list.length), duration: 260 }}>
        <button type="button" class="tile add" data-tile onclick={() => (editing = "new")}>
          <span class="plus"><Plus size={26} /></span>
          <span class="name">Add profile</span>
          <span class="meta">&nbsp;</span>
        </button>
      </div>
    </div>

    {#if confirming}
      <div class="confirm" role="alert" in:blurFade={{ y: 6, duration: 180 }}>
        <span class="flex-1">
          Switching to <b>{profileName(confirming)}</b> closes {sessions.tabs.length} open
          tab{sessions.tabs.length === 1 ? "" : "s"}.
        </span>
        <button type="button" class="btn-ghost" onclick={() => (confirming = null)}>Cancel</button>
        <button type="button" class="btn-primary" onclick={() => confirming && pick(confirming)}>Switch</button>
      </div>
    {/if}

    <label class="check mt-8 text-sm text-muted">
      <input
        type="checkbox"
        checked={profiles.askAtStart}
        onchange={(e) => profiles.setAskAtStart((e.currentTarget as HTMLInputElement).checked)}
      />
      Show this when Hopr opens
    </label>
  </div>
</div>

{#if editing}
  <ProfileDialog profile={editing === "new" ? null : editing} onclose={() => (editing = null)} />
{/if}

<style>
  .picker {
    position: absolute;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    overflow: auto;
    background: var(--bg);
  }  .dismiss {
    position: absolute;
    top: 14px;
    right: 14px;
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
  }
  .dismiss:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .inner {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 48px 24px;
    text-align: center;
  }
  .tiles {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 18px;
    max-width: 880px;
    margin-top: 36px;
  }
  .tile-wrap {
    position: relative;
  }
  .tile {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    width: 156px;
    padding: 26px 12px 16px;
    overflow: hidden;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    outline: none;
    transition:
      transform var(--dur) var(--ease-spring),
      border-color var(--dur-fast) var(--ease-out),
      box-shadow var(--dur) var(--ease-out);
  }
  .tile:hover,
  .tile:focus-visible {
    transform: translateY(-3px);
    border-color: var(--accent);
    box-shadow:
      0 0 0 1px var(--accent),
      var(--shadow-pop);
  }
  .tile:active {
    transform: translateY(-1px) scale(0.98);
  }
  .tile.opening {
    animation: pulse 900ms var(--ease-in-out) infinite;
  }
  @keyframes pulse {
    50% {
      box-shadow: 0 0 0 4px color-mix(in oklab, var(--accent) 35%, transparent);
    }
  }
  /* The profile's surface and accent along the top: its theme at a glance. */
  .stripe {
    position: absolute;
    inset: 0 0 auto;
    height: 4px;
    background: var(--accent);
    opacity: 0.85;
  }
  .name {
    max-width: 100%;
    overflow: hidden;
    font-size: 14px;
    font-weight: 500;
    color: var(--text);
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .name.unnamed {
    font-style: italic;
    color: var(--text-muted);
  }
  .meta {
    font-size: 11.5px;
    color: var(--text-faint);
  }
  .tile.add {
    background: transparent;
    border-style: dashed;
  }
  .plus {
    display: grid;
    place-items: center;
    width: 64px;
    height: 64px;
    color: var(--text-muted);
    background: var(--surface-2);
    border-radius: 999px;
  }
  .more {
    position: absolute;
    top: 8px;
    right: 8px;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    color: var(--text-muted);
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    opacity: 0;
    transition: opacity var(--dur-fast) var(--ease-out);
  }
  .tile-wrap:hover .more,
  .more:focus-visible {
    opacity: 1;
  }
  .more:hover {
    color: var(--text);
  }
  .confirm {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    max-width: 520px;
    margin-top: 24px;
    padding: 8px 8px 8px 14px;
    font-size: 13px;
    text-align: left;
    color: var(--text-muted);
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-left: 2px solid var(--warning);
    border-radius: var(--radius);
  }
</style>
