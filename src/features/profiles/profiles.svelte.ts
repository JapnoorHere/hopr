/**
 * Profiles: separate vaults, each in its own data folder (see src-tauri/src/profiles.rs).
 * Switching restarts Hopr into the chosen profile; the picker shows at start-up.
 */
import { wrap } from "$features/vault/vault.svelte";
import { setStorageScope } from "$lib/design/theme";
import type { ThemeSpec } from "$lib/design/themes";
import { commands, inTauri, type Profile } from "$lib/ipc";

class ProfilesStore {
  list = $state<Profile[]>([]);
  current = $state("default");
  askAtStart = $state(true);
  /** The picker is showing (at start-up, or opened from the switcher). */
  picking = $state(false);
  /** The picker was opened from the switcher (so it can be dismissed). */
  fromMenu = $state(false);
  /** A switch is under way (Hopr is about to restart). */
  switching = $state<string | null>(null);

  active = $derived(this.list.find((p) => p.id === this.current) ?? null);

  /** Before anything reads browser storage: which profile is open, and should we ask. */
  async load() {
    if (!inTauri) return;
    const v = await commands.profilesGet();
    this.list = v.profiles;
    this.current = v.current;
    this.askAtStart = v.askAtStart;
    this.picking = v.askNow;
    setStorageScope(v.current);
  }

  async refresh() {
    if (!inTauri) return;
    const v = await commands.profilesGet();
    this.list = v.profiles;
    this.askAtStart = v.askAtStart;
  }

  async create(name: string, color: string) {
    const r = wrap(await commands.profileCreate(name, color));
    if (r.ok) await this.refresh();
    return r;
  }

  async update(id: string, patch: { name?: string; color?: string }) {
    const r = wrap(await commands.profileUpdate(id, patch.name ?? null, patch.color ?? null));
    if (r.ok) await this.refresh();
    return r;
  }

  async remove(id: string) {
    const r = wrap(await commands.profileDelete(id));
    if (r.ok) await this.refresh();
    return r;
  }

  async setAskAtStart(ask: boolean) {
    this.askAtStart = ask;
    wrap(await commands.profilesSetAskAtStart(ask));
  }

  /** Open a profile: the current one just closes the picker; another restarts Hopr into it. */
  async open(id: string) {
    if (id === this.current) {
      this.picking = false;
      return { ok: true as const, data: null };
    }
    this.switching = id;
    const r = wrap(await commands.profileSwitch(id));
    if (!r.ok) this.switching = null;
    return r;
  }

  /** Keep the open profile's colours in the registry, so the picker can draw it. */
  #lastLook = "";
  syncLook(spec: ThemeSpec) {
    if (!inTauri) return;
    const look = { bg: spec.bg, text: spec.text, accent: spec.accent };
    const key = JSON.stringify(look);
    if (key === this.#lastLook) return;
    this.#lastLook = key;
    void commands.profileSetLook(look).then(() => this.refresh());
  }
}

export const profiles = new ProfilesStore();

/** What to show for a profile's name (the original data starts unnamed). */
export function profileName(p: Profile | null | undefined): string {
  return p?.name || "Unnamed profile";
}

/** The avatar letter. */
export function initial(p: Profile): string {
  return (p.name.trim()[0] ?? "?").toUpperCase();
}
