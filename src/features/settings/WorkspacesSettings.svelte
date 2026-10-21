<script lang="ts">
import { sessions } from "$features/terminal/session.svelte";
import { paneCount, parseWorkspace } from "$features/workspace/snapshot";
import { workspaces } from "$features/workspace/workspaces.svelte";
import { inTauri } from "$lib/ipc";
import FieldError from "$lib/ui/FieldError.svelte";
import { toast } from "$lib/ui/toast.svelte";
import { isMainWindow } from "$lib/window";

let name = $state("");
/** Results show as toasts. */
function say(m: { ok: boolean; text: string }) {
  if (m.ok) toast.success(m.text);
  else toast.error(m.text);
}

$effect(() => {
  void workspaces.refresh();
});

let nameError = $state<string | null>(null);

async function save(e: SubmitEvent) {
  e.preventDefault();
  nameError = null;
  if (sessions.tabs.length === 0) {
    toast.info("Open the tabs you want first; a workspace saves the current layout");
    return;
  }
  if (!name.trim()) {
    nameError = "Name this layout, e.g. Deploy day";
    return;
  }
  const res = await workspaces.save(name);
  if (res.ok) {
    say({ ok: true, text: `Saved “${res.data.name}”` });
    name = "";
  } else {
    say({ ok: false, text: res.error.message });
  }
}

function summary(json: string): string {
  const ws = parseWorkspace(json);
  if (!ws) return "empty";
  const t = ws.tabs.length;
  const p = paneCount(ws);
  return `${t} tab${t === 1 ? "" : "s"}${p > t ? `, ${p} panes` : ""}`;
}
</script>

{#if inTauri && isMainWindow}
  <section class="mt-5 flex flex-col gap-2">
    <h3 class="label-caps text-faint">Workspaces</h3>
    <form class="flex flex-col gap-1" onsubmit={save} novalidate>
      <div class="flex gap-2">
        <input
          class="input flex-1"
          bind:value={name}
          placeholder="Name this layout"
          aria-label="Workspace name"
          aria-invalid={!!nameError}
          oninput={() => (nameError = null)}
        />
        <button type="submit" class="btn-secondary">Save current</button>
      </div>
      <FieldError message={nameError} />
    </form>
    {#each workspaces.list as w (w.id)}
      <div class="item">
        <div class="min-w-0 flex-1">
          <p class="truncate text-sm text-fg">{w.name}{w.startup ? " · opens at start" : ""}</p>
          <p class="text-xs text-faint">
            {summary(w.layoutJson)}{w.autosave ? " · saved automatically" : ""}
          </p>
        </div>
        <button type="button" class="btn-ghost" onclick={() => workspaces.open(w)}>Open</button>
        {#if !w.autosave}
          <button
            type="button"
            class="btn-ghost"
            onclick={() => workspaces.setStartup(w.startup ? null : w.id)}
          >
            {w.startup ? "Don't open at start" : "Open at start"}
          </button>
          <button
            type="button"
            class="btn-ghost"
            aria-label="Delete {w.name}"
            onclick={() => workspaces.remove(w.id)}>Delete</button
          >
        {/if}
      </div>
    {:else}
      <p class="text-xs text-muted">
        Your tabs and splits are saved as you work and offered back next time.
      </p>
    {/each}
  </section>
{/if}

<style>
  .item {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 4px 6px 10px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
</style>
