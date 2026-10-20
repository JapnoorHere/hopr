<script lang="ts">
import { ArrowLeft, ArrowRight, ArrowUp, File, Folder, X } from "phosphor-svelte";
import { library } from "$features/hosts/library.svelte";
import { commands, inTauri, type LocalEntry, type RemoteEntry } from "$lib/ipc";
import ClashBar from "./ClashBar.svelte";
import { files } from "./files.svelte";
import { crumbs, formatSize } from "./format";

const DRAG = "application/x-hopr-files";

let localCwd = $state("");
let localEntries = $state<LocalEntry[]>([]);
let localSel = $state<string[]>([]);
let localErr = $state<string | null>(null);
let over = $state<"local" | "remote" | null>(null);
/** The side files are being dragged from: its selection dims while they travel. */
let sending = $state<"local" | "remote" | null>(null);

async function goLocal(path: string) {
  const r = await commands.localList(path);
  if (r.status === "error") {
    localErr = r.error.message;
    return;
  }
  localErr = null;
  localCwd = path;
  localEntries = r.data;
  localSel = [];
}

function localUp() {
  const trimmed = localCwd.replace(/[\\/]+$/, "");
  const i = Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/"));
  if (i <= 0) return;
  // Keep the root's separator: "C:\Users" goes up to "C:\".
  const up = trimmed.slice(0, i);
  void goLocal(up.endsWith(":") ? `${up}\\` : up);
}

$effect(() => {
  if (!inTauri) return;
  // Start at home, unless a folder was picked while that answer was on its way.
  void commands.localHome().then((home) => {
    if (!localCwd) void goLocal(home);
  });
  if (!files.hostId && library.hosts[0]) void files.open(library.hosts[0].id);
});

function pick(list: string[], path: string, additive: boolean): string[] {
  if (!additive) return [path];
  return list.includes(path) ? list.filter((p) => p !== path) : [...list, path];
}

function dragStart(e: DragEvent, from: "local" | "remote", path: string) {
  const sel = from === "local" ? localSel : files.selected;
  const paths = sel.includes(path) ? sel : [path];
  e.dataTransfer?.setData(DRAG, JSON.stringify({ from, paths }));
  sending = from;
  (e.currentTarget as HTMLElement | null)?.addEventListener("dragend", () => (sending = null), {
    once: true,
  });
  if (e.dataTransfer) e.dataTransfer.effectAllowed = "copy";
}

function dragOver(e: DragEvent, side: "local" | "remote") {
  if (!e.dataTransfer?.types.includes(DRAG)) return;
  e.preventDefault();
  over = side;
}

async function drop(e: DragEvent, side: "local" | "remote") {
  over = null;
  sending = null;
  const raw = e.dataTransfer?.getData(DRAG);
  if (!raw) return;
  e.preventDefault();
  const { from, paths } = JSON.parse(raw) as { from: string; paths: string[] };
  if (from === side) return;
  if (side === "remote") await files.upload(paths);
  else {
    await files.download(paths, localCwd);
    setTimeout(() => void goLocal(localCwd), 800);
  }
}

async function downloadSelected() {
  await files.download(files.selected, localCwd);
  setTimeout(() => void goLocal(localCwd), 800);
}

function openRemote(e: RemoteEntry) {
  if (e.kind === "dir" || e.kind === "link") void files.go(e.path);
}

function openLocal(e: LocalEntry) {
  if (e.dir) void goLocal(e.path);
}

function pickHost(e: Event) {
  void files.open((e.currentTarget as HTMLSelectElement).value);
}
</script>

