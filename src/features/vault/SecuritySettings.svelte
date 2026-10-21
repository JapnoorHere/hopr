<script lang="ts">
import { shell } from "$features/shell/shell.state.svelte";
import { inTauri } from "$lib/ipc";
import { vault } from "./vault.svelte";

const CLIPBOARD = [
  { value: 15, label: "15 seconds" },
  { value: 30, label: "30 seconds" },
  { value: 60, label: "1 minute" },
  { value: 120, label: "2 minutes" },
  { value: 0, label: "Never" },
];

async function setHello(e: Event) {
  const input = e.currentTarget as HTMLInputElement;
  const res = await vault.setBiometric(input.checked);
  if (!res.ok) {
    input.checked = !input.checked;
    message = { ok: false, text: res.error.message };
  }
}

async function setClipboard(e: Event) {
  const seconds = Number((e.currentTarget as HTMLSelectElement).value);
  const res = await vault.setClipboardClear(seconds);
  if (!res.ok) message = { ok: false, text: res.error.message };
}

const AUTO_LOCK = [
  { value: 5, label: "5 minutes" },
  { value: 15, label: "15 minutes" },
  { value: 30, label: "30 minutes" },
  { value: 60, label: "1 hour" },
  { value: 240, label: "4 hours" },
  { value: 0, label: "Never" },
];

let changing = $state(false);
let current = $state("");
let next = $state("");
let confirm = $state("");
let message = $state<{ ok: boolean; text: string } | null>(null);
let busy = $state(false);

async function setAutoLock(e: Event) {
  const minutes = Number((e.currentTarget as HTMLSelectElement).value);
  const res = await vault.setAutoLock(minutes);
  if (!res.ok) message = { ok: false, text: res.error.message };
}

async function change(e: SubmitEvent) {
  e.preventDefault();
  if (next !== confirm) {
    message = { ok: false, text: "The new passwords don't match" };
    return;
  }
  busy = true;
  const res = await vault.changePassword(current, next);
  busy = false;
  if (!res.ok) {
    message = { ok: false, text: res.error.message };
    return;
  }
  current = "";
  next = "";
  confirm = "";
  changing = false;
  message = { ok: true, text: "Master password changed" };
}

function startChange() {
  changing = true;
  message = null;
}
</script>

{#if inTauri}
  <section class="mt-5">
    <h3 class="label-caps mb-2 text-faint">Security</h3>
    <label class="field">
      <span class="text-xs text-muted">Lock after inactivity</span>
      <select class="input" value={vault.status?.autoLockMinutes ?? 15} onchange={setAutoLock}>
        {#each AUTO_LOCK as o (o.value)}
          <option value={o.value}>{o.label}</option>
        {/each}
      </select>
    </label>
    <label class="field mt-2">
      <span class="text-xs text-muted">Clear copied passwords from the clipboard after</span>
      <select class="input" value={vault.status?.clipboardClearSecs ?? 30} onchange={setClipboard}>
        {#each CLIPBOARD as o (o.value)}
          <option value={o.value}>{o.label}</option>
        {/each}
      </select>
    </label>
    {#if vault.status?.biometricAvailable || vault.status?.biometric}
      <label class="check mt-2">
        <input type="checkbox" checked={vault.status?.biometric ?? false} onchange={setHello} />
        Unlock with Windows Hello (face, fingerprint or PIN)
      </label>
    {/if}

    {#if changing}
      <form class="mt-3 flex flex-col gap-2" onsubmit={change}>
        <input class="input" type="password" bind:value={current} placeholder="Current password" aria-label="Current password" />
        <input class="input" type="password" bind:value={next} placeholder="New password" aria-label="New password" />
        <input
          class="input"
          type="password"
          bind:value={confirm}
          placeholder="Confirm new password"
          aria-label="Confirm new password"
        />
        <div class="flex justify-end gap-2">
          <button type="button" class="btn-ghost" onclick={() => (changing = false)}>Cancel</button>
          <button type="submit" class="btn-primary" disabled={busy || !current || !next}>Change</button>
        </div>
      </form>
    {:else}
      <div class="mt-3 flex flex-wrap gap-2">
        <button type="button" class="btn-ghost" onclick={startChange}>Change master password</button>
        {#if vault.status?.remembered}
          <button type="button" class="btn-ghost" onclick={() => vault.forgetDevice()}>Forget this device</button>
        {/if}
        <button type="button" class="btn-ghost" onclick={() => vault.lock()}>Lock now</button>
        <button type="button" class="btn-ghost" onclick={() => shell.toggle("audit")}>Audit log</button>
      </div>
    {/if}
    {#if message}
      <p class="mt-2 text-xs" class:text-success={message.ok} class:text-danger={!message.ok} role="status">
        {message.text}
      </p>
    {/if}
  </section>
{/if}
