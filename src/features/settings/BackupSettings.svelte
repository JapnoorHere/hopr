<script lang="ts">
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { relaunch } from "@tauri-apps/plugin-process";
import { prefs } from "$features/terminal/prefs.svelte";
import { wrap } from "$features/vault/vault.svelte";
import { appearance } from "$lib/design/appearance.svelte";
import { parseAppearance } from "$lib/design/theme";
import { saveText } from "$lib/download";
import { type BackupInfo, commands, inTauri } from "$lib/ipc";
import { shortcuts } from "$lib/shortcuts";
import { toast } from "$lib/ui/toast.svelte";

/** Results show as toasts. */
function say(m: { ok: boolean; text: string }) {
  if (m.ok) toast.success(m.text);
  else toast.error(m.text);
}
let backups = $state<BackupInfo[]>([]);
let busy = $state(false);
/** The backup being restored: a saved one by name, or a picked file's bytes. */
let restoring = $state<{ name: string | null; bytes: number[] | null; label: string } | null>(null);
let password = $state("");
let pending = $state(false);

$effect(() => {
  void loadBackups();
});

async function loadBackups() {
  if (!inTauri) return;
  const r = await commands.backupsList();
  if (r.status === "ok") backups = r.data;
  pending = await commands.backupRestorePending();
}

/** Look and keys only: never hosts or secrets (those live in the vault). */
function exportSettings() {
  const data = {
    hopr: "settings",
    version: 1,
    appearance: appearance.current,
    terminal: prefs.current,
    keybindings: shortcuts.overrides(),
  };
  saveText("hopr-settings.json", JSON.stringify(data, null, 2), "application/json");
  say({ ok: true, text: "Exported hopr-settings.json" });
}

async function importSettings(e: Event) {
  const file = (e.currentTarget as HTMLInputElement).files?.[0];
  if (!file) return;
  try {
    const data = JSON.parse(await file.text()) as Record<string, unknown>;
    if (data.hopr !== "settings") throw new Error("not a Hopr settings file");
    appearance.update(parseAppearance(data.appearance));
    prefs.update(data.terminal as object);
    shortcuts.replaceOverrides(data.keybindings);
    say({ ok: true, text: "Settings imported" });
  } catch (err) {
    const why = err instanceof Error ? err.message : "invalid file";
    say({ ok: false, text: `Could not import: ${why}` });
  }
}

async function backupNow() {
  busy = true;
  const res = wrap(await commands.backupNow());
  busy = false;
  say(
    res.ok
      ? { ok: true, text: `Backed up to ${res.data.name}` }
      : { ok: false, text: res.error.message },
  );
  await loadBackups();
}

async function pickFile(e: Event) {
  const input = e.currentTarget as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file) return;
  restoring = {
    name: null,
    bytes: Array.from(new Uint8Array(await file.arrayBuffer())),
    label: file.name,
  };
}

async function restore(e: SubmitEvent) {
  e.preventDefault();
  const r = restoring;
  if (!r) return;
  busy = true;
  const res = wrap(await commands.backupRestore(r.name, r.bytes, password));
  busy = false;
  password = "";
  if (!res.ok) {
    say({ ok: false, text: res.error.message });
    return;
  }
  restoring = null;
  pending = true;
  say({ ok: true, text: `${r.label} will be restored when Hopr restarts.` });
}

async function showFolder() {
  const dir = await commands.backupsDir();
  await revealItemInDir(dir).catch(() => undefined);
}

function when(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
}

function size(bytes: number): string {
  return bytes > 1024 * 1024
    ? `${(bytes / 1024 / 1024).toFixed(1)} MB`
    : `${Math.ceil(bytes / 1024)} KB`;
}
</script>

<section class="mt-5 flex flex-col gap-2">
  <h3 class="label-caps text-faint">Backup</h3>
  {#if inTauri}
    <p class="text-xs text-muted">
      Everything in your vault (hosts, keys, passwords, settings), encrypted with your master password. One is made
      automatically each day; the last 7 are kept.
    </p>
    <div class="flex flex-wrap gap-2">
      <button type="button" class="btn-ghost" disabled={busy} onclick={backupNow}>Back up now</button>
      <label class="btn-ghost">
        Restore from file…
        <input type="file" accept=".hopr" class="hidden" onchange={pickFile} aria-label="Restore from a .hopr file" />
      </label>
      <button type="button" class="btn-ghost" onclick={showFolder}>Show folder</button>
    </div>
    {#if pending}
      <div class="flex items-center gap-2 rounded-md border border-border px-3 py-2">
        <span class="flex-1 text-xs text-fg">A restore is ready. It replaces your current data when Hopr restarts.</span>
        <button type="button" class="btn-primary" onclick={() => relaunch()}>Restart now</button>
      </div>
    {/if}
    {#if restoring}
      <form class="flex flex-col gap-2 rounded-md border border-border p-3" onsubmit={restore}>
        <span class="text-xs text-fg">Restore {restoring.label}? Your current data is kept as a copy next to it.</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="input"
          type="password"
          bind:value={password}
          placeholder="Master password of that backup"
          aria-label="Backup master password"
          autofocus
        />
        <div class="flex justify-end gap-2">
          <button type="button" class="btn-ghost" onclick={() => (restoring = null)}>Cancel</button>
          <button type="submit" class="btn-primary" disabled={busy || !password}>Restore</button>
        </div>
      </form>
    {/if}
    <ul class="flex flex-col gap-1" aria-label="Backups">
      {#each backups as b (b.name)}
        <li class="item">
          <span class="min-w-0 flex-1">
            <span class="block truncate text-sm text-fg">{when(b.createdAt)}</span>
            <span class="block text-xs text-faint">{b.automatic ? "Daily" : "Manual"} · {size(b.size)}</span>
          </span>
          <button type="button" class="btn-ghost" onclick={() => (restoring = { name: b.name, bytes: null, label: when(b.createdAt) })}>
            Restore
          </button>
        </li>
      {:else}
        <li class="text-xs text-faint">No backups yet.</li>
      {/each}
    </ul>
  {/if}

  <p class="mt-2 text-xs text-muted">Appearance, terminal settings and shortcuts as a file:</p>
  <div class="flex gap-2">
    <button type="button" class="btn-ghost" onclick={exportSettings}>Export settings</button>
    <label class="btn-ghost">
      Import settings
      <input type="file" accept="application/json,.json" class="hidden" onchange={importSettings} />
    </label>
  </div>
</section>

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
