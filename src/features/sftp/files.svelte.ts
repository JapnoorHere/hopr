/**
 * The files drawer (SFTP, Phase 4a): one host at a time, its current folder, selection,
 * sort/filter, preview, bookmarks and locally edited files.
 */

import { wrap } from "$features/vault/vault.svelte";
import { scoped } from "$lib/design/theme";
import {
  type AppError,
  type ConflictPolicy,
  commands,
  events,
  inTauri,
  type Preview,
  type QueueResult,
  type RemoteEntry,
} from "$lib/ipc";
import { toast } from "$lib/ui/toast.svelte";
import { filterEntries, parentPath, type SortKey, sortEntries } from "./format";

const BOOKMARKS_KEY = "hopr.sftp.bookmarks";

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: AppError };

function loadBookmarks(): Record<string, string[]> {
  try {
    const raw = JSON.parse(localStorage.getItem(scoped(BOOKMARKS_KEY)) ?? "{}");
    return raw && typeof raw === "object" ? raw : {};
  } catch {
    return {};
  }
}

class FilesStore {
  hostId = $state<string | null>(null);
  cwd = $state("/");
  entries = $state<RemoteEntry[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  selected = $state<string[]>([]);
  showHidden = $state(false);
  filter = $state("");
  sortKey = $state<SortKey>("name");
  sortAsc = $state(true);
  preview = $state<{ entry: RemoteEntry; data: Preview } | null>(null);
  notice = $state<string | null>(null);
  /** An edit with a pending conflict (changed on the server too). */
  conflict = $state<{ editId: string; remote: string } | null>(null);
  bookmarks = $state<Record<string, string[]>>(loadBookmarks());
  /** Targets that already exist, waiting for "overwrite / keep both / skip". */
  clash = $state<{ names: string[]; resolve: (p: ConflictPolicy | null) => void } | null>(null);
  /** The local | remote dual pane is open. */
  dual = $state(false);
  #noticeTimer: ReturnType<typeof setTimeout> | undefined;
  #listening = false;

  get shown(): RemoteEntry[] {
    return sortEntries(
      filterEntries(this.entries, this.showHidden, this.filter),
      this.sortKey,
      this.sortAsc,
    );
  }

  get hostBookmarks(): string[] {
    return this.hostId ? (this.bookmarks[this.hostId] ?? []) : [];
  }

  /** A short confirmation or failure, shown as a toast. `notice` stays for tests. */
  flash(text: string, kind: "success" | "error" = "success") {
    toast.show(text, kind);
    this.notice = text;
    clearTimeout(this.#noticeTimer);
    this.#noticeTimer = setTimeout(() => (this.notice = null), 4000);
  }

  async #listen() {
    if (this.#listening || !inTauri) return;
    this.#listening = true;
    await events.editEvent.listen((e) => {
      const { editId, remote, status, message } = e.payload;
      const name = remote.split("/").pop();
      if (status === "saved") {
        this.flash(`Saved ${name} to the server`);
        if (parentPath(remote) === this.cwd) void this.refresh();
      } else if (status === "conflict") {
        this.conflict = { editId, remote };
      } else if (status === "error") {
        this.flash(message ?? `Couldn't save ${name}`, "error");
      }
    });
  }

  /** Browse a host, starting in `at` (the terminal's folder, from shell integration) or,
   *  if that can't be listed, its home folder. */
  async open(hostId: string, at: string | null = null) {
    void this.#listen();
    if (this.hostId === hostId && this.entries.length) {
      if (at && at !== this.cwd) await this.#goOr(at, this.cwd);
      return;
    }
    this.hostId = hostId;
    this.entries = [];
    this.selected = [];
    this.preview = null;
    this.loading = true;
    this.error = null;
    const res = wrap(await commands.sftpOpen(hostId));
    if (!res.ok) {
      this.loading = false;
      this.error = res.error.message;
      return;
    }
    if (at) await this.#goOr(at, res.data);
    else await this.go(res.data);
  }

  async #goOr(path: string, fallback: string) {
    await this.go(path);
    if (this.error && this.cwd !== path) await this.go(fallback);
  }

  async go(path: string) {
    if (!this.hostId) return;
    this.loading = true;
    this.error = null;
    const res = wrap(await commands.sftpList(this.hostId, path));
    this.loading = false;
    if (!res.ok) {
      this.error = res.error.message;
      return;
    }
    this.cwd = path;
    this.entries = res.data;
    this.selected = [];
  }

  /** A typed path: `~`, relative and `..` parts are resolved by the server. */
  async goTyped(text: string) {
    if (!this.hostId) return;
    const t = text.trim();
    const full = t.startsWith("/") || t.startsWith("~") ? t : `${this.cwd}/${t}`;
    const res = wrap(await commands.sftpResolve(this.hostId, full));
    if (res.ok) await this.go(res.data);
    else this.error = res.error.message;
  }

  refresh() {
    return this.go(this.cwd);
  }

  up() {
    return this.go(parentPath(this.cwd));
  }

  toggle(path: string, additive: boolean) {
    if (!additive) this.selected = [path];
    else if (this.selected.includes(path)) this.selected = this.selected.filter((p) => p !== path);
    else this.selected = [...this.selected, path];
  }

  async #run(p: Promise<Res<null>>, done?: string): Promise<boolean> {
    const res = await p;
    if (res.status === "error") {
      this.flash(res.error.message, "error");
      return false;
    }
    if (done) this.flash(done);
    await this.refresh();
    return true;
  }

