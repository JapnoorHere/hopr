<script lang="ts">
import { Check, Copy, PaperPlaneTilt, Trash } from "phosphor-svelte";
import { tick } from "svelte";
import { flip } from "svelte/animate";
import { library } from "$features/hosts/library.svelte";
import { shell } from "$features/shell/shell.state.svelte";
import { wrap } from "$features/vault/vault.svelte";
import {
  type CertificateInfo,
  commands,
  inTauri,
  type KeyAlgorithm,
  type KnownHost,
} from "$lib/ipc";
import { blurFade, flash, pop } from "$lib/motion";
import { motion } from "$lib/platform";
import Drawer from "$lib/ui/Drawer.svelte";
import FieldError from "$lib/ui/FieldError.svelte";
import { type FieldErrors, focusFirstInvalid, hasErrors } from "$lib/ui/form";
import { toast } from "$lib/ui/toast.svelte";

// Opened from the host editor's "Generate key" / "Import key": start on that panel.
// svelte-ignore state_referenced_locally
let mode = $state<"list" | "generate" | "import">(shell.keysIntent);
shell.keysIntent = "list";
let errors = $state<FieldErrors<"genLabel" | "impLabel" | "impText" | "impPassphrase">>({});
let form = $state<HTMLFormElement>();
let busy = $state(false);
let copiedId = $state<string | null>(null);

let genLabel = $state("");
let genAlgorithm = $state<KeyAlgorithm>("ed25519");
let genComment = $state("");

let impLabel = $state("");
let impText = $state("");
let impPassphrase = $state("");
let needsPassphrase = $state(false);

let known = $state<KnownHost[]>([]);

$effect(() => {
  void library.refresh();
  void loadKnown();
});

async function loadKnown() {
  if (!inTauri) return;
  const r = await commands.knownHostsList();
  if (r.status === "ok") known = r.data;
}

function reset(m: typeof mode) {
  mode = m;
  errors = {};
  needsPassphrase = false;
}

async function copy(id: string, text: string) {
  await navigator.clipboard.writeText(text);
  copiedId = id;
  toast.success("Public key copied");
  setTimeout(() => (copiedId = null), 1500);
}

/** After a key is added: select it in an open host editor and go back there. */
async function added(label: string, id: string) {
  library.addedKeyId = id;
  await library.refresh();
  if (shell.overEditor) {
    toast.success(`Key “${label}” added and selected`);
    shell.close();
  } else {
    toast.success(`Key “${label}” added`);
    reset("list");
    // The new key glows as it lands in the list.
    await tick();
    flash(document.querySelector(`[data-key-id="${id}"]`));
  }
}

async function generate(e: SubmitEvent) {
  e.preventDefault();
  errors = genLabel.trim() ? {} : { genLabel: "Name this key, e.g. work laptop" };
  if (hasErrors(errors)) return focusFirstInvalid(form);
  busy = true;
  const res = wrap(await commands.keyGenerate(genLabel.trim(), genAlgorithm, genComment));
  busy = false;
  if (!res.ok) {
    toast.error(res.error.message);
    return;
  }
  const label = genLabel.trim();
  genLabel = "";
  genComment = "";
  await added(label, res.data.id);
}

async function importKey(e: SubmitEvent) {
  e.preventDefault();
  errors = {};
  if (!impLabel.trim()) errors.impLabel = "Name this key";
  if (!impText.trim()) errors.impText = "Choose the private key file, or paste the key";
  if (hasErrors(errors)) return focusFirstInvalid(form);
  busy = true;
  const res = wrap(await commands.keyImport(impLabel.trim(), impText, impPassphrase || null));
  busy = false;
  if (!res.ok) {
    const needs = res.error.code === "passphrase_required";
    needsPassphrase ||= needs;
    if (needs) {
      errors = {
        impPassphrase: impPassphrase
          ? "That passphrase doesn't open this key"
          : "This key is protected; enter its passphrase",
      };
    } else if (/passphrase/i.test(res.error.message)) {
      errors = { impPassphrase: res.error.message };
    } else {
      errors = { impText: res.error.message };
    }
    return focusFirstInvalid(form);
  }
  const label = impLabel.trim();
  impLabel = "";
  impText = "";
  impPassphrase = "";
  await added(label, res.data.id);
}

async function readFile(e: Event) {
  const file = (e.currentTarget as HTMLInputElement).files?.[0];
  if (!file) return;
  if (file.size > 64 * 1024) {
    errors = { ...errors, impText: "That file is too large to be a private key" };
    return;
  }
  impText = await file.text();
  if (!impLabel) impLabel = file.name;
  errors = { ...errors, impText: undefined, impLabel: impLabel ? undefined : errors.impLabel };
}