<section class="dual" aria-label="Dual pane">
  <header class="flex items-center gap-2 border-b border-border px-4 py-2">
    <h2 class="text-sm font-medium">Files</h2>
    <select class="input host" value={files.hostId ?? ""} onchange={pickHost} aria-label="Remote host">
      {#each library.hosts as h (h.id)}
        <option value={h.id}>{h.label}</option>
      {/each}
    </select>
    <span class="flex-1"></span>
    <button type="button" class="btn-ghost" disabled={localSel.length === 0} onclick={() => files.upload(localSel)}>
      Upload <ArrowRight size={13} />
    </button>
    <button type="button" class="btn-ghost" disabled={files.selected.length === 0} onclick={downloadSelected}>
      <ArrowLeft size={13} /> Download
    </button>
    <button type="button" class="tool" aria-label="Close dual pane" onclick={() => (files.dual = false)}>
      <X size={15} />
    </button>
  </header>
  <div class="px-4 pt-2"><ClashBar /></div>
  <div class="panes">
    <div
      class="pane"
      class:over={over === "local"}
      class:beam-border={over === "local"}
      class:sending={sending === "local"}
      aria-label="Local files"
      role="region"
      ondragover={(e) => dragOver(e, "local")}
      ondragleave={() => (over = null)}
      ondrop={(e) => drop(e, "local")}
    >
      <div class="bar">
        <button type="button" class="tool" aria-label="Local up" onclick={localUp}><ArrowUp size={14} /></button>
        <form
          class="min-w-0 flex-1"
          onsubmit={(e) => {
            e.preventDefault();
            void goLocal(localCwd);
          }}
        >
          <input class="input font-mono text-xs" bind:value={localCwd} aria-label="Local path" spellcheck="false" />
        </form>
      </div>
      {#if localErr}<p class="p-2 text-xs text-danger">{localErr}</p>{/if}
      <div class="list">
        {#each localEntries as e (e.path)}
          <div
            class="row"
            class:selected={localSel.includes(e.path)}
            role="option"
            aria-selected={localSel.includes(e.path)}
            tabindex="-1"
            data-local={e.name}
            draggable="true"
            ondragstart={(ev) => dragStart(ev, "local", e.path)}
            onclick={(ev) => (localSel = pick(localSel, e.path, ev.ctrlKey || ev.metaKey))}
            ondblclick={() => openLocal(e)}
            onkeydown={() => {}}
          >
            <span class="text-faint">{#if e.dir}<Folder size={14} weight="fill" />{:else}<File size={14} />{/if}</span>
            <span class="flex-1 truncate text-sm">{e.name}</span>
            <span class="tabular text-xs text-muted">{e.dir ? "" : formatSize(e.size)}</span>
          </div>
        {/each}
      </div>
    </div>

    <div
      class="pane"
      class:over={over === "remote"}
      class:beam-border={over === "remote"}
      class:sending={sending === "remote"}
      aria-label="Remote files"
      role="region"
      ondragover={(e) => dragOver(e, "remote")}
      ondragleave={() => (over = null)}
      ondrop={(e) => drop(e, "remote")}
    >
      <div class="bar">
        <button type="button" class="tool" aria-label="Remote up" onclick={() => files.up()}>
          <ArrowUp size={14} />
        </button>
        <nav class="crumbs min-w-0 flex-1" aria-label="Remote path">
          {#each crumbs(files.cwd) as c, i (c.path)}
            {#if i > 1}<span class="text-faint">/</span>{/if}
            <button type="button" class="crumb" onclick={() => files.go(c.path)}>{c.name}</button>
          {/each}
        </nav>
      </div>
      {#if files.error}<p class="p-2 text-xs text-danger">{files.error}</p>{/if}
      <div class="list">
        {#each files.shown as e (e.path)}
          <div
            class="row"
            class:selected={files.selected.includes(e.path)}
            role="option"
            aria-selected={files.selected.includes(e.path)}
            tabindex="-1"
            data-remote={e.name}
            draggable="true"
            ondragstart={(ev) => dragStart(ev, "remote", e.path)}
            onclick={(ev) => files.toggle(e.path, ev.ctrlKey || ev.metaKey)}
            ondblclick={() => openRemote(e)}
            onkeydown={() => {}}
          >
            <span class="text-faint">{#if e.kind === "dir"}<Folder size={14} weight="fill" />{:else}<File size={14} />{/if}</span>
            <span class="flex-1 truncate text-sm">{e.name}</span>
            <span class="tabular text-xs text-muted">{e.kind === "dir" ? "" : formatSize(e.size)}</span>
          </div>
        {/each}
      </div>
    </div>
  </div>
  <p class="px-4 pb-2 text-xs text-faint">
    Drag between the panes, or select and use Upload / Download.
  </p>
</section>

<style>
  .dual {
    position: absolute;
    inset: 0;
    z-index: 15;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .host {
    width: auto;
    min-width: 180px;
    padding-top: 3px;
    padding-bottom: 3px;
  }
  .panes {
    display: grid;
    min-height: 0;
    flex: 1;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    padding: 8px 16px;
  }
  .pane {
    display: flex;
    min-height: 0;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  /* Files on their way to the other side. */
  .pane.sending :global(.selected) {
    opacity: 0.5;
    transition: opacity var(--dur) var(--ease-out);
  }
  .pane.over {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px;
    border-bottom: 1px solid var(--border);
  }
  .list {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    padding: 4px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 8px;
    border-radius: var(--radius-sm);
    cursor: default;
  }
  .row:hover {
    background: var(--surface-3);
  }
  .row.selected {
    background: color-mix(in oklab, var(--accent) 18%, transparent);
  }
  .tool {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    color: var(--text-muted);
    border-radius: 4px;
  }
  .tool:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 2px;
    overflow: hidden;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .crumb {
    padding: 2px 4px;
    color: var(--text-muted);
    border-radius: 4px;
  }
  .crumb:hover {
    color: var(--text);
    background: var(--surface-3);
  }
</style>
