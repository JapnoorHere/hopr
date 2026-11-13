<script lang="ts">
/**
 * Create or edit a profile: a name the person chooses and an avatar colour. Deleting is
 * separate (ProfileDeleteDialog), so Enter here only ever saves.
 */
import { X } from "phosphor-svelte";
import { PROFILE_COLORS } from "$lib/design/themes";
import type { Profile } from "$lib/ipc";
import { sink, springIn } from "$lib/motion";
import ColorSwatches from "$lib/ui/ColorSwatches.svelte";
import FieldError from "$lib/ui/FieldError.svelte";
import Modal from "$lib/ui/Modal.svelte";
import { toast } from "$lib/ui/toast.svelte";
import ProfileAvatar from "./ProfileAvatar.svelte";
import { profiles } from "./profiles.svelte";

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

const preview = $derived<Profile>({
  id: profile?.id ?? "new",
  // Unnamed default profile: shown as "Personal", like everywhere else. A new one: "?".
  name: name.trim() || (profile && !profile.name ? "" : "?"),
  color,
  createdAt: 0,
  look: null,
});

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
</script>

<Modal label={profile ? "Edit profile" : "New profile"} {onclose} layer={90}>
  <form
    class="dialog"
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
      <div class="mt-1.5">
        <ColorSwatches colors={PROFILE_COLORS} value={color} onchange={(c) => (color = c)} />
      </div>
    </div>


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
</Modal>

<style>
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
</style>
