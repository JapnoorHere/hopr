<script lang="ts">
import {
  ArrowRight,
  DotsThree,
  DownloadSimple,
  Export,
  MagnifyingGlass,
  PencilSimple,
  Plus,
  Star,
  TerminalWindow,
} from "phosphor-svelte";
import { flip } from "svelte/animate";
import { SvelteSet } from "svelte/reactivity";
import { files } from "$features/sftp/files.svelte";
import { shell } from "$features/shell/shell.state.svelte";
import { sessions } from "$features/terminal/session.svelte";
import { parseTarget } from "$features/terminal/target";
import { workspaces } from "$features/workspace/workspaces.svelte";
import { saveText } from "$lib/download";
import HoprMark from "$lib/icons/HoprMark.svelte";
import {
  commands,
  type Environment,
  type ExportFormat,
  type Host,
  inTauri,
  type ShellProfile,
} from "$lib/ipc";
import { blurFade, expand, flash, pop, rise, sink, stagger } from "$lib/motion";
import { motion } from "$lib/platform";
import Count from "$lib/ui/Count.svelte";
import FieldError from "$lib/ui/FieldError.svelte";
import Kbd from "$lib/ui/Kbd.svelte";
import { sshCommand } from "./chain";
import { sectionHosts } from "./filter";
import { library, PING_INTERVAL_MS } from "./library.svelte";

let query = $state("");
let quick = $state(quickFromDraft());
let quickError = $state<string | null>(null);
let shells = $state<ShellProfile[]>([]);
let shellMenu = $state(false);
let exportMenu = $state(false);
let rowMenu = $state<string | null>(null);
let list: HTMLDivElement | undefined = $state();
const selected = new SvelteSet<string>();
let bulkTag = $state("");

const sections = $derived(sectionHosts(library.hosts, library.groups, query));
const visibleIds = $derived([...new Set(sections.flatMap((s) => s.hosts.map((h) => h.id)))]);
const empty = $derived(library.loaded && library.hosts.length === 0);

function quickFromDraft(): string {
  const d = sessions.draft;
  if (!d?.host) return "";
  const user = d.username ? `${d.username}@` : "";
  const port = d.port && d.port !== 22 ? `:${d.port}` : "";
  return `${user}${d.host}${port}`;
}

$effect(() => {
  void library.refresh();
  if (inTauri) {
    void commands.shellsList().then((r) => {
      if (r.status === "ok") shells = r.data;
    });
  }
});

// Reachability of the visible rows: now, then every minute while this view is up (H9).
$effect(() => {
  const ids = visibleIds;
  void library.ping(ids);
  const timer = setInterval(() => void library.ping(ids), PING_INTERVAL_MS);
  return () => clearInterval(timer);
});

// Selected hosts that were deleted drop out of the selection.
$effect(() => {
  for (const id of selected) if (!library.host(id)) selected.delete(id);
});

function connect(h: Host) {
  // The row glows as it turns into a connection.
  flash(list?.querySelector(`[data-host-id="${h.id}"]`));
  library.connect(h);
}

function quickConnect(e: SubmitEvent) {
  e.preventDefault();
  quickError = null;
  if (!quick.trim()) {
    quickError = "Type a host to connect to, like deploy@10.0.3.4";
    return;
  }
  const t = parseTarget(quick);
  if (!t) {
    quickError = "Use host, user@host or user@host:port";
    return;
  }
  if (!t.username) {
    quickError = `Add a user, like deploy@${t.host}`;
    return;
  }
  sessions.open({ kind: "quick", host: t.host, port: t.port ?? 22, username: t.username });
  quick = "";
}

function openShell(s: ShellProfile) {
  shellMenu = false;
  sessions.open({ kind: "local", shellId: s.id, label: s.name });
}

