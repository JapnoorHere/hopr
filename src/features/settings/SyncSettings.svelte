<script lang="ts">
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { ArrowsClockwise } from "phosphor-svelte";
import { syncStore } from "$features/sync/sync.svelte";
import { inTauri } from "$lib/ipc";
import { pop } from "$lib/motion";
import FieldError from "$lib/ui/FieldError.svelte";
import { type FieldErrors, focusFirstInvalid, hasErrors } from "$lib/ui/form";
import { toast } from "$lib/ui/toast.svelte";

let folder = $state("");
let deviceName = $state("");
let passphrase = $state("");
let confirmText = $state("");
let errors = $state<FieldErrors<"folder" | "deviceName" | "passphrase" | "confirm" | "form">>({});
let form = $state<HTMLFormElement>();

$effect(() => {
  void syncStore.refresh();
});

async function choose() {
  const picked = await pickFolder({ directory: true, title: "Folder to sync through" });
  if (typeof picked === "string") folder = picked;
}

async function enable(e: SubmitEvent) {
  e.preventDefault();
  errors = {};
  if (!folder.trim()) errors.folder = "Choose the folder to sync through";
  if (!deviceName.trim()) errors.deviceName = "Name this computer, e.g. Work laptop";
  if (!passphrase) errors.passphrase = "Enter the sync passphrase";
  else if (passphrase.length < 8) errors.passphrase = "Use at least 8 characters";
  else if (confirmText && confirmText !== passphrase)
    errors.confirm = "The passphrases don't match";
  if (hasErrors(errors)) return focusFirstInvalid(form);
  if (await syncStore.enable(folder, passphrase, deviceName)) {
    passphrase = "";
    confirmText = "";
    toast.success("Sync is on");
    return;
  }
  const m = syncStore.error ?? "Couldn't turn sync on";
  const low = m.toLowerCase();
  errors = low.includes("passphrase")
    ? { passphrase: m }
    : low.includes("folder")
      ? { folder: m }
      : low.includes("device") || low.includes("name")
        ? { deviceName: m }
        : { form: m };
  void focusFirstInvalid(form);
}

async function syncNow() {
  if (await syncStore.now()) {
    const l = syncStore.last;
    toast.success(l ? `Synced · ${l.applied} changed, ${l.deleted} removed` : "Synced");
  } else toast.error(syncStore.error ?? "Sync failed");
}

async function turnOff() {
  await syncStore.disable();
  toast.success("Sync turned off on this computer");
}

function ago(iso: string | null): string {
  if (!iso) return "never";
  const s = Math.round((Date.now() - Date.parse(iso)) / 1000);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  return new Date(iso).toLocaleString();
}
</script>

{#if inTauri}
  <section class="mt-5 flex flex-col gap-2" aria-label="Sync">
    <h3 class="label-caps text-faint">Sync</h3>
    {#if syncStore.status?.enabled}
      {@const s = syncStore.status}
      <p class="text-sm text-fg" role="status">
        Syncing through <code class="text-xs">{s.folder}</code> as “{s.deviceName}”.
      </p>
      <p class="text-xs text-muted">
        Last synced {ago(s.lastSync)}{s.devices.length ? " · with" : " · no other devices yet."}
      </p>
      {#if s.devices.length}
        <div class="flex flex-wrap gap-1.5">
          {#each s.devices as d, i (d.name)}
            <span class="device" in:pop={{ delay: i * 60 }}>{d.name}</span>
          {/each}
        </div>
      {/if}
      {#if s.lastError}<p class="text-xs text-danger" role="alert">{s.lastError}</p>{/if}
      {#if syncStore.last}
        <p class="text-xs text-muted" aria-label="Last sync result">
          {syncStore.last.applied} changed, {syncStore.last.deleted} removed here · {syncStore.last.pushed} records shared
        </p>
        {#each syncStore.last.warnings as w, i (i)}<p class="text-xs text-warning">{w}</p>{/each}
      {/if}
      <div class="flex gap-2">
        <button type="button" class="btn-ghost" disabled={syncStore.busy} onclick={syncNow}>
          <ArrowsClockwise size={14} class={syncStore.busy ? "spin" : ""} />
          {syncStore.busy ? "Syncing…" : "Sync now"}
        </button>
        <button type="button" class="btn-ghost" onclick={turnOff}>Turn off</button>
      </div>
    {:else}
      <p class="text-xs text-muted">
        Keep hosts, keys, snippets, secrets and tunnels the same on your computers, through a folder you already sync
        (OneDrive, Dropbox, a network share). Everything in it is encrypted with your sync passphrase; the folder's
        provider can't read it.
      </p>
      <form class="flex flex-col gap-2" onsubmit={enable} novalidate bind:this={form}>
        <div class="flex gap-2">
          <input
            class="input min-w-0 flex-1 font-mono text-xs"
            bind:value={folder}
            placeholder="Folder"
            aria-label="Sync folder"
            spellcheck="false"
            aria-invalid={!!errors.folder}
            oninput={() => (errors.folder = undefined)}
          />
          <button
            type="button"
            class="btn-secondary"
            onclick={async () => {
              await choose();
              if (folder) errors.folder = undefined;
            }}>Choose…</button
          >
        </div>
        <FieldError message={errors.folder} />
        <input
          class="input"
          bind:value={deviceName}
          placeholder="This computer's name, e.g. Work laptop"
          aria-label="Device name"
          aria-invalid={!!errors.deviceName}
          oninput={() => (errors.deviceName = undefined)}
        />
        <FieldError message={errors.deviceName} />
        <input
          class="input"
          type="password"
          bind:value={passphrase}
          placeholder="Sync passphrase"
          aria-label="Sync passphrase"
          aria-invalid={!!errors.passphrase}
          oninput={() => (errors.passphrase = undefined)}
        />
        <FieldError message={errors.passphrase} />
        <input
          class="input"
          type="password"
          bind:value={confirmText}
          placeholder="Repeat it (first computer only)"
          aria-label="Repeat sync passphrase"
          aria-invalid={!!errors.confirm}
          oninput={() => (errors.confirm = undefined)}
        />
        <FieldError message={errors.confirm} />
        <p class="text-xs text-faint">
          Use the same passphrase on every computer. It isn't your master password and can't be recovered if lost.
        </p>
        <FieldError message={errors.form} />
        <button type="submit" class="btn-primary self-start" disabled={syncStore.busy}>
          {syncStore.busy ? "Setting up…" : "Turn on sync"}
        </button>
      </form>
    {/if}
  </section>
{/if}

<style>
  .device {
    padding: 1px 8px;
    font-size: 11px;
    color: var(--text-muted);
    background: var(--surface-3);
    border-radius: 999px;
  }
  :global(.spin) {
    animation: sync-spin 0.9s linear infinite;
  }
  @keyframes sync-spin {
    to {
      transform: rotate(360deg);
    }
  }
  :global(:root[data-motion="reduced"] .spin) {
    animation: none;
  }
</style>
