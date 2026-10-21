<script lang="ts">
import { LockSimple } from "phosphor-svelte";
import HoprMark from "$lib/icons/HoprMark.svelte";
import { blurFade } from "$lib/motion";
import FieldError from "$lib/ui/FieldError.svelte";
import { type FieldErrors, focusFirstInvalid } from "$lib/ui/form";
import { vault } from "./vault.svelte";

let mode = $state<"password" | "recover">("password");
let password = $state("");
let remember = $state(vault.status?.remembered ?? false);
let recoveryKey = $state("");
let newPassword = $state("");
let confirm = $state("");
let busy = $state(false);
let errors = $state<FieldErrors<"password" | "recoveryKey" | "newPassword" | "confirm" | "form">>(
  {},
);
let form = $state<HTMLFormElement>();

const reason = $derived(
  vault.lockReason === "idle"
    ? "Locked after inactivity"
    : vault.lockReason === "manual"
      ? "Locked"
      : "Enter your master password",
);

async function hello() {
  errors = {};
  busy = true;
  const res = await vault.unlockBiometric();
  busy = false;
  if (!res.ok) errors = { form: res.error.message };
}

async function unlock(e: SubmitEvent) {
  e.preventDefault();
  errors = {};
  if (!password) {
    errors.password = "Enter your master password";
    return focusFirstInvalid(form);
  }
  busy = true;
  const res = await vault.unlock(password, remember);
  busy = false;
  if (res.ok) password = "";
  else {
    errors = { password: res.error.message };
    void focusFirstInvalid(form);
  }
}

async function recover(e: SubmitEvent) {
  e.preventDefault();
  errors = {};
  if (!recoveryKey.trim()) errors.recoveryKey = "Enter the recovery key you saved";
  if (newPassword.length < 8)
    errors.newPassword = newPassword ? "Use at least 8 characters" : "Choose a new master password";
  else if (newPassword !== confirm)
    errors.confirm = confirm
      ? "The passwords don't match"
      : "Type the password again to confirm it";
  if (errors.recoveryKey || errors.newPassword || errors.confirm) return focusFirstInvalid(form);
  busy = true;
  const res = await vault.recover(recoveryKey, newPassword);
  busy = false;
  if (res.ok) {
    recoveryKey = "";
    newPassword = "";
    confirm = "";
  } else {
    errors = /recovery/i.test(res.error.message)
      ? { recoveryKey: res.error.message }
      : { form: res.error.message };
    void focusFirstInvalid(form);
  }
}

function switchMode(m: typeof mode) {
  mode = m;
  errors = {};
}
</script>

<div
  class="absolute inset-0 z-50 flex bg-bg"
  role="dialog"
  aria-modal="true"
  aria-label="Unlock Hopr"
  in:blurFade={{ y: 0, scale: 0.985, blur: 8, duration: 260 }}
  out:blurFade={{ y: 0, scale: 1.03, blur: 8, duration: 280 }}
>
  <div class="m-auto flex w-full max-w-sm flex-col gap-6 px-6">
    <span class="text-accent"><HoprMark size={40} /></span>

    {#if mode === "password"}
      <form class="flex flex-col gap-4" onsubmit={unlock} novalidate bind:this={form}>
        <div>
          <h1 class="flex items-center gap-2 text-xl font-medium text-fg">
            <LockSimple size={20} class="text-muted" />
            {reason}
          </h1>
          {#if vault.everUnlocked}
            <p class="mt-1 text-sm text-muted">Your sessions are still running.</p>
          {/if}
        </div>
        <label class="field">
          <span class="label-caps text-faint">Master password</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="input"
            type="password"
            bind:value={password}
            autocomplete="current-password"
            autofocus
            aria-invalid={!!errors.password}
            oninput={() => (errors.password = undefined)}
          />
          <FieldError message={errors.password} />
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={remember} />
          Remember on this device
        </label>
        <FieldError message={errors.form} />
        <button type="submit" class="btn-primary" disabled={busy}>
          {busy ? "Unlocking…" : "Unlock"}
        </button>
        {#if vault.status?.biometric}
          <button type="button" class="btn-ghost" disabled={busy} onclick={hello}>
            Unlock with Windows Hello
          </button>
        {/if}
        <button type="button" class="btn-ghost self-start" onclick={() => switchMode("recover")}>
          Forgot your password?
        </button>
      </form>
    {:else}
      <form class="flex flex-col gap-4" onsubmit={recover} novalidate bind:this={form}>
        <div>
          <h1 class="text-xl font-medium text-fg">Use your recovery key</h1>
          <p class="mt-1 text-sm text-muted">Enter the recovery key you saved, then choose a new password.</p>
        </div>
        <label class="field">
          <span class="label-caps text-faint">Recovery key</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="input font-mono"
            bind:value={recoveryKey}
            placeholder="XXXX-XXXX-XXXX-…"
            spellcheck="false"
            autocomplete="off"
            autofocus
            aria-invalid={!!errors.recoveryKey}
            oninput={() => (errors.recoveryKey = undefined)}
          />
          <FieldError message={errors.recoveryKey} />
        </label>
        <label class="field">
          <span class="label-caps text-faint">New master password</span>
          <input
            class="input"
            type="password"
            bind:value={newPassword}
            autocomplete="new-password"
            aria-invalid={!!errors.newPassword}
            oninput={() => (errors.newPassword = undefined)}
          />
          <FieldError message={errors.newPassword} />
        </label>
        <label class="field">
          <span class="label-caps text-faint">Confirm</span>
          <input
            class="input"
            type="password"
            bind:value={confirm}
            autocomplete="new-password"
            aria-invalid={!!errors.confirm}
            oninput={() => (errors.confirm = undefined)}
          />
          <FieldError message={errors.confirm} />
        </label>
        <FieldError message={errors.form} />
        <button type="submit" class="btn-primary" disabled={busy}>
          {busy ? "Recovering…" : "Set new password"}
        </button>
        <button type="button" class="btn-ghost self-start" onclick={() => switchMode("password")}>
          Back
        </button>
      </form>
    {/if}
  </div>
</div>