async function removeKey(id: string) {
  const label = library.keys.find((k) => k.id === id)?.label ?? "Key";
  const res = wrap(await commands.keyDelete(id));
  if (!res.ok) {
    toast.error(res.error.message);
    return;
  }
  toast.success(`“${label}” deleted`);
  await library.refresh();
}

/** Key whose "attach certificate" panel is open. */
let certFor = $state<string | null>(null);
let certText = $state("");

async function readCert(e: Event) {
  const file = (e.currentTarget as HTMLInputElement).files?.[0];
  if (file) certText = await file.text();
}

async function saveCert(id: string, text: string | null) {
  const res = wrap(await commands.keySetCertificate(id, text));
  if (!res.ok) {
    toast.error(res.error.message);
    return;
  }
  toast.success(text ? "Certificate attached" : "Certificate removed");
  certFor = null;
  certText = "";
  await library.refresh();
}

function untilText(c: CertificateInfo): string {
  if (c.validBefore === null) return "never expires";
  const when = new Date(c.validBefore * 1000).toLocaleString();
  return c.expired ? `expired ${when}` : `valid until ${when}`;
}

async function forget(id: string) {
  await commands.knownHostDelete(id);
  toast.success("Host key forgotten");
  await loadKnown();
}

function knownName(k: KnownHost): string {
  if (k.hostPattern.startsWith("|1|")) return "Hashed entry";
  return k.port === 22 ? k.hostPattern : `${k.hostPattern}:${k.port}`;
}
</script>

