<script lang="ts">
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as pickFiles } from "@tauri-apps/plugin-dialog";
import {
  ArrowClockwise,
  ArrowUp,
  BookmarkSimple,
  DotsThree,
  DownloadSimple,
  Eye,
  EyeSlash,
  File,
  FilePlus,
  Folder,
  FolderPlus,
  LinkSimple,
  UploadSimple,
} from "phosphor-svelte";
import { untrack } from "svelte";
import { library } from "$features/hosts/library.svelte";
import { shell } from "$features/shell/shell.state.svelte";
import { inTauri, type RemoteEntry } from "$lib/ipc";
import { blurFade, expand, sink } from "$lib/motion";
import ConfirmBar from "$lib/ui/ConfirmBar.svelte";
import Drawer from "$lib/ui/Drawer.svelte";
import FieldError from "$lib/ui/FieldError.svelte";
import Menu from "$lib/ui/Menu.svelte";
import ClashBar from "./ClashBar.svelte";
import EditsBar from "./EditsBar.svelte";
import FilePreview from "./FilePreview.svelte";
import { files } from "./files.svelte";
import {
  crumbs,
  formatSize,
  formatTime,
  modeString,
  octal,
  type SortKey,
  shellQuote,
} from "./format";
import PermissionsCard from "./PermissionsCard.svelte";

type Naming = { kind: "folder" | "file" | "rename"; entry?: RemoteEntry; value: string };

let editingPath = $state(false);
let typedPath = $state("");
let menu = $state<string | null>(null);
let naming = $state<Naming | null>(null);
let confirmDelete = $state<string[] | null>(null);
let perms = $state<RemoteEntry | null>(null);
let dropping = $state(false);
// Going into a folder slides the listing in from the right; going up, from the left.
let slide = $state(12);
let lastCwd = "";
$effect(() => {
  const cwd = files.cwd;
  untrack(() => {
    const depth = (p: string) => p.split("/").filter(Boolean).length;
    slide = lastCwd && depth(cwd) < depth(lastCwd) ? -12 : 12;
    lastCwd = cwd;
  });
});

const hostId = $derived(files.hostId ?? "");
const host = $derived(hostId ? library.host(hostId) : undefined);

// Closing the last place that shows files lets the host's SFTP connection go.
$effect(() => () => {
  if (shell.drawer !== "files" && !files.dual) files.release();
});

// Open the first host if none is chosen yet.
$effect(() => {
  if (!files.hostId && library.hosts[0]) void files.open(library.hosts[0].id);
});

// Files dragged in from the OS upload into the current folder.
$effect(() => {
  if (!inTauri) return;
  let unlisten: (() => void) | undefined;
  void getCurrentWebview()
    .onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "enter" || p.type === "over") dropping = true;
      else if (p.type === "leave") dropping = false;
      else if (p.type === "drop") {
        dropping = false;
        void files.upload(p.paths);
      }
    })
    .then((u) => {
      unlisten = u;
    });
  return () => unlisten?.();
});

function pickHost(e: Event) {
  void files.open((e.currentTarget as HTMLSelectElement).value);
}

function activate(entry: RemoteEntry) {
  if (entry.kind === "dir" || entry.kind === "link") void files.go(entry.path);
  else void files.showPreview(entry);
}

function onListKey(e: KeyboardEvent) {
  const sel = files.shown.filter((x) => files.selected.includes(x.path));
  if (e.key === "Delete" && sel.length) {
    e.preventDefault();
    confirmDelete = sel.map((x) => x.path);
  } else if (e.key === "F2" && sel[0]) {
    e.preventDefault();
    naming = { kind: "rename", entry: sel[0], value: sel[0].name };
  } else if (e.key === "Enter" && sel[0]) {
    e.preventDefault();
    activate(sel[0]);
  } else if (e.key === "Backspace") {
    e.preventDefault();
    void files.up();
  }
}

let nameError = $state<string | null>(null);

async function submitName(e: SubmitEvent) {
  e.preventDefault();
  const n = naming;
  if (!n) return;
  const v = n.value.trim();
  nameError = !v
    ? `Type a name for the ${n.kind === "folder" ? "folder" : n.kind === "file" ? "file" : "item"}`
    : v.includes("/")
      ? "A name can't contain /"
      : v === "." || v === ".."
        ? "Pick another name"
        : null;
  if (nameError) return;
  naming = null;
  if (n.kind === "folder") await files.mkdir(n.value);
  else if (n.kind === "file") await files.touch(n.value);
  else if (n.entry) await files.rename(n.entry, n.value);
}

async function upload() {
  const picked = await pickFiles({ multiple: true, title: `Upload to ${files.cwd}` });
  if (picked) await files.upload(Array.isArray(picked) ? picked : [picked]);
}

