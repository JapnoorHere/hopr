<script lang="ts">
import { open as pickFile } from "@tauri-apps/plugin-dialog";
import { shell } from "$features/shell/shell.state.svelte";
import { wrap } from "$features/vault/vault.svelte";
import { t } from "$lib/i18n/i18n.svelte";
import {
  type ConfigImport,
  commands,
  type ForeignHost,
  type ForeignImport,
  type ForeignSource,
  type ImportSummary,
  inTauri,
} from "$lib/ipc";
import { blurFade, stagger } from "$lib/motion";
import Drawer from "$lib/ui/Drawer.svelte";
import { toast } from "$lib/ui/toast.svelte";
import { library } from "./library.svelte";

let preview = $state<ConfigImport | null>(null);
let selected = $state<Record<string, boolean>>({});
let error = $state<string | null>(null);
let busy = $state(false);
let summary = $state<ImportSummary | null>(null);

function fail(message: string) {
  error = message;
  toast.error(message);
}

const existing = $derived(
  new Set(library.hosts.map((h) => `${h.username}@${h.hostname}:${h.port}`)),
);
const count = $derived(Object.values(selected).filter(Boolean).length);

$effect(() => {
  if (inTauri) void load();
});

async function load() {
  const res = wrap(await commands.importSshConfigPreview());
  if (!res.ok) {
    fail(res.error.message);
    return;
  }
  preview = res.data;
  selected = Object.fromEntries(res.data.hosts.map((h) => [h.alias, true]));
}

async function apply() {
  busy = true;
  error = null;
  const aliases = Object.entries(selected)
    .filter(([, on]) => on)
    .map(([a]) => a);
  const res = wrap(await commands.importSshConfigApply(aliases));
  busy = false;
  if (!res.ok) {
    fail(res.error.message);
    return;
  }
  summary = res.data;
  toast.success(describe(res.data));
  await library.refresh();
}

async function importKnown() {
  busy = true;
  error = null;
  const res = wrap(await commands.importKnownHosts());
  busy = false;
  if (!res.ok) {
    fail(res.error.message);
    return;
  }
  summary = res.data;
  toast.success(describe(res.data));
}

// Other apps: PuTTY (registry or .reg), MobaXterm, SecureCRT, CSV (Termius, spreadsheets).
const SOURCES: { id: ForeignSource; label: string; hint: string }[] = [
  {
    id: "putty",
    label: "PuTTY / KiTTY",
    hint: "Reads this computer's saved sessions, or a .reg export.",
  },
  { id: "mobaxterm", label: "MobaXterm", hint: "A .mxtsessions export or MobaXterm.ini." },
  { id: "securecrt", label: "SecureCRT", hint: "Its Sessions folder (or one session .ini)." },
  {
    id: "csv",
    label: "CSV (Termius, spreadsheets)",
    hint: "Columns: label, hostname, port, username, group, protocol.",
  },
];
let source = $state<ForeignSource>("putty");
let foreignPath = $state<string | null>(null);
let foreignPreview = $state<ForeignImport | null>(null);
let foreignSelected = $state<Record<string, boolean>>({});
const foreignCount = $derived(Object.values(foreignSelected).filter(Boolean).length);

async function previewForeign(path: string | null) {
  error = null;
  summary = null;
  foreignPath = path;
  const res = wrap(await commands.importForeignPreview(source, path));
  if (!res.ok) {
    foreignPreview = null;
    fail(res.error.message);
    return;
  }
  foreignPreview = res.data;
  foreignSelected = Object.fromEntries(res.data.hosts.map((h) => [h.label, true]));
}

async function chooseForeign() {
  const folder = source === "securecrt";
  const picked = await pickFile({
    directory: folder,
    multiple: false,
    title: folder ? "SecureCRT Sessions folder" : "Choose the export file",
  });
  if (typeof picked === "string") await previewForeign(picked);
}

async function applyForeign() {
  busy = true;
  error = null;
  const labels = Object.entries(foreignSelected)
    .filter(([, on]) => on)
    .map(([l]) => l);
  const res = wrap(await commands.importForeignApply(source, foreignPath, labels));
  busy = false;
  if (!res.ok) {
    fail(res.error.message);
    return;
  }
  summary = res.data;
  toast.success(describe(res.data));
  foreignPreview = null;
  await library.refresh();
}

function where(h: ForeignHost): string {
  if (h.protocol === "serial") return `serial · ${h.hostname} · ${h.serial?.baud ?? 9600} baud`;
  const user = h.username ? `${h.username}@` : "";
  const proto = h.protocol === "telnet" ? "telnet · " : "";
  return `${proto}${user}${h.hostname}:${h.port}${h.group ? ` · ${h.group}` : ""}`;
}

function describe(s: ImportSummary): string {
  const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;
  const parts = [
    s.hosts ? plural(s.hosts, "host") : "",
    s.keys ? plural(s.keys, "key") : "",
    s.knownHosts ? plural(s.knownHosts, "trusted key") : "",
  ].filter(Boolean);
  return `${parts.join(", ") || "Nothing new"} imported.`;
}
</script>

