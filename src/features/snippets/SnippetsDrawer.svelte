<script lang="ts">
import { Key, PencilSimple, Play, Plus, Trash } from "phosphor-svelte";
import { shell } from "$features/shell/shell.state.svelte";
import type { RunMode, Snippet } from "$lib/ipc";
import Drawer from "$lib/ui/Drawer.svelte";
import FieldError from "$lib/ui/FieldError.svelte";
import { type FieldErrors, focusFirstInvalid, hasErrors } from "$lib/ui/form";
import { snippets } from "./snippets.svelte";
import { secretsIn } from "./template";

const MODES: { id: RunMode; label: string }[] = [
  { id: "paste", label: "Type it (no Enter)" },
  { id: "paste_enter", label: "Type it and press Enter" },
  { id: "confirm", label: "Show it, run on confirm" },
];

let name = $state("");
let folder = $state("");
let tags = $state("");
let description = $state("");
let body = $state("");
let runMode = $state<RunMode>("paste_enter");
let errors = $state<FieldErrors<"name" | "body" | "form">>({});
let secretName = $state("");
let secretValue = $state("");
let secretErrors = $state<FieldErrors<"name" | "value">>({});
let editor = $state<HTMLFormElement>();
let secretForm = $state<HTMLFormElement>();

$effect(() => {
  void snippets.refresh();
});

// Opening the editor (new, edit, or "Save as snippet") fills the form.
$effect(() => {
  const id = snippets.editing;
  if (id === null) return;
  const s = id === "new" ? null : snippets.list.find((x) => x.id === id);
  name = s?.name ?? "";
  folder = s?.folder ?? "";
  tags = s?.tags.join(", ") ?? "";
  description = s?.description ?? "";
  body = s?.body ?? snippets.draftBody;
  runMode = s?.runMode ?? "paste_enter";
  errors = {};
});

async function save(e: SubmitEvent) {
  e.preventDefault();
  errors = {};
  if (!name.trim()) errors.name = "Name the snippet";
  if (!body.trim()) errors.body = "Write the command to run";
  if (hasErrors(errors)) return focusFirstInvalid(editor);
  const failed = await snippets.save(snippets.editing === "new" ? null : snippets.editing, {
    name,
    folder,
    tags: tags.split(",").map((t) => t.trim()),
    description,
    body,
    runMode,
  });
  if (failed) {
    const m = failed.toLowerCase();
    errors = m.includes("name")
      ? { name: failed }
      : m.includes("snippet")
        ? { body: failed }
        : { form: failed };
    void focusFirstInvalid(editor);
  }
}

async function addSecret(e: SubmitEvent) {
  e.preventDefault();
  secretErrors = {};
  if (!secretName.trim()) secretErrors.name = "Name the secret, e.g. db_password";
  if (!secretValue) secretErrors.value = "Enter the secret's value";
  if (hasErrors(secretErrors)) return focusFirstInvalid(secretForm);
  const failed = await snippets.setSecret(secretName.trim(), secretValue);
  if (failed) {
    secretErrors = { name: failed };
    return focusFirstInvalid(secretForm);
  }
  secretName = "";
  secretValue = "";
}

const groups = $derived.by(() => {
  const by = new Map<string, Snippet[]>();
  for (const s of snippets.shown) by.set(s.folder, [...(by.get(s.folder) ?? []), s]);
  return [...by.entries()];
});
const missing = $derived(secretsIn(body).filter((n) => !snippets.secrets.includes(n)));
</script>

