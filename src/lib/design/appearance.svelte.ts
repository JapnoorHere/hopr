import { getCurrentWindow } from "@tauri-apps/api/window";
import { inTauri } from "$lib/ipc";
import { type Appearance, applyAppearance, loadAppearance, saveAppearance } from "./theme";
import { themeSpec, themeTokens } from "./themes";

class AppearanceStore {
  current = $state<Appearance>(loadAppearance());
  /** The theme in use, and its tokens: computed, so readers never wait on the DOM. */
  spec = $derived(themeSpec(this.current.theme, this.current.custom));
  tokens = $derived(themeTokens(this.spec));

  init() {
    // Read again: the open profile's storage scope is set by now.
    this.current = loadAppearance();
    applyAppearance(this.current);
    void this.#windowTheme();
    // "System" motion follows Windows' animation setting live.
    try {
      globalThis
        .matchMedia?.("(prefers-reduced-motion: reduce)")
        .addEventListener("change", () => applyAppearance(this.current));
    } catch {
      // No matchMedia (tests): the setting is read once.
    }
  }

  update(patch: Partial<Appearance>) {
    const themeChanged = patch.theme !== undefined && patch.theme !== this.current.theme;
    this.current = { ...this.current, ...patch };
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
    saveAppearance(this.current);
    if (patch.theme !== undefined) void this.#windowTheme();
  }

  /** Match the window frame to the theme's light/dark, and clear any old backdrop effect. */
  async #windowTheme() {
    if (!inTauri) return;
    try {
      const win = getCurrentWindow();
      const dark = this.tokens.scheme !== "light";
      await win.setTheme(dark ? "dark" : "light");
      await win.clearEffects();
    } catch {
      // Older Windows or no permission: nothing to change.
    }
  }
}

export const appearance = new AppearanceStore();
