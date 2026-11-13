<script lang="ts">
/**
 * Delete a profile, on its own: typing the profile's name arms it, and Enter then deletes
 * (it can't be mistaken for saving an edit). The open profile can't be deleted.
 */
import { Trash, X } from "phosphor-svelte";
import type { Profile } from "$lib/ipc";
import { sink, springIn } from "$lib/motion";
import FieldError from "$lib/ui/FieldError.svelte";
import Modal from "$lib/ui/Modal.svelte";
import { toast } from "$lib/ui/toast.svelte";
import ProfileAvatar from "./ProfileAvatar.svelte";
import { profileName, profiles } from "./profiles.svelte";

let { profile, onclose }: { profile: Profile; onclose: () => void } = $props();

let typed = $state("");
let busy = $state(false);
let error = $state<string | null>(null);
let field = $state<HTMLInputElement>();

// Ready to type straight away (after the menu that opened this has let go of focus).
$effect(() => {
  const el = field;
  if (!el) return;
  const raf = requestAnimationFrame(() => el.focus());
  return () => cancelAnimationFrame(raf);
});

const name = $derived(profileName(profile));
const isOpen = $derived(profile.id === profiles.current);
const armed = $derived(!isOpen && typed.trim() === name);

async function remove() {
  if (!armed || busy) return;
  busy = true;
  const r = await profiles.remove(profile.id);
  busy = false;
  if (!r.ok) {
    error = r.error.message;
    return;
  }
  toast.success(`Deleted “${name}”`);
  onclose();
}
</script>

<Modal label="Delete profile" {onclose} layer={90}>
  <form
    class="dialog"
    onsubmit={(e) => {
      e.preventDefault();
      void remove();
    }}
    novalidate
    in:springIn={{ y: 10, scale: 0.97 }}
    out:sink={{ y: 6, scale: 0.98, duration: 120 }}
  >
    <header class="flex items-center gap-3">
      <ProfileAvatar {profile} size={44} />
      <div class="min-w-0 flex-1">
        <h2 class="text-base font-semibold text-fg">Delete “{name}”?</h2>
        <p class="text-xs text-muted">This can't be undone.</p>
      </div>
      <button type="button" class="close" aria-label="Close" onclick={onclose}><X size={15} /></button>
    </header>

    {#if isOpen}
      <p class="mt-5 text-sm text-muted">
        <b class="text-fg">{name}</b> is open right now. Switch to another profile first, then delete it from there.
      </p>
      <footer class="mt-6 flex justify-end">
        <button type="button" class="btn-primary" onclick={onclose}>OK</button>
      </footer>
    {:else}
      <p class="mt-5 text-sm text-muted">
        Its hosts, keys, snippets, history and backups on this computer are deleted. Type
        <b class="text-fg">{name}</b> to confirm.
      </p>
      <input
        bind:this={field}
        class="input mt-3 w-full"
        bind:value={typed}
        placeholder={name}
        aria-label="Type the profile name to confirm"
        aria-invalid={!!error}
        oninput={() => (error = null)}
        spellcheck="false"
        autocomplete="off"
      />
      {#if error}<div class="mt-1.5"><FieldError message={error} /></div>{/if}
      <footer class="mt-6 flex justify-end gap-2">
        <button type="button" class="btn-ghost" onclick={onclose}>Cancel</button>
        <button type="submit" class="btn-danger" disabled={!armed || busy}>
          <Trash size={14} />
          {busy ? "Deleting…" : "Delete profile"}
        </button>
      </footer>
    {/if}
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
