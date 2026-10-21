/** Signed updates (U6): check, download (signature verified in the backend), install. */
import { wrap } from "$features/vault/vault.svelte";
import { commands, inTauri, type UpdateInfo } from "$lib/ipc";

class UpdatesStore {
  available = $state<UpdateInfo | null>(null);
  /** The toast was closed for this version. */
  dismissed = $state(false);
  phase = $state<"idle" | "checking" | "downloading" | "installing">("idle");
  message = $state<string | null>(null);
  lastChecked = $state<Date | null>(null);

  /** `quiet`: background check, failures are not shown. */
  async check(quiet = false) {
    if (!inTauri || this.phase !== "idle") return;
    this.phase = "checking";
    if (!quiet) this.message = null;
    const res = wrap(await commands.updateCheck());
    this.phase = "idle";
    this.lastChecked = new Date();
    if (!res.ok) {
      if (!quiet) this.message = res.error.message;
      return;
    }
    if (res.data && res.data.version !== this.available?.version) this.dismissed = false;
    this.available = res.data;
    if (!quiet) this.message = res.data ? null : "Hopr is up to date";
  }

  /** Download and verify, then install and restart. */
  async install() {
    this.message = null;
    this.phase = "downloading";
    const dl = wrap(await commands.updateDownload());
    if (!dl.ok) {
      this.phase = "idle";
      this.message = dl.error.message;
      return;
    }
    this.phase = "installing";
    const res = wrap(await commands.updateInstall());
    if (!res.ok) {
      this.phase = "idle";
      this.message = res.error.message;
    }
  }
}

export const updates = new UpdatesStore();
