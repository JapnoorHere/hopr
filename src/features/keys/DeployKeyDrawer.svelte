<script lang="ts">
import { untrack } from "svelte";
import { library } from "$features/hosts/library.svelte";
import { shell } from "$features/shell/shell.state.svelte";
import { wrap } from "$features/vault/vault.svelte";
import { commands } from "$lib/ipc";
import Drawer from "$lib/ui/Drawer.svelte";
import FieldError from "$lib/ui/FieldError.svelte";
import { focusFirstInvalid } from "$lib/ui/form";
import { toast } from "$lib/ui/toast.svelte";

let { hostId: initialHost, keyId: initialKey }: { hostId: string | null; keyId: string | null } =
  $props();

// svelte-ignore state_referenced_locally
let hostId = $state(initialHost ?? library.hosts[0]?.id ?? "");
// svelte-ignore state_referenced_locally
let keyId = $state(initialKey ?? library.keys[0]?.id ?? "");
let password = $state("");
let useKey = $state(true);
let busy = $state(false);
let error = $state<string | null>(null);
let done = $state<string | null>(null);
let passwordError = $state<string | null>(null);
let form = $state<HTMLFormElement>();

// A key generated or imported from here is picked straight away.
$effect(() => {
  const added = library.addedKeyId;
  const first = library.keys[0]?.id;
  untrack(() => {
    if (added && library.keys.some((k) => k.id === added)) {
      keyId = added;
      library.addedKeyId = null;
    } else if (!keyId && first) keyId = first;
  });
});

const host = $derived(library.host(hostId));
const key = $derived(library.keys.find((k) => k.id === keyId));
/** A password is asked for only when the host logs in with one and none is saved. */
const needsPassword = $derived(!!host && host.auth === "password" && !host.hasPassword);
const alreadyUses = $derived(!!host && host.auth === "key" && host.keyId === keyId);

async function deploy(e: SubmitEvent) {
  e.preventDefault();
  if (!host || !key) return;
  passwordError = null;
  if (needsPassword && !password) {
    passwordError = `Enter the password for ${host.username}@${host.hostname} to log in once`;
    return focusFirstInvalid(form);
  }
  busy = true;
  error = null;
  done = null;
  const res = wrap(
    await commands.keyDeploy(host.id, key.id, password || null, useKey && !alreadyUses),
  );
  busy = false;
  password = "";
  if (!res.ok) {
    if (needsPassword && /password|auth/i.test(res.error.message)) {
      passwordError = res.error.message;
      void focusFirstInvalid(form);
    } else error = res.error.message;
    return;
  }
  const what = res.data.added
    ? `Added ${key.label} to ${host.label}`
    : `${key.label} was already on ${host.label}`;
  done = res.data.switched ? `${what}. ${host.label} now logs in with this key.` : `${what}.`;
  toast.success(done);
  await library.refresh();
}
</script>

<Drawer title="Deploy key" onclose={() => (library.deploying = null)}>
  <form class="flex flex-col gap-4 pt-2" onsubmit={deploy} autocomplete="off" novalidate bind:this={form}>
    <p class="text-xs text-muted">
      Adds the public key to <span class="font-mono">~/.ssh/authorized_keys</span> on the host, once, and fixes the
      folder and file permissions. Running it again changes nothing.
    </p>
    <label class="field">
      <span class="label-caps text-faint">Key</span>
      {#if library.keys.length}
        <select class="input" bind:value={keyId} aria-label="Key to deploy">
          {#each library.keys as k (k.id)}
            <option value={k.id}>{k.label} ({k.algorithm})</option>
          {/each}
        </select>
      {:else}
        <span class="text-sm text-muted">No SSH keys in Hopr yet.</span>
        <span class="flex gap-2">
          <button type="button" class="btn-primary" onclick={() => shell.openKeys("generate")}>Generate key</button>
          <button type="button" class="btn-secondary" onclick={() => shell.openKeys("import")}>Import key</button>
        </span>
      {/if}
    </label>
    <label class="field">
      <span class="label-caps text-faint">Host</span>
      <select class="input" bind:value={hostId} aria-label="Host to deploy to">
        {#each library.hosts as h (h.id)}
          <option value={h.id}>{h.label} ({h.username}@{h.hostname})</option>
        {/each}
      </select>
    </label>
    {#if needsPassword}
      <label class="field">
        <span class="label-caps text-faint">Password for {host?.username}@{host?.hostname}</span>
        <input
          class="input"
          type="password"
          bind:value={password}
          aria-label="Host password"
          aria-invalid={!!passwordError}
          oninput={() => (passwordError = null)}
        />
        <FieldError message={passwordError} />
        <span class="text-xs text-faint">Used for this login only, not saved.</span>
      </label>
    {/if}
    {#if !alreadyUses}
      <label class="check">
        <input type="checkbox" bind:checked={useKey} /> Log in to {host?.label ?? "the host"} with this key from now on
      </label>
    {/if}
    <FieldError message={error} />
    {#if done}<p class="text-sm text-success" role="status">{done}</p>{/if}
    <div class="flex justify-end gap-2">
      <button type="button" class="btn-ghost" onclick={() => (library.deploying = null)}>{done ? "Done" : "Cancel"}</button>
      <button type="submit" class="btn-primary" disabled={busy || !host || !key}>
        {busy ? "Deploying…" : "Deploy"}
      </button>
    </div>
  </form>
</Drawer>