  async mkdir(name: string) {
    return this.hostId ? this.#run(commands.sftpMkdir(this.hostId, this.cwd, name)) : false;
  }

  async touch(name: string) {
    return this.hostId ? this.#run(commands.sftpTouch(this.hostId, this.cwd, name)) : false;
  }

  async rename(entry: RemoteEntry, name: string) {
    return this.hostId ? this.#run(commands.sftpRename(this.hostId, entry.path, name)) : false;
  }

  async remove(paths: string[]) {
    if (!this.hostId || paths.length === 0) return false;
    const label = paths.length === 1 ? paths[0]?.split("/").pop() : `${paths.length} items`;
    return this.#run(commands.sftpRemove(this.hostId, paths), `Deleted ${label}`);
  }

  async chmod(path: string, mode: number, recursive: boolean) {
    if (!this.hostId) return false;
    return this.#run(commands.sftpChmod(this.hostId, path, mode, recursive), "Permissions changed");
  }

  /** Queue with `policy`; on clashes, ask once and apply the answer to all of them. */
  async #queue(run: (policy: ConflictPolicy) => Promise<Res<QueueResult>>) {
    let res = await run("ask");
    if (res.status === "ok" && res.data.conflicts.length) {
      const names = res.data.conflicts;
      const choice = await new Promise<ConflictPolicy | null>((resolve) => {
        this.clash = { names, resolve };
      });
      this.clash = null;
      if (!choice) return;
      res = await run(choice);
    }
    if (res.status === "error") this.flash(res.error.message, "error");
  }

  /** Answer the pending "already exists" question (null cancels). */
  answerClash(policy: ConflictPolicy | null) {
    this.clash?.resolve(policy);
  }

  async download(paths: string[], localDir: string | null = null) {
    const host = this.hostId;
    if (!host || paths.length === 0) return;
    await this.#queue((policy) => commands.transfersDownload(host, paths, localDir, policy));
  }

  async upload(localPaths: string[], remoteDir: string = this.cwd) {
    const host = this.hostId;
    if (!host || localPaths.length === 0) return;
    await this.#queue((policy) => commands.transfersUpload(host, localPaths, remoteDir, policy));
  }

  async showPreview(entry: RemoteEntry) {
    if (!this.hostId) return;
    const res = wrap(await commands.sftpPreview(this.hostId, entry.path));
    if (res.ok) this.preview = { entry, data: res.data };
    else this.flash(res.error.message, "error");
  }

  async edit(entry: RemoteEntry) {
    if (!this.hostId) return;
    void this.#listen();
    const res = wrap(await commands.sftpEdit(this.hostId, entry.path, true));
    if (res.ok) this.flash(`Editing ${entry.name}: changes upload when you save`);
    else this.flash(res.error.message, "error");
  }

  async resolveConflict(overwrite: boolean) {
    const c = this.conflict;
    this.conflict = null;
    if (c) await commands.sftpEditResolve(c.editId, overwrite);
  }

  toggleBookmark(path: string) {
    if (!this.hostId) return;
    const list = this.bookmarks[this.hostId] ?? [];
    const next = list.includes(path) ? list.filter((p) => p !== path) : [...list, path];
    this.bookmarks = { ...this.bookmarks, [this.hostId]: next };
    try {
      localStorage.setItem(scoped(BOOKMARKS_KEY), JSON.stringify(this.bookmarks));
    } catch {
      // Storage unavailable: bookmarks last for this run.
    }
  }
}

export const files = new FilesStore();
