<script lang="ts">
/**
 * Create or edit a profile: a name the person chooses and an avatar colour. Editing also
 * offers delete, guarded by typing the profile's name.
 */
import { Check, Trash, X } from "phosphor-svelte";
import { fade } from "svelte/transition";
import { PROFILE_COLORS } from "$lib/design/themes";
import type { Profile } from "$lib/ipc";
import { sink, springIn } from "$lib/motion";
import { motion } from "$lib/platform";
import FieldError from "$lib/ui/FieldError.svelte";
import { toast } from "$lib/ui/toast.svelte";
import ProfileAvatar from "./ProfileAvatar.svelte";
import { profileName, profiles } from "./profiles.svelte";

let { profile = null, onclose }: { profile?: Profile | null; onclose: () => void } = $props();

// svelte-ignore state_referenced_locally
let name = $state(profile?.name ?? "");
// svelte-ignore state_referenced_locally
let color = $state<string>(
  profile?.color ??
    PROFILE_COLORS[profiles.list.length % PROFILE_COLORS.length] ??
    PROFILE_COLORS[0],
);
let error = $state<string | null>(null);
let busy = $state(false);
let deleting = $state(false);
let confirmName = $state("");

const preview = $derived<Profile>({
  id: profile?.id ?? "new",
  name: name.trim() || "?",
  color,
  createdAt: 0,
  look: null,
});
const isOpen = $derived(profile?.id === profiles.current);

async function submit(open: boolean) {
  error = null;
  busy = true;
  if (profile) {
    const r = await profiles.update(profile.id, { name, color });
    busy = false;
    if (!r.ok) {
      error = r.error.message;
      return;
    }
    toast.success("Profile saved");
    onclose();
    return;
  }
  const r = await profiles.create(name, color);
  if (!r.ok) {
    busy = false;
    error = r.error.message;
    return;
  }
  if (open) {
    await profiles.open(r.data.id);
    return;
  }
  busy = false;
  toast.success(`Profile “${r.data.name}” created`);
  onclose();
}

async function remove() {
  if (!profile) return;
  busy = true;
  const r = await profiles.remove(profile.id);
  busy = false;
  if (!r.ok) {
    error = r.error.message;
    return;
  }
  toast.success(`Deleted “${profileName(profile)}”`);
  onclose();
}

function onkey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.preventDefault();
    e.stopImmediatePropagation();
    onclose();
  }
}
</script>

<svelte:window onkeydowncapture={onkey} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<div
  class="scrim"
  role="dialog"
  aria-modal="true"
  aria-label={profile ? "Edit profile" : "New profile"}
  tabindex="-1"
  transition:fade={{ duration: motion(140) }}
  onclick={onclose}
>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <form
    class="dialog"
    onclick={(e) => e.stopPropagation()}
    onsubmit={(e) => {
      e.preventDefault();
      void submit(!profile);
    }}
    novalidate
    in:springIn={{ y: 10, scale: 0.97 }}
    out:sink={{ y: 6, scale: 0.98, duration: 120 }}
  >
    <header class="flex items-center gap-3">
      <ProfileAvatar profile={preview} size={44} />
      <div class="min-w-0 flex-1">
        <h2 class="text-base font-semibold text-fg">{profile ? "Edit profile" : "New profile"}</h2>
        <p class="text-xs text-muted">
          {profile ? "Change its name or colour." : "Its own hosts, keys, history and password."}
        </p>
      </div>
      <button type="button" class="close" aria-label="Close" onclick={onclose}><X size={15} /></button>
    </header>

    <label class="field mt-5">
      <span class="text-xs text-muted">Name</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="input"
        bind:value={name}
        maxlength="40"
        placeholder="Name this profile"
        aria-invalid={!!error}
        oninput={() => (error = null)}
        autofocus
        spellcheck="false"
      />
    </label>
    {#if error}<div class="mt-1.5"><FieldError message={error} /></div>{/if}

    <div class="mt-4">
      <span class="text-xs text-muted">Colour</span>
      <div class="swatches" role="radiogroup" aria-label="Colour">
        {#each PROFILE_COLORS as c (c)}
          <button
            type="button"
            role="radio"
            aria-checked={color === c}
            aria-label="Colour {c}"
            class="swatch"
            class:on={color === c}
            style:background={c}
            onclick={() => (color = c)}
          >
            {#if color === c}<Check size={12} weight="bold" />{/if}
          </button>
        {/each}
      </div>
    </div>

    {#if profile}
      <div class="danger-zone">
        {#if !deleting}
          <button type="button" class="link-danger" disabled={isOpen} onclick={() => (deleting = true)}>
            <Trash size={13} /> Delete profile…
          </button>
          {#if isOpen}<span class="text-xs text-faint">It's open now; switch to another profile to delete it.</span>{/if}
        {:else}
          <p class="text-xs text-muted">
            This deletes <b class="text-fg">{profileName(profile)}</b>'s hosts, keys, snippets, history and backups on
            this computer. Type its name to confirm.
          </p>
          <div class="mt-2 flex gap-2">
            <input
              class="input flex-1"
              bind:value={confirmName}
              placeholder={profileName(profile)}
              aria-label="Type the profile name to confirm"
              spellcheck="false"
            />
            <button
              type="button"
              class="btn-danger"
              disabled={busy || confirmName.trim() !== profileName(profile)}
              onclick={remove}>Delete</button
            >
          </div>
        {/if}
      </div>
    {/if}

    <footer class="mt-6 flex justify-end gap-2">
      <button type="button" class="btn-ghost" onclick={onclose}>Cancel</button>
      {#if profile}
        <button type="submit" class="btn-primary" disabled={busy}>Save</button>
      {:else}
        <button type="button" class="btn-ghost" disabled={busy} onclick={() => submit(false)}>Create</button>
        <button type="submit" class="btn-primary" disabled={busy}>Create and open</button>
      {/if}
    </footer>
  </form>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: grid;
    place-items: center;
    padding: 24px;
    background: color-mix(in oklab, var(--bg) 55%, transparent);
    backdrop-filter: blur(6px);
  }
  .dialog {
    width: min(420px, 100%);
    padding: 20px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-pop);
  }
  .close {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
  }
  .close:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 6px;
  }
  .swatch {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    color: var(--bg);
    border-radius: 999px;
    box-shadow: inset 0 0 0 1px var(--border);
    transition: transform var(--dur-fast) var(--ease-spring);
  }
  .swatch:hover {
    transform: scale(1.1);
  }
  .swatch.on {
    box-shadow:
      0 0 0 2px var(--surface-2),
      0 0 0 4px var(--text);
  }
  .danger-zone {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 20px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
  }
  .link-danger {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    width: fit-content;
    font-size: 13px;
    color: var(--danger);
  }
  .link-danger:disabled {
    opacity: 0.45;
  }
</style>
