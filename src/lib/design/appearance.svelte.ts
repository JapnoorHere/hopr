import { getCurrentWindow } from "@tauri-apps/api/window";
import { setThemedWindowIcon } from "$lib/icons/window-icon";
import { inTauri } from "$lib/ipc";
import {
  type Appearance,
  applyAppearance,
  loadAppAppearance,
  loadAppearance,
  profileHasOwn,
  saveAppAppearance,
  saveAppearance,
  setProfileOwn,
} from "./theme";
import { themeSpec, themeTokens } from "./themes";

/** Whose look Settings is changing: Hopr's, or the open profile's. */
export type LookTarget = "app" | "profile";

/**
 * How this window looks. Hopr has its own look (the app theme); each profile follows it unless
 * it has its own. The profile picker and the quick terminal always show Hopr's.
 */
class AppearanceStore {
  /** What this window shows. */
  current = $state<Appearance>(loadAppearance());
  /** Hopr's look. */
  app = $state<Appearance>(loadAppearance());
  /** The open profile's look: its own, or Hopr's when it follows it. */
  profile = $state<Appearance>(loadAppearance());
  /** The open profile has its own look. */
  own = $state(false);
  /** This window shows Hopr's look whatever the profile (quick terminal, profile picker). */
  appOnly = $state(false);
  /** Whose look Settings is changing (the window previews it). */
  editing = $state<LookTarget>("app");
  /** The theme in use, and its tokens: computed, so readers never wait on the DOM. */
  spec = $derived(themeSpec(this.current.theme, this.current.custom));
  tokens = $derived(themeTokens(this.spec));
  /** Hopr's theme (the "Same as Hopr" card shows it). */
  appSpec = $derived(themeSpec(this.app.theme, this.app.custom));
  /** The open profile's theme (its picker tile shows it). */
  profileSpec = $derived(themeSpec(this.profile.theme, this.profile.custom));
  /** Changes go to Hopr's look (else to this profile's own). */
  editsApp = $derived(this.appOnly || this.editing === "app");

  init(appOnly = false) {
    this.appOnly = appOnly;
    // Read again: the open profile's storage scope is set by now.
    this.reload();
    this.resetEditing();
    // "System" motion follows Windows' animation setting live.
    try {
      globalThis
        .matchMedia?.("(prefers-reduced-motion: reduce)")
        .addEventListener("change", () => applyAppearance(this.current));
    } catch {
      // No matchMedia (tests): the setting is read once.
    }
  }

  /** Read the saved looks and apply (at start, and when another window changed them). */
  reload() {
    this.own = profileHasOwn();
    this.app = loadAppAppearance();
    this.profile = this.own ? loadAppearance() : this.app;
    this.#show(this.editsApp ? this.app : this.profile);
  }

  /** Show Hopr's look in this window (the profile picker is up), or the profile's again. */
  setAppOnly(on: boolean) {
    if (on === this.appOnly) return;
    this.appOnly = on;
    this.reload();
  }

  /** Change (and preview) Hopr's look or the profile's. */
  setEditing(target: LookTarget) {
    this.editing = target;
    this.#show(this.editsApp ? this.app : this.profile);
  }

  /** Back to the profile's own look being the one shown (Settings closed). */
  resetEditing() {
    this.setEditing(this.own ? "profile" : "app");
  }

  /** The profile follows Hopr's look again ("Same as Hopr"). */
  followApp() {
    setProfileOwn(false);
    this.reload();
  }

  update(patch: Partial<Appearance>) {
    const next = { ...this.current, ...patch };
    if (this.editsApp) {
      saveAppAppearance(next);
      this.app = next;
      if (!this.own) this.profile = next;
    } else {
      // Picking a look for the profile gives it its own.
      saveAppearance(next);
      if (!this.own) setProfileOwn(true);
      this.own = true;
      this.profile = next;
    }
    this.#show(next);
  }

  #show(next: Appearance) {
    const themeChanged = next.theme !== this.current.theme;
    this.current = next;
    const apply = () => applyAppearance(this.current);
    // A new theme cross-fades the whole window instead of flashing (View Transitions).
    const doc = globalThis.document as
      | (Document & { startViewTransition?: (cb: () => void) => unknown })
      | undefined;
    if (themeChanged && doc?.startViewTransition && doc.documentElement.dataset.motion !== "off") {
      doc.startViewTransition(apply);
    } else {
      apply();
    }
    void this.#windowTheme();
    void setThemedWindowIcon(this.spec);
  }

  /** Match the window frame to the theme's light or dark. */
  async #windowTheme() {
    if (!inTauri) return;
    try {
      const win = getCurrentWindow();
      const dark = this.tokens.scheme !== "light";
      await win.setTheme(dark ? "dark" : "light");
    } catch {
      // Older Windows: the frame keeps its default.
    }
  }
}

export const appearance = new AppearanceStore();