<Drawer title="Keys" onclose={() => shell.close()}>
  {#if mode === "list"}
    <div class="flex gap-2 pt-2">
      <button type="button" class="btn-primary flex-1" onclick={() => reset("generate")}>Generate</button>
      <button type="button" class="btn-secondary flex-1" onclick={() => reset("import")}>Import</button>
    </div>

    <section class="mt-4">
      <h3 class="label-caps mb-2 text-faint">Your keys</h3>
      {#each library.keys as k, i (k.id)}
        <div
          class="item"
          data-key-id={k.id}
          in:blurFade={{ delay: i < 8 ? i * 20 : 0, y: 6, blur: 0, duration: 200 }}
          animate:flip={{ duration: motion(200) }}
        >
          <div class="min-w-0 flex-1">
            <p class="truncate text-sm text-fg">{k.label}</p>
            <p class="truncate font-mono text-xs text-faint" title={k.fingerprint}>{k.fingerprint}</p>
            <p class="text-xs text-muted">
              {k.algorithm} · {k.usedBy ? `used by ${k.usedBy} host${k.usedBy === 1 ? "" : "s"}` : "unused"}
            </p>
            {#if k.certificateInfo}
              {@const c = k.certificateInfo}
              <p class="text-xs" class:text-danger={c.expired} class:text-muted={!c.expired} data-cert={k.label}>
                Certificate “{c.keyId}” for {c.principals.length ? c.principals.join(", ") : "any user"} · {untilText(c)}
                <button type="button" class="link" onclick={() => saveCert(k.id, null)}>Remove</button>
              </p>
            {:else if certFor !== k.id}
              <button type="button" class="link text-xs" onclick={() => ((certFor = k.id), (certText = ""))}>
                Add certificate…
              </button>
            {/if}
            {#if certFor === k.id}
              <div class="mt-1 flex flex-col gap-1">
                <textarea
                  class="input font-mono text-xs"
                  rows="3"
                  bind:value={certText}
                  placeholder="ssh-ed25519-cert-v01@openssh.com AAAA…"
                  aria-label="Certificate for {k.label}"
                  spellcheck="false"
                ></textarea>
                <div class="flex items-center gap-2">
                  <input type="file" accept=".pub,.txt" onchange={readCert} aria-label="Certificate file" class="text-xs" />
                  <span class="flex-1"></span>
                  <button type="button" class="btn-ghost" onclick={() => (certFor = null)}>Cancel</button>
                  <button type="button" class="btn-primary" disabled={!certText.trim()} onclick={() => saveCert(k.id, certText)}>
                    Attach
                  </button>
                </div>
              </div>
            {/if}
          </div>
          <button
            type="button"
            class="icon"
            aria-label="Copy public key of {k.label}"
            onclick={() => copy(k.id, k.publicKey)}
          >
            {#if copiedId === k.id}<span class="grid text-success" in:pop><Check size={14} weight="bold" /></span>{:else}<Copy size={14} />{/if}
          </button>
          <button
            type="button"
            class="icon"
            aria-label="Deploy {k.label} to a host"
            title="Deploy to a host"
            disabled={library.hosts.length === 0}
            onclick={() => {
              shell.close();
              library.deploying = { hostId: null, keyId: k.id };
            }}
          >
            <PaperPlaneTilt size={14} />
          </button>
          <button type="button" class="icon" aria-label="Delete {k.label}" onclick={() => removeKey(k.id)}>
            <Trash size={14} />
          </button>
        </div>
      {:else}
        <p class="text-sm text-muted">No keys yet. Generate one, or import an existing private key.</p>
      {/each}
    </section>

    <section class="mt-5">
      <h3 class="label-caps mb-2 text-faint">Trusted host keys</h3>
      {#each known as k (k.id)}
        <div class="item">
          <div class="min-w-0 flex-1">
            <p class="truncate text-sm text-fg">{knownName(k)}</p>
            <p class="truncate font-mono text-xs text-faint" title={k.fingerprint}>{k.fingerprint}</p>
            <p class="text-xs text-muted">{k.keyType} · {k.source}</p>
          </div>
          <button type="button" class="icon" aria-label="Forget the key for {knownName(k)}" onclick={() => forget(k.id)}>
            <Trash size={14} />
          </button>
        </div>
      {:else}
        <p class="text-sm text-muted">Hosts you trust on first connect appear here.</p>
      {/each}
    </section>
  {:else if mode === "generate"}
    <form class="flex flex-col gap-4 pt-2" onsubmit={generate} novalidate bind:this={form}>
      <label class="field">
        <span class="label-caps text-faint">Name</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="input"
          bind:value={genLabel}
          placeholder="work laptop"
          autofocus
          aria-invalid={!!errors.genLabel}
          oninput={() => (errors.genLabel = undefined)}
        />
        <FieldError message={errors.genLabel} />
      </label>
      <label class="field">
        <span class="label-caps text-faint">Type</span>
        <select class="input" bind:value={genAlgorithm}>
          <option value="ed25519">Ed25519 (recommended)</option>
          <option value="ecdsa_p256">ECDSA P-256</option>
          <option value="rsa3072">RSA 3072</option>
          <option value="rsa4096">RSA 4096</option>
        </select>
      </label>
      <label class="field">
        <span class="label-caps text-faint">Comment</span>
        <input class="input" bind:value={genComment} placeholder="me@laptop" />
      </label>
      <div class="flex justify-end gap-2">
        <button type="button" class="btn-ghost" onclick={() => reset("list")}>Cancel</button>
        <button type="submit" class="btn-primary" disabled={busy}>{busy ? "Generating…" : "Generate"}</button>
      </div>
    </form>
  {:else}
    <form class="flex flex-col gap-4 pt-2" onsubmit={importKey} novalidate bind:this={form}>
      <label class="field">
        <span class="label-caps text-faint">Name</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="input"
          bind:value={impLabel}
          placeholder="id_ed25519"
          autofocus
          aria-invalid={!!errors.impLabel}
          oninput={() => (errors.impLabel = undefined)}
        />
        <FieldError message={errors.impLabel} />
      </label>
      <label class="field">
        <span class="label-caps text-faint">Private key file</span>
        <input class="input" type="file" onchange={readFile} aria-invalid={!!errors.impText && !impText} />
      </label>
      <label class="field">
        <span class="label-caps text-faint">Or paste it</span>
        <textarea
          class="input"
          bind:value={impText}
          rows="5"
          placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
          spellcheck="false"
          aria-invalid={!!errors.impText && !!impText}
          oninput={() => (errors.impText = undefined)}
        ></textarea>
        <FieldError message={errors.impText} />
      </label>
      {#if needsPassphrase}
        <label class="field">
          <span class="label-caps text-faint">Passphrase</span>
          <input
            class="input"
            type="password"
            bind:value={impPassphrase}
            aria-invalid={!!errors.impPassphrase}
            oninput={() => (errors.impPassphrase = undefined)}
          />
          <FieldError message={errors.impPassphrase} />
        </label>
      {/if}
      <p class="text-xs text-muted">
        OpenSSH, PEM, PKCS#8 and PuTTY keys work. The key is stored encrypted in your vault, without its old
        passphrase.
      </p>
      <div class="flex justify-end gap-2">
        <button type="button" class="btn-ghost" onclick={() => reset("list")}>Cancel</button>
        <button type="submit" class="btn-primary" disabled={busy}>{busy ? "Importing…" : "Import"}</button>
      </div>
    </form>
  {/if}
</Drawer>

<style>
  .link {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 4px 8px 10px;
    margin-bottom: 4px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    flex-shrink: 0;
    color: var(--text-faint);
    border-radius: 4px;
  }
  .icon:hover {
    color: var(--text);
    background: var(--surface-3);
  }
</style>
