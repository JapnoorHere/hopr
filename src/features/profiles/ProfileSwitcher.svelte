<script lang="ts">
/**
 * The profile chip in the title bar (hidden while a terminal is in front): the open profile,
 * and a menu to switch, add one, open the picker or manage them in Settings.
 */
import { CaretDown, Check, GearSix, Plus, SquaresFour } from "phosphor-svelte";
import { shell } from "$features/shell/shell.state.svelte";
import { sessions } from "$features/terminal/session.svelte";
import type { Profile } from "$lib/ipc";
import { blurFade, sink } from "$lib/motion";
import { toast } from "$lib/ui/toast.svelte";
import ProfileAvatar from "./ProfileAvatar.svelte";
import ProfileDialog from "./ProfileDialog.svelte";
import { profileName, profiles } from "./profiles.svelte";

let open = $state(false);
let creating = $state(false);
let confirming = $state<Profile | null>(null);
let root = $state<HTMLDivElement>();

const active = $derived(profiles.active);

async function pick(p: Profile) {
  if (p.id === profiles.current) {
    open = false;
    return;
  }
  if (sessions.tabs.length > 0 && confirming?.id !== p.id) {
    confirming = p;
    return;
  }
  const r = await profiles.open(p.id);
  if (!r.ok) toast.error(r.error.message);
}

function close() {
  open = false;
  confirming = null;
}

function onWindowPointer(e: PointerEvent) {
  if (open && e.target instanceof Node && !root?.contains(e.target)) close();
}
</script>

<svelte:window
  onpointerdown={onWindowPointer}
  onkeydown={(e) => {
    if (open && e.key === "Escape") {
      e.stopImmediatePropagation();
      close();
    }
  }}
/>

{#if active}
  <div class="relative flex items-center" bind:this={root}>
    <button
      type="button"
      class="chip"
      class:on={open}
      aria-haspopup="menu"
      aria-expanded={open}
      title="Profile: {profileName(active)}"
      onclick={() => (open ? close() : (open = true))}
    >
      <ProfileAvatar profile={active} size={18} />
      <span class="max-w-36 truncate">{profileName(active)}</span>
      <CaretDown size={11} class="text-faint" />
    </button>

    {#if open}
      <div
        class="menu"
        role="menu"
        aria-label="Profiles"
        in:blurFade={{ y: -4, scale: 0.97, blur: 2, duration: 160 }}
        out:sink={{ y: -4, scale: 0.97, duration: 100 }}
      >
        <p class="head">Profiles</p>
        {#each profiles.list as p (p.id)}
          <button type="button" role="menuitem" class="item" onclick={() => pick(p)}>
            <ProfileAvatar profile={p} size={22} />
            <span class="min-w-0 flex-1 truncate" class:italic={!p.name}>{profileName(p)}</span>
            {#if p.id === profiles.current}<Check size={13} weight="bold" class="text-accent" />{/if}
            {#if profiles.switching === p.id}<span class="text-xs text-faint">Opening…</span>{/if}
          </button>
        {/each}
        {#if confirming}
          <div class="confirm" in:blurFade={{ y: -2, duration: 140 }}>
            <p>
              Switching closes {sessions.tabs.length} open tab{sessions.tabs.length === 1 ? "" : "s"}.
            </p>
            <div class="mt-2 flex justify-end gap-1.5">
              <button type="button" class="btn-ghost" onclick={() => (confirming = null)}>Cancel</button>
              <button type="button" class="btn-primary" onclick={() => confirming && pick(confirming)}
                >Switch to {profileName(confirming)}</button
              >
            </div>
          </div>
        {/if}
        <hr class="sep" />
        <button
          type="button"
          role="menuitem"
          class="item"
          onclick={() => {
            close();
            creating = true;
          }}
        >
          <span class="ico"><Plus size={14} /></span> New profile…
        </button>
        <button
          type="button"
          role="menuitem"
          class="item"
          onclick={() => {
            close();
            profiles.fromMenu = true;
            profiles.picking = true;
          }}
        >
          <span class="ico"><SquaresFour size={14} /></span> Show profile picker
        </button>
        <button
          type="button"
          role="menuitem"
          class="item"
          onclick={() => {
            close();
            shell.openSettings("profiles");
          }}
        >
          <span class="ico"><GearSix size={14} /></span> Manage profiles…
        </button>
      </div>
    {/if}
  </div>
{/if}

{#if creating}
  <ProfileDialog onclose={() => (creating = false)} />
{/if}

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 28px;
    margin: 0 4px;
    padding: 0 8px 0 5px;
    font-size: 12.5px;
    color: var(--text-muted);
    border: 1px solid transparent;
    border-radius: 999px;
    transition:
      background var(--dur-fast) var(--ease-out),
      border-color var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out);
  }
  .chip:hover,
  .chip.on {
    color: var(--text);
    background: var(--surface-3);
    border-color: var(--border);
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 70;
    display: flex;
    width: 260px;
    flex-direction: column;
    padding: 4px;
    background: color-mix(in oklab, var(--surface-3) 97%, transparent);
    backdrop-filter: blur(var(--blur));
    border: 1px solid var(--border-strong, var(--border));
    border-radius: var(--radius);
    box-shadow: var(--shadow-pop);
  }
  .head {
    padding: 6px 10px 4px;
    font-family: var(--font-label);
    font-size: 10.5px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-faint);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 9px;
    min-height: 32px;
    padding: 4px 10px;
    font-size: 13px;
    text-align: left;
    color: var(--text);
    border-radius: var(--radius-sm);
  }
  .item:hover,
  .item:focus-visible {
    background: var(--surface-4);
  }
  .ico {
    display: grid;
    place-items: center;
    width: 22px;
    color: var(--text-muted);
  }
  .sep {
    margin: 4px 6px;
    border: 0;
    border-top: 1px solid var(--border);
  }
  .confirm {
    margin: 4px;
    padding: 8px 10px;
    font-size: 12px;
    color: var(--text-muted);
    background: var(--surface-2);
    border-left: 2px solid var(--warning);
    border-radius: var(--radius-sm);
  }
</style>
