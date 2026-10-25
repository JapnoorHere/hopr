<script lang="ts">
/** Settings > Profiles: every profile, with edit and open, and the start-up picker toggle. */
import { ArrowRight, PencilSimple, Plus } from "phosphor-svelte";
import { sessions } from "$features/terminal/session.svelte";
import type { Profile } from "$lib/ipc";
import { toast } from "$lib/ui/toast.svelte";
import ProfileAvatar from "./ProfileAvatar.svelte";
import ProfileDialog from "./ProfileDialog.svelte";
import { profileName, profiles } from "./profiles.svelte";

let editing = $state<Profile | null | "new">(null);
let confirming = $state<Profile | null>(null);

const since = (p: Profile) =>
  p.createdAt
    ? new Date(p.createdAt * 1000).toLocaleDateString(undefined, { dateStyle: "medium" })
    : "";

async function open(p: Profile) {
  if (sessions.tabs.length > 0 && confirming?.id !== p.id) {
    confirming = p;
    return;
  }
  const r = await profiles.open(p.id);
  if (!r.ok) toast.error(r.error.message);
}
</script>

<div class="group flex flex-col gap-3">
  <div class="flex items-center gap-2">
    <h3 class="label-caps flex-1 text-faint">Profiles</h3>
    <button type="button" class="btn-ghost" onclick={() => (editing = "new")}><Plus size={14} /> New profile</button>
  </div>
  <p class="-mt-1 text-xs text-muted">
    Each profile is separate: its own hosts, keys, snippets, history, backups and master password. Switching
    restarts Hopr into the other profile.
  </p>
  <ul class="list">
    {#each profiles.list as p (p.id)}
      <li class="row">
        <ProfileAvatar profile={p} size={32} />
        <span class="min-w-0 flex-1">
          <span class="block truncate text-sm text-fg" class:italic={!p.name}>{profileName(p)}</span>
          <span class="block text-xs text-faint">
            {p.id === profiles.current ? "Open now" : since(p) ? `Created ${since(p)}` : "Your original data"}
          </span>
        </span>
        <button type="button" class="btn-ghost" aria-label="Edit {profileName(p)}" onclick={() => (editing = p)}>
          <PencilSimple size={14} /> Edit
        </button>
        {#if p.id !== profiles.current}
          <button type="button" class="btn-ghost" onclick={() => open(p)}>
            {profiles.switching === p.id ? "Opening…" : "Open"}
            <ArrowRight size={13} />
          </button>
        {/if}
      </li>
      {#if confirming?.id === p.id}
        <li class="confirm">
          <span class="flex-1">Switching closes {sessions.tabs.length} open tab{sessions.tabs.length === 1 ? "" : "s"}.</span>
          <button type="button" class="btn-ghost" onclick={() => (confirming = null)}>Cancel</button>
          <button type="button" class="btn-primary" onclick={() => open(p)}>Switch</button>
        </li>
      {/if}
    {/each}
  </ul>
  <label class="check text-sm">
    <input
      type="checkbox"
      checked={profiles.askAtStart}
      onchange={(e) => profiles.setAskAtStart((e.currentTarget as HTMLInputElement).checked)}
    />
    Ask which profile to open when Hopr starts
  </label>
</div>

{#if editing}
  <ProfileDialog profile={editing === "new" ? null : editing} onclose={() => (editing = null)} />
{/if}

<style>
  .list {
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
  }
  .row + .row,
  .confirm {
    border-top: 1px solid var(--border);
  }
  .confirm {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    font-size: 12.5px;
    color: var(--text-muted);
    background: color-mix(in oklab, var(--warning) 8%, transparent);
  }
</style>
