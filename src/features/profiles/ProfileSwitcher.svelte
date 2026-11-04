<script lang="ts">
/**
 * The profile chip in the title bar (hidden while a terminal is in front): the open profile,
 * and a menu to switch, add one, open the picker or manage them in Settings.
 */
import { CaretDown, Check, GearSix, Plus, SquaresFour } from "phosphor-svelte";
import { shell } from "$features/shell/shell.state.svelte";
import { sessions } from "$features/terminal/session.svelte";
import type { Profile } from "$lib/ipc";
import ConfirmBar from "$lib/ui/ConfirmBar.svelte";
import Menu from "$lib/ui/Menu.svelte";
import { toast } from "$lib/ui/toast.svelte";
import ProfileAvatar from "./ProfileAvatar.svelte";
import ProfileDialog from "./ProfileDialog.svelte";
import { profileName, profiles } from "./profiles.svelte";

let open = $state(false);
let creating = $state(false);
let confirming = $state<Profile | null>(null);

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
</script>

{#if active}
  <div class="flex items-center">
    <Menu
      label="Profiles"
      align="start"
      width="260px"
      closeOnSelect={false}
      {open}
      onopenchange={(o) => (o ? (open = true) : close())}
    >
      {#snippet trigger(props)}
        <button type="button" class="chip" class:on={open} title="Profile: {profileName(active)}" {...props}>
          <ProfileAvatar profile={active} size={18} />
          <span class="max-w-36 truncate">{profileName(active)}</span>
          <CaretDown size={11} class="text-faint" />
        </button>
      {/snippet}
      <p class="menu-head">Profiles</p>
      {#each profiles.list as p (p.id)}
        <button type="button" role="menuitem" class="menu-item" onclick={() => pick(p)}>
          <ProfileAvatar profile={p} size={22} />
          <span class="min-w-0 flex-1 truncate" class:italic={!p.name}>{profileName(p)}</span>
          {#if p.id === profiles.current}<Check size={13} weight="bold" class="text-accent" />{/if}
          {#if profiles.switching === p.id}<span class="text-xs text-faint">Opening…</span>{/if}
        </button>
      {/each}
      {#if confirming}
        <div class="m-1">
          <ConfirmBar
            confirmLabel="Switch to {profileName(confirming)}"
            onconfirm={() => confirming && pick(confirming)}
            oncancel={() => (confirming = null)}
          >
            Switching closes {sessions.tabs.length} open tab{sessions.tabs.length === 1 ? "" : "s"}.
          </ConfirmBar>
        </div>
      {/if}
      <hr class="menu-sep" />
      <button
        type="button"
        role="menuitem"
        class="menu-item"
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
        class="menu-item"
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
        class="menu-item"
        onclick={() => {
          close();
          shell.openSettings("profiles");
        }}
      >
        <span class="ico"><GearSix size={14} /></span> Manage profiles…
      </button>
    </Menu>
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
  .ico {
    display: grid;
    place-items: center;
    width: 22px;
    color: var(--text-muted);
  }
</style>