<Drawer title="Snippets" width="480px" onclose={() => shell.close()}>
  <div class="flex flex-col gap-3 pt-1">
    {#if snippets.pending}
      {@const p = snippets.pending}
      <section class="panel flex flex-col gap-2" aria-label="Run snippet">
        <p class="text-sm text-fg">
          {p.snippet.name} <span class="text-xs text-muted">→ {p.session.label}</span>
        </p>
        {#if p.text === null}
          <form
            class="flex flex-col gap-2"
            onsubmit={(e) => {
              e.preventDefault();
              void snippets.fill();
            }}
          >
            {#each p.fields as f (f.key)}
              <label class="field">
                <span class="label-caps text-faint">{f.label}</span>
                {#if f.kind === "choice"}
                  <select class="input" bind:value={p.values[f.key]} aria-label={f.label}>
                    {#each f.options as o (o)}<option value={o}>{o}</option>{/each}
                  </select>
                {:else}
                  <input
                    class="input font-mono text-xs"
                    bind:value={p.values[f.key]}
                    aria-label={f.label}
                    spellcheck="false"
                  />
                {/if}
              </label>
            {/each}
            <div class="flex justify-end gap-2">
              <button type="button" class="btn-ghost" onclick={() => (snippets.pending = null)}>Cancel</button>
              <button type="submit" class="btn-primary">Continue</button>
            </div>
          </form>
        {:else}
          <pre class="preview" aria-label="Command to run">{p.text}</pre>
          <p class="text-sm text-fg" role="alert">{p.confirm}</p>
          <div class="flex justify-end gap-2">
            <button type="button" class="btn-ghost" onclick={() => snippets.confirm(false)}>Cancel</button>
            <button type="button" class="btn-primary" onclick={() => snippets.confirm(true)}>Run</button>
          </div>
        {/if}
      </section>
    {/if}

    {#if snippets.editing !== null}
      <form class="panel flex flex-col gap-2" onsubmit={save} aria-label="Snippet editor" novalidate bind:this={editor}>
        <div class="grid grid-cols-2 gap-2">
          <input
            class="input"
            bind:value={name}
            placeholder="Name"
            aria-label="Snippet name"
            aria-invalid={!!errors.name}
            oninput={() => (errors.name = undefined)}
          />
          <input class="input" bind:value={folder} placeholder="Folder (e.g. Ops/Postgres)" aria-label="Snippet folder" />
        </div>
        <FieldError message={errors.name} />
        <textarea
          class="input font-mono text-xs"
          rows="5"
          bind:value={body}
          placeholder={"systemctl restart {{service:nginx|caddy}}"}
          aria-label="Snippet body"
          spellcheck="false"
          aria-invalid={!!errors.body}
          oninput={() => (errors.body = undefined)}
        ></textarea>
        <FieldError message={errors.body} />
        <p class="text-xs text-faint">
          Variables: {"{{name}}"}, {"{{name:default}}"}, {"{{name:a|b|c}}"}, {"{{secret:x}}"}; built-ins {"{{host}}"}
          {"{{user}}"}
          {"{{date}}"}
          {"{{clipboard}}"}.
        </p>
        {#if missing.length}
          <p class="text-xs text-warning" role="note">
            Add the secret{missing.length === 1 ? "" : "s"}
            {missing.join(", ")} below.
          </p>
        {/if}
        <input class="input" bind:value={description} placeholder="What it does (optional)" aria-label="Snippet description" />
        <div class="grid grid-cols-2 gap-2">
          <select class="input" bind:value={runMode} aria-label="Run mode">
            {#each MODES as m (m.id)}<option value={m.id}>{m.label}</option>{/each}
          </select>
          <input class="input" bind:value={tags} placeholder="Tags, comma separated" aria-label="Snippet tags" />
        </div>
        <FieldError message={errors.form} />
        <div class="flex justify-end gap-2">
          <button type="button" class="btn-ghost" onclick={() => (snippets.editing = null)}>Cancel</button>
          <button type="submit" class="btn-primary">Save</button>
        </div>
      </form>
    {:else}
      <div class="flex gap-2">
        <input
          class="input min-w-0 flex-1"
          bind:value={snippets.query}
          placeholder="Search snippets"
          aria-label="Search snippets"
        />
        <button type="button" class="btn-primary" onclick={() => snippets.newFrom("")}><Plus size={14} /> New</button>
      </div>
    {/if}


    {#if snippets.list.length === 0 && snippets.editing === null}
      <p class="py-4 text-center text-sm text-muted">
        Save commands you type often. Click one to run it in the focused terminal; it asks for its variables first.
      </p>
    {/if}
    {#each groups as [dir, list] (dir)}
      <section aria-label={dir || "Snippets"}>
        {#if dir}<h3 class="label-caps mb-1 text-faint">{dir}</h3>{/if}
        {#each list as s (s.id)}
          <div class="row" data-snippet={s.name}>
            <button
              type="button"
              class="run"
              aria-label="Run {s.name}"
              title="Run in the focused terminal"
              onclick={() => snippets.run(s)}
            >
              <Play size={13} weight="fill" />
            </button>
            <div class="min-w-0 flex-1">
              <span class="block truncate text-sm text-fg">{s.name}</span>
              <code class="block truncate text-xs text-muted">{s.body}</code>
            </div>
            <button type="button" class="act" aria-label="Edit {s.name}" onclick={() => (snippets.editing = s.id)}>
              <PencilSimple size={14} />
            </button>
            <button type="button" class="act" aria-label="Delete {s.name}" onclick={() => snippets.remove(s.id)}>
              <Trash size={14} />
            </button>
          </div>
        {/each}
      </section>
    {/each}

    <section class="mt-3" aria-label="Secrets">
      <h3 class="label-caps mb-1 text-faint">Secrets</h3>
      <p class="mb-2 text-xs text-muted">
        Use as {"{{secret:name}}"}. Stored encrypted in the vault and typed straight into the terminal; Hopr's window
        never sees them.
      </p>
      {#each snippets.secrets as n (n)}
        <div class="row">
          <Key size={14} class="text-faint" />
          <code class="flex-1 text-sm text-fg">{n}</code>
          <button type="button" class="act" aria-label="Delete secret {n}" onclick={() => snippets.removeSecret(n)}>
            <Trash size={14} />
          </button>
        </div>
      {/each}
      <form class="mt-2 flex flex-col gap-1" onsubmit={addSecret} novalidate bind:this={secretForm}>
        <div class="flex gap-2">
          <input
            class="input w-32"
            bind:value={secretName}
            placeholder="name"
            aria-label="Secret name"
            spellcheck="false"
            aria-invalid={!!secretErrors.name}
            oninput={() => (secretErrors.name = undefined)}
          />
          <input
            class="input min-w-0 flex-1"
            type="password"
            bind:value={secretValue}
            placeholder="value"
            aria-label="Secret value"
            aria-invalid={!!secretErrors.value}
            oninput={() => (secretErrors.value = undefined)}
          />
          <button type="submit" class="btn-secondary">Save</button>
        </div>
        <FieldError message={secretErrors.name} />
        <FieldError message={secretErrors.value} />
      </form>
    </section>
  </div>
</Drawer>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 4px;
    border-bottom: 1px solid var(--border);
  }
  .run {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    color: var(--accent);
    border: 1px solid var(--border);
    border-radius: 50%;
  }
  .run:hover {
    background: var(--surface-3);
  }
  .act {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    color: var(--text-muted);
    border-radius: 4px;
  }
  .act:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .preview {
    max-height: 160px;
    overflow: auto;
    padding: 8px;
    font-family: var(--font-mono);
    font-size: 12px;
    white-space: pre-wrap;
    background: var(--surface-1);
    border-radius: var(--radius-sm);
  }
</style>
