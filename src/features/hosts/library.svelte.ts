/** Saved hosts, groups and keys, kept in sync with the backend after every change. */
import { sessions } from "$features/terminal/session.svelte";
import { wrap } from "$features/vault/vault.svelte";
import {
  type BulkEdit,
  commands,
  type ExportFormat,
  type Group,
  type Host,
  type HostInput,
  inTauri,
  type PingResult,
  type SshKeyInfo,
} from "$lib/ipc";
import { toast } from "$lib/ui/toast.svelte";
import { jumpTrail } from "./chain";

/** Hosts are probed at most this often while the list is visible (H9). */
export const PING_INTERVAL_MS = 60_000;

class LibraryStore {
  hosts = $state<Host[]>([]);
  groups = $state<Group[]>([]);
  keys = $state<SshKeyInfo[]>([]);
  loaded = $state(false);
  /** Host the editor drawer is open for (`"new"` = creating). */
  editing = $state<string | null>(null);
  /** Host the deploy-key dialog is open for, with an optional preselected key. */
  deploying = $state<{ hostId: string | null; keyId: string | null } | null>(null);
  /** Key just generated or imported, so an open host editor can select it. */
  addedKeyId = $state<string | null>(null);
  /** Last deleted hosts, for Undo. */
  undo = $state<{ ids: string[]; label: string } | null>(null);
  /** Latest ping result per host id. */
  reach = $state<Record<string, PingResult>>({});
  /** Short-lived confirmation ("Password copied…"). */
  notice = $state<string | null>(null);

  #undoTimer: ReturnType<typeof setTimeout> | undefined;
  #undoToast = 0;
  #noticeTimer: ReturnType<typeof setTimeout> | undefined;

  async refresh() {
    if (!inTauri) {
      this.loaded = true;
      return;
    }
    const res = await commands.libraryGet();
    if (res.status === "ok") {
      this.hosts = res.data.hosts;
      this.groups = res.data.groups;
      this.keys = res.data.keys;
      this.loaded = true;
    }
  }

  host(id: string): Host | undefined {
    return this.hosts.find((h) => h.id === id);
  }

  /** Open a terminal tab to a saved host (its jump trail labels the tab). */
  connect(h: Host) {
    const trail = jumpTrail(h, this.hosts).map((x) => x.label);
    sessions.open({
      kind: "host",
      hostId: h.id,
      label: h.label,
      environment: h.environment,
      trail,
    });
  }

  async save(id: string | null, input: HostInput) {
    const res = wrap(id ? await commands.hostUpdate(id, input) : await commands.hostCreate(input));
    if (res.ok) await this.refresh();
    return res;
  }

  async remove(ids: string[]) {
    const first = ids[0] ? this.host(ids[0]) : undefined;
    const res = wrap(await commands.hostsDelete(ids));
    if (res.ok) {
      const label = ids.length === 1 ? (first?.label ?? "host") : `${ids.length} hosts`;
      this.undo = { ids, label };
      clearTimeout(this.#undoTimer);
      this.#undoTimer = setTimeout(() => (this.undo = null), 8000);
      this.#undoToast = toast.info(`Deleted “${label}”`, {
        label: "Undo",
        run: () => void this.restore(),
      });
      await this.refresh();
    }
    return res;
  }

  async restore() {
    const u = this.undo;
    if (!u) return;
    this.undo = null;
    toast.dismiss(this.#undoToast);
    const r = wrap(await commands.hostsRestore(u.ids));
    if (r.ok) toast.success(`Restored “${u.label}”`);
    else toast.error(r.error.message);
    await this.refresh();
  }

  async duplicate(h: Host) {
    const res = wrap(await commands.hostDuplicate(h.id));
    if (res.ok) await this.refresh();
    return res;
  }

  async bulkEdit(edit: BulkEdit) {
    const res = wrap(await commands.hostsBulkEdit(edit));
    if (res.ok) await this.refresh();
    return res;
  }

  async exportHosts(format: ExportFormat, ids: string[]) {
    return wrap(await commands.hostsExport(format, ids));
  }

  /** Probe hosts and merge the results into `reach`. */
  async ping(ids: string[]) {
    if (!inTauri || ids.length === 0) return;
    const res = await commands.hostsPing(ids);
    if (res.status !== "ok") return;
    const next = { ...this.reach };
    for (const r of res.data) next[r.id] = r;
    this.reach = next;
  }

  async copyPassword(h: Host) {
    const res = wrap(await commands.hostCopyPassword(h.id));
    if (res.ok) {
      this.flash(
        res.data > 0
          ? `Password for ${h.label} copied · clears in ${res.data} s`
          : `Password for ${h.label} copied`,
      );
    } else {
      this.flash(res.error.message, "error");
    }
    return res;
  }

  /** Show a short confirmation (or failure) as a toast. `notice` stays for tests. */
  flash(text: string, kind: "success" | "error" | "info" = "success") {
    toast.show(text, kind);
    this.notice = text;
    clearTimeout(this.#noticeTimer);
    this.#noticeTimer = setTimeout(() => (this.notice = null), 4000);
  }

  async toggleFavorite(h: Host) {
    h.favorite = !h.favorite; // optimistic
    await commands.hostSetFavorite(h.id, h.favorite);
  }

  async createGroup(name: string) {
    const res = wrap(await commands.groupCreate(name));
    if (res.ok) await this.refresh();
    return res;
  }

  async deleteGroup(id: string) {
    const res = wrap(await commands.groupDelete(id));
    if (res.ok) await this.refresh();
    return res;
  }
}

export const library = new LibraryStore();
