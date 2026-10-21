<script lang="ts">
import { Check, Copy, ShieldCheck } from "phosphor-svelte";
import HoprMark from "$lib/icons/HoprMark.svelte";
import { blurFade } from "$lib/motion";
import FieldError from "$lib/ui/FieldError.svelte";
import { type FieldErrors, focusFirstInvalid } from "$lib/ui/form";
import { toast } from "$lib/ui/toast.svelte";
import { passwordStrength } from "./strength";
import { vault } from "./vault.svelte";

let password = $state("");
let confirm = $state("");
let busy = $state(false);
let errors = $state<FieldErrors<"password" | "confirm" | "form">>({});
let form = $state<HTMLFormElement>();
let recoveryKey = $state("");
let copied = $state(false);
let saved = $state(false);

const strength = $derived(passwordStrength(password));

async function create(e: SubmitEvent) {
  e.preventDefault();
  errors = {};
  if (!password) errors.password = "Choose a master password";
  else if (password.length < 8) errors.password = "Use at least 8 characters";
  else if (!confirm) errors.confirm = "Type the password again to confirm it";
  else if (password !== confirm) errors.confirm = "The passwords don't match";
  if (errors.password || errors.confirm) return focusFirstInvalid(form);
  busy = true;
  const res = await vault.create(password);
  busy = false;
  if (!res.ok) {
    errors = { form: res.error.message };
    return;
  }
  password = "";
  confirm = "";
  recoveryKey = res.data;
}

async function copy() {
  await navigator.clipboard.writeText(recoveryKey);
  copied = true;
  toast.success("Recovery key copied");
  setTimeout(() => (copied = false), 1500);
}

function finish() {
  recoveryKey = "";
  vault.showRecovery = false;
}
</script>

<div class="m-auto flex w-full max-w-sm flex-col gap-6 px-6">
  <span class="text-accent"><HoprMark size={40} /></span>

  {#if !recoveryKey}
    <form class="flex flex-col gap-4" onsubmit={create} novalidate bind:this={form}>
      <div>
        <h1 class="text-xl font-medium text-fg">Welcome to Hopr</h1>
        <p class="mt-1 text-sm text-muted">
          Choose a master password. It encrypts your saved hosts, passwords and keys on this device.
        </p>
      </div>
      <label class="field">
        <span class="label-caps text-faint">Master password</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="input"
          type="password"
          bind:value={password}
          autocomplete="new-password"
          autofocus
          aria-invalid={!!errors.password}
          oninput={() => (errors.password = undefined)}
        />
        <FieldError message={errors.password} />
      </label>
      {#if password}
        <div class="flex items-center gap-2" aria-live="polite">
          <div class="flex flex-1 gap-1">
            {#each [1, 2, 3] as n (n)}
              <span class="meter" class:on={strength.score >= n} data-score={strength.score}></span>
            {/each}
          </div>
          <span class="text-xs text-muted">{strength.label}</span>
        </div>
      {/if}
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
        {busy ? "Creating vault…" : "Create vault"}
      </button>
    </form>
  {:else}
    <div class="flex flex-col gap-4">
      <div>
        <h1 class="flex items-center gap-2 text-xl font-medium text-fg">
          <ShieldCheck size={22} class="text-success" /> Save your recovery key
        </h1>
        <p class="mt-1 text-sm text-muted">
          If you forget your master password, this key is the only way back in. Hopr can't recover it for
          you. Store it somewhere safe, like a password manager or on paper.
        </p>
      </div>
      <div class="panel items-center" in:blurFade={{ y: 8, blur: 6, duration: 320 }}>
        <code class="reveal font-mono text-base tracking-wide break-all text-fg select-text" data-testid="recovery-key"
          >{recoveryKey}</code
        >
        <button type="button" class="btn-ghost" onclick={copy}>
          {#if copied}<Check size={14} /> Copied{:else}<Copy size={14} /> Copy{/if}
        </button>
      </div>
      <label class="check">
        <input type="checkbox" bind:checked={saved} />
        I've saved my recovery key
      </label>
      <button type="button" class="btn-primary" disabled={!saved} onclick={finish}>Continue</button>
    </div>
  {/if}
</div>

<style>
  .meter {
    height: 4px;
    flex: 1;
    border-radius: 2px;
    background: var(--surface-3);
    transition: background var(--dur-slow) var(--ease-out);
  }
  /* The recovery key is uncovered left to right once, so it reads as something new. */
  .reveal {
    animation: key-reveal 700ms var(--ease-out) 1 both;
  }
  @keyframes key-reveal {
    from {
      clip-path: inset(0 100% 0 0);
      filter: blur(2px);
    }
  }
  :global(:root[data-motion="reduced"]) .reveal {
    animation: none;
  }
  .meter.on[data-score="1"] {
    background: var(--danger);
  }
  .meter.on[data-score="2"] {
    background: var(--warning);
  }
  .meter.on[data-score="3"] {
    background: var(--success);
  }
</style>