async function downloadTo(paths: string[]) {
  const dir = await pickFiles({ directory: true, title: "Download to…" });
  if (typeof dir === "string") await files.download(paths, dir);
}

async function runMenu(action: string, entry: RemoteEntry) {
  menu = null;
  switch (action) {
    case "open":
      return activate(entry);
    case "edit":
      return files.edit(entry);
    case "download":
      return files.download([entry.path]);
    case "download-to":
      return downloadTo([entry.path]);
    case "rename":
      naming = { kind: "rename", entry, value: entry.name };
      return;
    case "perms":
      perms = entry;
      return;
    case "copy-path":
      await navigator.clipboard.writeText(entry.path);
      return files.flash("Path copied");
    case "copy-scp":
      if (host) {
        const port = host.port === 22 ? "" : `-P ${host.port} `;
        await navigator.clipboard.writeText(
          `scp ${port}${host.username}@${host.hostname}:${shellQuote(entry.path)} .`,
        );
        files.flash("scp command copied");
      }
      return;
    case "delete":
      confirmDelete = [entry.path];
      return;
  }
}

function sortBy(key: SortKey) {
  if (files.sortKey === key) files.sortAsc = !files.sortAsc;
  else {
    files.sortKey = key;
    files.sortAsc = true;
  }
}

function arrow(key: SortKey): string {
  return files.sortKey === key ? (files.sortAsc ? " ↑" : " ↓") : "";
}

function editPath() {
  typedPath = files.cwd;
  editingPath = true;
}
</script>

<Drawer
  title="Files"
  width="520px"
  onclose={() => shell.close()}