/** Arrow keys move between host rows; Enter connects (rows are buttons). */
function onListKey(e: KeyboardEvent) {
  if (e.key === "Escape" && (rowMenu || selected.size)) {
    rowMenu = null;
    selected.clear();
    e.stopPropagation();
    return;
  }
  if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
  const rows = [...(list?.querySelectorAll<HTMLElement>("[data-host-row]") ?? [])];
  const i = rows.indexOf(document.activeElement as HTMLElement);
  const next = rows[e.key === "ArrowDown" ? i + 1 : Math.max(0, i - 1)];
  if (next) {
    e.preventDefault();
    next.focus();
  }
}

function onSearchKey(e: KeyboardEvent) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    list?.querySelector<HTMLElement>("[data-host-row]")?.focus();
  } else if (e.key === "Enter") {
    const first = sections[0]?.hosts[0];
    if (first) connect(first);
  }
}

function toggleSelect(id: string) {
  if (selected.has(id)) selected.delete(id);
  else selected.add(id);
}

function onRowClick(e: MouseEvent, h: Host) {
  // Ctrl/Cmd+click (or any click while selecting) selects instead of connecting.
  if (e.ctrlKey || e.metaKey || selected.size) {
    toggleSelect(h.id);
    return;
  }
  connect(h);
}

async function copy(text: string, what: string) {
  await navigator.clipboard.writeText(text);
  library.flash(`${what} copied`);
}

async function runRow(action: string, h: Host) {
  rowMenu = null;
  switch (action) {
    case "connect":
      return connect(h);
    case "edit":
      library.editing = h.id;
      return;
    case "duplicate": {
      const r = await library.duplicate(h);
      if (r.ok) library.flash(`Created “${r.data.label}”`);
      else library.flash(r.error.message, "error");
      return;
    }
    case "ssh":
      return copy(sshCommand(h, library.hosts), "SSH command");
    case "hostname":
      return copy(h.hostname, "Hostname");
    case "password":
      await library.copyPassword(h);
      return;
    case "files":
      shell.drawer = "files";
      await files.open(h.id);
      return;
    case "deploy":
      library.deploying = { hostId: h.id, keyId: null };
      return;
    case "ping":
      await library.ping([h.id]);
      return;
    case "delete":
      await library.remove([h.id]);
      return;
  }
}

async function bulk(edit: Partial<Parameters<typeof library.bulkEdit>[0]>) {
  const res = await library.bulkEdit({
    ids: [...selected],
    moveGroup: false,
    groupId: null,
    addTags: [],
    removeTags: [],
    environment: null,
    favorite: null,
    ...edit,
  });
  if (!res.ok) library.flash(res.error.message, "error");
  else library.flash(`Updated ${res.data} host${res.data === 1 ? "" : "s"}`);
}

function bulkGroup(e: Event) {
  const el = e.currentTarget as HTMLSelectElement;
  const v = el.value;
  el.value = "";
  if (v === "") return;
  void bulk({ moveGroup: true, groupId: v === "__none" ? null : v });
}

function bulkEnv(e: Event) {
  const el = e.currentTarget as HTMLSelectElement;
  const v = el.value as Environment | "";
  el.value = "";
  if (v) void bulk({ environment: v });
}

function addTag(e: SubmitEvent) {
  e.preventDefault();
  const tags = bulkTag
    .split(",")
    .map((t) => t.trim())
    .filter(Boolean);
  if (!tags.length) return;
  bulkTag = "";
  void bulk({ addTags: tags });
}

async function bulkDelete() {
  const ids = [...selected];
  selected.clear();
  await library.remove(ids);
}

async function exportHosts(format: ExportFormat, ids: string[]) {
  exportMenu = false;
  const res = await library.exportHosts(format, ids);
  if (!res.ok) {
    library.flash(res.error.message, "error");
    return;
  }
  if (format === "ssh_config") saveText("hopr_ssh_config", res.data);
  else saveText("hopr-hosts.csv", res.data, "text/csv");
  library.flash(`Exported ${ids.length || library.hosts.length} hosts`);
}

function reachTitle(h: Host): string {
  const r = library.reach[h.id];
  if (!r) return "Not checked yet";
  if (r.status === "reachable") return `Reachable · ${r.ms} ms`;
  if (r.status === "unreachable") return "Not reachable";
  return h.jumpHostId ? "Reached through a jump host" : "Reached through a proxy";
}