<Drawer title={t("import.title")} onclose={() => shell.close()}>
  <section class="pt-2">
    <h3 class="label-caps mb-1 text-faint">{t("import.fromConfig")}</h3>
    {#if preview}
      {#if preview.hosts.length}
        <div class="flex flex-col gap-1">
          {#each preview.hosts as h, i (h.alias)}
            <label class="item" in:blurFade={{ delay: stagger(i, 18, 12), y: 4, blur: 0, duration: 180 }}>
              <input type="checkbox" bind:checked={selected[h.alias]} />
              <span class="min-w-0 flex-1">
                <span class="block truncate text-sm text-fg">{h.alias}</span>
                <span class="tabular block truncate text-xs text-faint">
                  {h.user ? `${h.user}@` : ""}{h.hostname}{h.port === 22 ? "" : `:${h.port}`}{h.identityFiles.length
                    ? " · key"
                    : ""}{h.proxyJump ? ` · via ${h.proxyJump}` : ""}
                </span>
              </span>
              {#if existing.has(`${h.user ?? ""}@${h.hostname}:${h.port}`)}
                <span class="text-xs text-faint">saved</span>
              {/if}
            </label>
          {/each}
        </div>
        <button type="button" class="btn-primary mt-3 w-full" disabled={busy || count === 0} onclick={apply}>
          Import {count} host{count === 1 ? "" : "s"}
        </button>
        <p class="mt-2 text-xs text-muted">
          Readable, unencrypted identity files are stored as keys. Passphrase-protected ones can be added from
          the Keys screen.
        </p>
      {:else}
        <p class="text-sm text-muted">No concrete hosts found (wildcard entries are applied as defaults).</p>
      {/if}
    {:else if !error}
      <p class="text-sm text-muted">Reading your config…</p>
    {/if}
  </section>

  <section class="mt-5" aria-label="From another app">
    <h3 class="label-caps mb-1 text-faint">{t("import.fromApp")}</h3>
    <select
      class="input w-full"
      bind:value={source}
      aria-label="Import source"
      onchange={() => ((foreignPreview = null), (foreignPath = null))}
    >
      {#each SOURCES as s (s.id)}<option value={s.id}>{s.label}</option>{/each}
    </select>
    <p class="mt-1 text-xs text-muted">{SOURCES.find((s) => s.id === source)?.hint}</p>
    <div class="mt-2 flex gap-2">
      {#if source === "putty"}
        <button type="button" class="btn-ghost flex-1" disabled={busy} onclick={() => previewForeign(null)}>
          {t("import.readComputer")}
        </button>
      {/if}
      <button type="button" class="btn-ghost flex-1" disabled={busy} onclick={chooseForeign}>
        {source === "securecrt" ? t("import.chooseFolder") : t("import.chooseFile")}
      </button>
    </div>
    {#if foreignPreview}
      {#if foreignPreview.hosts.length}
        <div class="mt-2 flex flex-col gap-1" aria-label="Sessions found">
          {#each foreignPreview.hosts as h, i (`${h.label}-${i}`)}
            <label class="item" in:blurFade={{ delay: stagger(i, 18, 12), y: 4, blur: 0, duration: 180 }}>
              <input type="checkbox" bind:checked={foreignSelected[h.label]} />
              <span class="min-w-0 flex-1">
                <span class="block truncate text-sm text-fg">{h.label}</span>
                <span class="tabular block truncate text-xs text-faint">{where(h)}</span>
              </span>
            </label>
          {/each}
        </div>
        <button type="button" class="btn-primary mt-3 w-full" disabled={busy || foreignCount === 0} onclick={applyForeign}>
          {foreignCount === 1 ? t("import.sessionsOne") : t("import.sessions", { count: foreignCount })}
        </button>
        <p class="mt-2 text-xs text-muted">{t("import.noPasswords")}</p>
      {:else}
        <p class="mt-2 text-sm text-muted">{t("import.none")}</p>
      {/if}
      {#if foreignPreview.warnings.length}
        <ul class="mt-2 list-disc pl-4 text-xs text-muted">
          {#each foreignPreview.warnings as w, i (i)}<li>{w}</li>{/each}
        </ul>
      {/if}
    {/if}
  </section>

  <section class="mt-5">
    <h3 class="label-caps mb-1 text-faint">{t("import.fromKnownHosts")}</h3>
    <p class="mb-2 text-xs text-muted">{t("import.knownHint")}</p>
    <button type="button" class="btn-ghost w-full" disabled={busy} onclick={importKnown}>{t("import.knownButton")}</button>
  </section>


  {#if summary}
    <section class="panel mt-4" role="status">
      <p class="text-sm text-fg">{describe(summary)}</p>
      {#if summary.warnings.length}
        <ul class="list-disc pl-4 text-xs text-muted">
          {#each summary.warnings as w, i (i)}<li>{w}</li>{/each}
        </ul>
      {/if}
    </section>
  {/if}
</Drawer>

<style>
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border-radius: var(--radius-sm);
  }
  .item:hover {
    background: var(--surface-2);
  }
  .item input {
    accent-color: var(--accent);
  }
</style>