>
  <div class="relative flex h-full flex-col gap-2 pt-1">
    <select class="input" value={hostId} onchange={pickHost} aria-label="Host">
      {#each library.hosts as h (h.id)}
        <option value={h.id}>{h.label} ({h.username}@{h.hostname})</option>
      {/each}
    </select>

    <div class="flex items-center gap-1">
      <button type="button" class="tool" title="Up (Backspace)" aria-label="Up one folder" onclick={() => files.up()}>
        <ArrowUp size={14} />
      </button>
      {#if editingPath}
        <form
          class="min-w-0 flex-1"
          onsubmit={(e) => {
            e.preventDefault();
            editingPath = false;
            void files.goTyped(typedPath);
          }}
        >
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="input font-mono text-xs"
            bind:value={typedPath}
            onblur={() => (editingPath = false)}
            aria-label="Path"
            spellcheck="false"
            autofocus
          />
        </form>
      {:else}
        <nav class="crumbs min-w-0 flex-1" aria-label="Path" ondblclick={editPath}>
          {#each crumbs(files.cwd) as c, i (c.path)}
            {#if i > 1}<span class="text-faint">/</span>{/if}
            <button type="button" class="crumb" onclick={() => files.go(c.path)} in:blurFade={{ x: 8, y: 0, blur: 0, duration: 160 }}>{c.name}</button>
          {/each}
          <button type="button" class="crumb text-faint" title="Type a path" onclick={editPath}>…</button>
        </nav>
      {/if}
      <button
        type="button"
        class="tool"
        class:on={files.hostBookmarks.includes(files.cwd)}
        aria-label="Bookmark this folder"
        title="Bookmark"
        onclick={() => files.toggleBookmark(files.cwd)}
      >
        <BookmarkSimple size={14} weight={files.hostBookmarks.includes(files.cwd) ? "fill" : "regular"} />
      </button>
    </div>

    {#if files.hostBookmarks.length}
      <div class="flex flex-wrap gap-1">
        {#each files.hostBookmarks as b (b)}
          <button type="button" class="chip" onclick={() => files.go(b)}>{b.split("/").pop() || "/"}</button>
        {/each}
      </div>
    {/if}

    <div class="flex items-center gap-1">
      <button type="button" class="tool" aria-label="Refresh" title="Refresh" onclick={() => files.refresh()}>
        <ArrowClockwise size={14} />
      </button>
      <button type="button" class="tool" aria-label="New folder" title="New folder" onclick={() => (naming = { kind: "folder", value: "" })}>
        <FolderPlus size={14} />
      </button>
      <button type="button" class="tool" aria-label="New file" title="New file" onclick={() => (naming = { kind: "file", value: "" })}>
        <FilePlus size={14} />
      </button>
      <button type="button" class="tool" aria-label="Upload files" title="Upload (or drop files here)" onclick={upload}>
        <UploadSimple size={14} />
      </button>
      <button
        type="button"
        class="tool"
        aria-label="Download selected"
        title="Download to Downloads"
        disabled={files.selected.length === 0}
        onclick={() => files.download(files.selected)}
      >
        <DownloadSimple size={14} />
      </button>
      <button
        type="button"
        class="tool"
        class:on={files.showHidden}
        aria-label={files.showHidden ? "Hide hidden files" : "Show hidden files"}
        aria-pressed={files.showHidden}
        title="Hidden files"
        onclick={() => (files.showHidden = !files.showHidden)}
      >
        {#if files.showHidden}<Eye size={14} />{:else}<EyeSlash size={14} />{/if}
      </button>
      <input class="input filter" bind:value={files.filter} placeholder="Filter" aria-label="Filter files" spellcheck="false" />
      <button type="button" class="btn-ghost" title="Local and remote side by side" onclick={() => { files.dual = true; shell.close(); }}>Dual pane</button>
    </div>

    {#if naming}
      <form class="flex flex-wrap gap-1" onsubmit={submitName} novalidate transition:expand={{ duration: 160 }}>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="input flex-1"
          bind:value={naming.value}
          placeholder={naming.kind === "rename" ? "New name" : naming.kind === "folder" ? "Folder name" : "File name"}
          aria-label="Name"
          spellcheck="false"
          autofocus
          aria-invalid={!!nameError}
          oninput={() => (nameError = null)}
          onkeydown={(e) => e.key === "Escape" && (naming = null)}
        />
        <button type="submit" class="btn-primary">{naming.kind === "rename" ? "Rename" : "Create"}</button>
        <button type="button" class="btn-ghost" onclick={() => (naming = null)}>Cancel</button>
        {#if nameError}<div class="w-full"><FieldError message={nameError} /></div>{/if}
      </form>
    {/if}

    {#if confirmDelete}
      <ConfirmBar
        tone="danger"
        label="Confirm delete"
        confirmLabel="Delete"
        oncancel={() => (confirmDelete = null)}
        onconfirm={() => {
          const p = confirmDelete ?? [];
          confirmDelete = null;
          void files.remove(p);
        }}
      >
          Delete {confirmDelete.length === 1 ? confirmDelete[0]?.split("/").pop() : `${confirmDelete.length} items`}?
          <span class="text-faint">Folders are deleted with everything in them.</span>
      </ConfirmBar>
    {/if}

    <ClashBar />
    <EditsBar />

    {#if files.conflict}
      <div class="strip" role="alertdialog" aria-label="Edit conflict">
        <span class="flex-1 text-sm text-fg">
          {files.conflict.remote.split("/").pop()} changed on the server since you opened it.
        </span>
        <button type="button" class="btn-ghost" onclick={() => files.resolveConflict(false)}>Keep server's</button>
        <button type="button" class="btn-primary" onclick={() => files.resolveConflict(true)}>Upload mine</button>
      </div>
    {/if}

    <div class="head">
      <button type="button" class="col-name" onclick={() => sortBy("name")}>Name{arrow("name")}</button>
      <button type="button" class="col-size" onclick={() => sortBy("size")}>Size{arrow("size")}</button>
      <button type="button" class="col-time" onclick={() => sortBy("modified")}>Modified{arrow("modified")}</button>
      <span class="col-mode">Mode</span>
      <span class="w-6"></span>
    </div>

    <div
      class="list min-h-0 flex-1"
      role="listbox"
      aria-label="Files"
      aria-multiselectable="true"
      tabindex="0"
      onkeydown={onListKey}
    >
      {#if files.loading && files.entries.length === 0}
        {#each [1, 2, 3, 4] as i (i)}<div class="skeleton"></div>{/each}
      {:else if files.error}
        <p class="p-3 text-sm text-danger" role="alert">{files.error}</p>
      {:else}
        {#key files.cwd}
        <div in:blurFade={{ x: slide, y: 0, blur: 2, duration: 200 }}>
        {#each files.shown as entry (entry.path)}
          <div
            class="row"
            class:selected={files.selected.includes(entry.path)}
            role="option"
            aria-selected={files.selected.includes(entry.path)}
            tabindex="-1"
            data-path={entry.path}
            onclick={(e) => files.toggle(entry.path, e.ctrlKey || e.metaKey)}
            ondblclick={() => activate(entry)}
            onkeydown={() => {}}
          >
            <span class="col-name flex min-w-0 items-center gap-2">
              <span class="shrink-0 text-faint">
                {#if entry.kind === "dir"}<Folder size={15} weight="fill" />{:else if entry.kind === "link"}<LinkSimple size={15} />{:else}<File size={15} />{/if}
              </span>
              <span class="truncate text-sm text-fg">{entry.name}</span>
            </span>
            <span class="col-size tabular text-xs text-muted">{entry.kind === "dir" ? "" : formatSize(entry.size)}</span>
            <span class="col-time tabular text-xs text-muted">{formatTime(entry.modified)}</span>
            <span class="col-mode font-mono text-xs text-faint" title={octal(entry.mode)}>{modeString(entry.mode)}</span>
            <span class="w-6">
              <Menu
                label="Actions for {entry.name}"
                open={menu === entry.path}
                onopenchange={(o) => (menu = o ? entry.path : null)}
              >
                {#snippet trigger(props)}
                  <button type="button" class="tool" aria-label="Actions for {entry.name}" {...props}>
                    <DotsThree size={15} weight="bold" />
                  </button>
                {/snippet}
                  <button type="button" role="menuitem" class="menu-item" onclick={() => runMenu("open", entry)}>
                    {entry.kind === "dir" ? "Open" : "Preview"}
                  </button>
                  {#if entry.kind === "file"}
                    <button type="button" role="menuitem" class="menu-item" onclick={() => runMenu("edit", entry)}>Edit locally</button>
                  {/if}
                  <button type="button" role="menuitem" class="menu-item" onclick={() => runMenu("download", entry)}>Download</button>
                  <button type="button" role="menuitem" class="menu-item" onclick={() => runMenu("download-to", entry)}>Download to…</button>
                  <hr class="menu-sep" />
                  <button type="button" role="menuitem" class="menu-item" onclick={() => runMenu("rename", entry)}>Rename</button>
                  <button type="button" role="menuitem" class="menu-item" onclick={() => runMenu("perms", entry)}>Permissions…</button>
                  <button type="button" role="menuitem" class="menu-item" onclick={() => runMenu("copy-path", entry)}>Copy path</button>
                  <button type="button" role="menuitem" class="menu-item" onclick={() => runMenu("copy-scp", entry)}>Copy scp command</button>
                  <hr class="menu-sep" />
                  <button type="button" role="menuitem" class="menu-item danger" onclick={() => runMenu("delete", entry)}>Delete</button>
              </Menu>
            </span>
          </div>
        {:else}
          <p class="p-3 text-sm text-muted">
            {files.filter ? `Nothing matches “${files.filter}”.` : "This folder is empty. Drop files here to upload."}
          </p>
        {/each}
        </div>
        {/key}
      {/if}
    </div>

    <footer class="flex items-center gap-2 text-xs text-faint">
      <span class="flex-1">
        {files.shown.length} item{files.shown.length === 1 ? "" : "s"}{files.selected.length ? ` · ${files.selected.length} selected` : ""}
      </span>
      
    </footer>

    {#if dropping}
      <div class="drop" aria-hidden="true" in:blurFade={{ scale: 0.98, y: 0, blur: 0, duration: 140 }} out:sink={{ duration: 120 }}>
        Drop to upload to {files.cwd}
      </div>
    {/if}
  </div>

  {#if perms}
    <PermissionsCard entry={perms} onclose={() => (perms = null)} />
  {/if}

  <FilePreview />
</Drawer>

<style>
  .tool {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    flex-shrink: 0;
    color: var(--text-muted);
    border-radius: 4px;
  }
  .tool:hover:not(:disabled),
  .tool.on {
    color: var(--text);
    background: var(--surface-4);
  }
  .tool:disabled {
    opacity: 0.4;
  }
  .filter {
    flex: 1;
    min-width: 80px;
    padding-top: 3px;
    padding-bottom: 3px;
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
    background: var(--surface-4);
  }
  .chip {
    padding: 1px 8px;
    font-size: 11px;
    color: var(--text-muted);
    background: var(--surface-4);
    border-radius: 999px;
  }
  .head,
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 4px 0 8px;
  }
  .head {
    font-size: 11px;
    color: var(--text-faint);
  }
  .head button {
    text-align: left;
  }
  .col-name {
    flex: 1;
    min-width: 0;
  }
  .col-size {
    width: 64px;
    text-align: right;
  }
  .col-time {
    width: 112px;
  }
  .col-mode {
    width: 76px;
  }
  .list {
    overflow-y: auto;
    outline: none;
  }
  .row {
    height: 30px;
    border-radius: var(--radius-sm);
    cursor: default;
  }
  .row:hover {
    background: var(--surface-4);
  }
  .row.selected {
    background: color-mix(in oklab, var(--accent) 18%, transparent);
  }
  .skeleton {
    height: 22px;
    margin: 6px 8px;
    border-radius: 4px;
  }
  .strip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 6px 6px 12px;
    background: var(--surface-4);
    border-left: 2px solid var(--warning);
    border-radius: var(--radius);
  }
  .drop {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    font-size: 14px;
    color: var(--accent);
    background: color-mix(in oklab, var(--bg) 70%, transparent);
    border: 2px dashed var(--accent);
    border-radius: var(--radius);
    pointer-events: none;
    animation: drop-glow 1.2s var(--ease-in-out) infinite;
  }
  @keyframes drop-glow {
    50% {
      box-shadow: inset 0 0 32px color-mix(in oklab, var(--accent) 22%, transparent);
    }
  }
  :global(:root[data-motion="reduced"]) .drop {
    animation: none;
  }
</style>