function onWindowPointer(e: PointerEvent) {
  const t = e.target as Element | null;
  if (!t?.closest("[data-menu]")) {
    rowMenu = null;
    exportMenu = false;
    shellMenu = false;
  }
}

const ENV_LABEL = { none: "", dev: "dev", staging: "staging", prod: "prod" } as const;
</script>

<svelte:window onpointerdown={onWindowPointer} />

<div class="relative flex min-h-0 w-full flex-1 flex-col">
  <header class="flex flex-wrap items-center gap-2 border-b border-border px-6 py-3">
    <div class="relative min-w-48 flex-1">
      <span class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-faint">
        <MagnifyingGlass size={14} />
      </span>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="input search"
        placeholder="Search hosts, tags, users (tag:db, env:prod)"
        bind:value={query}
        onkeydown={onSearchKey}
        spellcheck="false"
        aria-label="Search hosts"
        autofocus={!empty}
      />
    </div>
    <button type="button" class="btn-ghost" onclick={() => (library.editing = "new")}>
      <Plus size={14} /> New host
    </button>
    <button type="button" class="btn-ghost" onclick={() => shell.toggle("import")}>
      <DownloadSimple size={14} /> Import
    </button>
    <div class="relative" data-menu>
      <button
        type="button"
        class="btn-ghost"
        aria-haspopup="menu"
        aria-expanded={exportMenu}
        disabled={empty}
        onclick={() => (exportMenu = !exportMenu)}
      >
        <Export size={14} /> Export
      </button>
      {#if exportMenu}
        <div class="menu" role="menu">
          <button type="button" role="menuitem" class="menu-item" onclick={() => exportHosts("ssh_config", [])}>
            All hosts as SSH config
          </button>
          <button type="button" role="menuitem" class="menu-item" onclick={() => exportHosts("csv", [])}>
            All hosts as CSV
          </button>
          <p class="px-2.5 py-1 text-xs text-faint">Passwords and keys are never exported.</p>
        </div>
      {/if}
    </div>
    <div class="relative" data-menu>
      <button
        type="button"
        class="btn-ghost"
        aria-haspopup="menu"
        aria-expanded={shellMenu}
        onclick={() => (shellMenu = !shellMenu)}
        disabled={shells.length === 0}
      >
        <TerminalWindow size={14} /> Local terminal
      </button>
      {#if shellMenu}
        <div class="menu" role="menu">
          {#each shells as s (s.id)}
            <button type="button" role="menuitem" class="menu-item" onclick={() => openShell(s)}>{s.name}</button>
          {/each}
        </div>
      {/if}
    </div>
  </header>

  <form class="flex items-center gap-2 px-6 pt-4" onsubmit={quickConnect} novalidate>
    <input
      class="input font-mono"
      bind:value={quick}
      placeholder="Quick connect: user@host:port"
      spellcheck="false"
      aria-label="Quick connect"
      aria-invalid={!!quickError}
      oninput={() => (quickError = null)}
      onblur={() => {
        if (!quick.trim()) quickError = null;
      }}
    />
    <button type="submit" class="btn-primary shrink-0">
      Connect <ArrowRight size={14} />
    </button>
  </form>
  {#if quickError}<div class="px-6 pt-1.5"><FieldError message={quickError} /></div>{/if}
  {#if workspaces.offer}
    <div class="offer" role="status" aria-label="Restore last session">
      <span class="flex-1 text-sm text-fg">
        Pick up where you left off: {workspaces.offer.tabs.length} tab{workspaces.offer.tabs.length === 1
          ? ""
          : "s"} from your last session.
      </span>
      <button type="button" class="btn-ghost" onclick={() => workspaces.dismissOffer()}>Dismiss</button>
      <button type="button" class="btn-primary" onclick={() => workspaces.restoreLast()}>
        Restore <Kbd keys="Ctrl+Shift+R" />
      </button>
    </div>
  {/if}

  {#if !library.loaded}
    <!-- Placeholder rows while the library loads, so nothing jumps. -->
    <div class="flex flex-col gap-2 px-6 py-4" aria-hidden="true">
      {#each [0, 1, 2, 3, 4] as n (n)}
        <div class="flex items-center gap-3 px-2 py-2" style:opacity={1 - n * 0.16}>
          <span class="skeleton h-2 w-2 rounded-full"></span>
          <span class="flex flex-1 flex-col gap-1.5">
            <span class="skeleton h-3" style:width="{38 - n * 4}%"></span>
            <span class="skeleton h-2.5" style:width="{26 + n * 3}%"></span>
          </span>
          <span class="skeleton h-4 w-10"></span>
        </div>
      {/each}
    </div>
  {:else if empty}
    <div class="m-auto flex max-w-sm flex-col items-center gap-4 px-6 text-center" in:blurFade={{ y: 10 }}>
      <span class="text-faint" in:pop={{ delay: 80 }}><HoprMark size={40} /></span>
      <p class="text-lg text-fg">No saved hosts yet</p>
      <p class="text-sm text-muted">
        Add a host, import the ones in your <span class="font-mono">~/.ssh/config</span>, or quick
        connect above.
      </p>
      <div class="flex gap-2">
        <button type="button" class="btn-primary" onclick={() => (library.editing = "new")}>Add host</button>
        <button type="button" class="btn-ghost" onclick={() => shell.toggle("import")}>Import</button>
      </div>
    </div>
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="min-h-0 flex-1 overflow-y-auto px-4 py-3"
      class:selecting={selected.size > 0}
      bind:this={list}
      onkeydown={onListKey}
    >
      {#each sections as section (section.id)}
        <section class="mb-4">
          <h3 class="label-caps mb-1 px-2 text-faint">{section.title}</h3>
          {#each section.hosts as h, i (h.id)}
            <div
              class="row"
              class:prod={h.environment === "prod"}
              class:picked={selected.has(h.id)}
              data-host-id={h.id}
              in:blurFade={{ delay: stagger(i), y: 6, blur: 2, duration: 220 }}
              out:expand={{ duration: 180 }}
              animate:flip={{ duration: motion(220) }}
            >
              <input
                type="checkbox"
                class="pick"
                checked={selected.has(h.id)}
                onchange={() => toggleSelect(h.id)}
                aria-label="Select {h.label}"
              />
              <button
                type="button"
                class="flex min-w-0 flex-1 items-center gap-3 py-2 pl-2 text-left"
                data-host-row
                onclick={(e) => onRowClick(e, h)}
                title={selected.size ? `Select ${h.label}` : `Connect to ${h.label}`}
              >
                <span
                  class="dot"
                  class:ring-once={library.reach[h.id]?.status === "reachable"}
                  data-reach={library.reach[h.id]?.status ?? "unknown"}
                  title={reachTitle(h)}
                  aria-label={reachTitle(h)}
                  role="img"
                ></span>
                <span class="min-w-0 flex-1">
                  <span class="block truncate text-sm text-fg">{h.label}</span>
                  <span class="tabular block truncate text-xs text-faint">
                    {#if h.protocol === "serial"}serial · {h.hostname} · {h.serial?.baud ?? 9600} baud{:else if h.protocol === "telnet"}telnet · {h.username ? `${h.username}@` : ""}{h.hostname}:{h.port}{:else}{h.username}@{h.hostname}{h.port === 22 ? "" : `:${h.port}`}{/if}{h.proxyCommand ? " · via ProxyCommand" : ""}{h.jumpHostId ? ` · via ${library.host(h.jumpHostId)?.label ?? "jump host"}` : ""}{h.proxy && !h.jumpHostId ? ` · via ${h.proxy.protocol} proxy` : ""}
                  </span>
                </span>
                {#if library.reach[h.id]?.status === "reachable"}
                  <span class="shrink-0 text-xs text-faint" in:blurFade={{ y: 0, blur: 2, duration: 200 }}>
                    <Count value={library.reach[h.id]?.ms ?? 0} format={(v) => `${Math.round(v)} ms`} />
                  </span>
                {/if}
                {#if h.environment !== "none"}
                  <span class="env" data-env={h.environment}>{ENV_LABEL[h.environment]}</span>
                {/if}
                {#each h.tags.slice(0, 3) as tag (tag)}
                  <span class="tag">{tag}</span>
                {/each}
              </button>
              <button
                type="button"
                class="icon"
                class:on={h.favorite}
                aria-label={h.favorite ? `Unpin ${h.label}` : `Pin ${h.label}`}
                aria-pressed={h.favorite}
                onclick={() => library.toggleFavorite(h)}
              >
                {#key h.favorite}
                  <span class="grid" in:pop={{ duration: h.favorite ? 460 : 200 }}>
                    <Star size={14} weight={h.favorite ? "fill" : "regular"} />
                  </span>
                {/key}
              </button>
              <button type="button" class="icon" aria-label="Edit {h.label}" onclick={() => (library.editing = h.id)}>
                <PencilSimple size={14} />
              </button>
              <div class="relative" data-menu>
                <button
                  type="button"
                  class="icon"
                  class:on={rowMenu === h.id}
                  aria-label="More actions for {h.label}"
                  aria-haspopup="menu"
                  aria-expanded={rowMenu === h.id}
                  onclick={() => (rowMenu = rowMenu === h.id ? null : h.id)}
                >
                  <DotsThree size={16} weight="bold" />
                </button>
                {#if rowMenu === h.id}
                  <div
                    class="menu"
                    role="menu"
                    aria-label="Actions for {h.label}"
                    in:blurFade={{ y: -4, scale: 0.97, blur: 2, duration: 160 }}
                    out:sink={{ y: -4, scale: 0.97, duration: 100 }}
                  >
                    <button type="button" role="menuitem" class="menu-item" onclick={() => runRow("connect", h)}>Connect</button>
                    <button type="button" role="menuitem" class="menu-item" onclick={() => runRow("edit", h)}>Edit</button>
                    <button type="button" role="menuitem" class="menu-item" onclick={() => runRow("duplicate", h)}>Duplicate</button>
                    <button type="button" role="menuitem" class="menu-item" onclick={() => runRow("files", h)}>Browse files</button>
                    <hr class="sep" />
                    <button type="button" role="menuitem" class="menu-item" onclick={() => runRow("ssh", h)}>Copy SSH command</button>
                    <button type="button" role="menuitem" class="menu-item" onclick={() => runRow("hostname", h)}>Copy hostname</button>
                    {#if h.hasPassword}
                      <button type="button" role="menuitem" class="menu-item" onclick={() => runRow("password", h)}>Copy password</button>
                    {/if}
                    <hr class="sep" />
                    <button type="button" role="menuitem" class="menu-item" onclick={() => runRow("deploy", h)}>Deploy key…</button>
                    <button type="button" role="menuitem" class="menu-item" onclick={() => runRow("ping", h)}>Check reachability</button>
                    <hr class="sep" />
                    <button type="button" role="menuitem" class="menu-item danger" onclick={() => runRow("delete", h)}>Delete</button>
                  </div>
                {/if}
              </div>
            </div>
          {/each}
        </section>
      {:else}
        <p class="px-2 py-6 text-center text-sm text-muted">No hosts match “{query}”.</p>
      {/each}
      <p class="px-2 text-xs text-faint">
        <Kbd keys="Ctrl+Shift+P" /> palette · <Kbd keys="Ctrl+Shift+L" /> lock · ↑↓ then Enter to connect · Ctrl+click to select
      </p>
    </div>
  {/if}

  {#if selected.size}
    <div class="bulk" role="toolbar" aria-label="Selected hosts" in:rise={{ y: 12 }} out:sink={{ y: 12 }}>
      <span class="text-sm text-fg">{selected.size} selected</span>
      <select class="input bulk-input" onchange={bulkGroup} aria-label="Move to group">
        <option value="">Move to…</option>
        <option value="__none">No group</option>
        {#each library.groups as g (g.id)}
          <option value={g.id}>{g.name}</option>
        {/each}
      </select>
      <form class="flex" onsubmit={addTag}>
        <input class="input bulk-input" bind:value={bulkTag} placeholder="Add tag" aria-label="Add tag" spellcheck="false" />
      </form>
      <select class="input bulk-input" onchange={bulkEnv} aria-label="Set environment">
        <option value="">Environment…</option>
        <option value="none">None</option>
        <option value="dev">Dev</option>
        <option value="staging">Staging</option>
        <option value="prod">Prod</option>
      </select>
      <button type="button" class="btn-ghost" onclick={() => bulk({ favorite: true })}>Pin</button>
      <button type="button" class="btn-ghost" onclick={() => exportHosts("ssh_config", [...selected])}>Export</button>
      <button type="button" class="btn-danger" onclick={bulkDelete}>Delete</button>
      <button type="button" class="btn-ghost" onclick={() => selected.clear()}>Clear</button>
    </div>
  {/if}

</div>

<style>
  .search {
    padding-left: 32px;
  }
  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 2px;
    padding-right: 6px;
    padding-left: 6px;
    border-radius: var(--radius-sm);
  }
  .row:hover,
  .row:focus-within {
    background: var(--surface-2);
  }
  .row.picked {
    background: color-mix(in oklab, var(--accent) 12%, transparent);
  }
  .row.prod {
    box-shadow: inset 2px 0 0 var(--danger);
  }
  .pick {
    flex-shrink: 0;
    opacity: 0;
    accent-color: var(--accent);
  }
  .row:hover .pick,
  .row:focus-within .pick,
  .selecting .pick {
    opacity: 1;
  }
  .dot {
    width: 7px;
    height: 7px;
    flex-shrink: 0;
    border-radius: 999px;
    background: transparent;
    box-shadow: inset 0 0 0 1px var(--text-faint);
    color: var(--success);
    transition:
      background var(--dur-slow) var(--ease-out),
      box-shadow var(--dur-slow) var(--ease-out);
  }
  .dot[data-reach="reachable"] {
    background: var(--success);
    box-shadow: none;
  }
  .dot[data-reach="unreachable"] {
    background: var(--danger);
    box-shadow: none;
  }
  .dot[data-reach="skipped"] {
    background: var(--text-faint);
    box-shadow: none;
  }
  .env,
  .tag {
    flex-shrink: 0;
    padding: 1px 6px;
    font-size: 11px;
    color: var(--text-muted);
    background: var(--surface-3);
    border-radius: 4px;
  }
  .env[data-env="prod"] {
    color: var(--danger);
    background: color-mix(in oklab, var(--danger) 14%, transparent);
  }
  .env[data-env="staging"] {
    color: var(--warning);
    background: color-mix(in oklab, var(--warning) 14%, transparent);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    color: var(--text-faint);
    border-radius: 4px;
    opacity: 0;
  }
  .row:hover .icon,
  .row:focus-within .icon,
  .icon.on {
    opacity: 1;
  }
  .icon.on {
    color: var(--accent);
  }
  .icon:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    z-index: 30;
    display: flex;
    min-width: 200px;
    flex-direction: column;
    padding: 4px;
    background: var(--surface-3);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .menu-item {
    padding: 6px 10px;
    font-size: 13px;
    text-align: left;
    color: var(--text);
    border-radius: var(--radius-sm);
  }
  .menu-item:hover,
  .menu-item:focus-visible {
    background: var(--surface-4);
  }
  .menu-item.danger {
    color: var(--danger);
  }
  .sep {
    margin: 4px 6px;
    border: 0;
    border-top: 1px solid var(--border);
  }
  .bulk {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    background: var(--surface-2);
    border-top: 1px solid var(--border);
  }
  .bulk-input {
    width: auto;
    min-width: 120px;
    padding-top: 4px;
    padding-bottom: 4px;
  }
  .offer {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 12px 24px 0;
    padding: 6px 6px 6px 14px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-left: 2px solid var(--accent);
    border-radius: var(--radius);
  }
</style>
