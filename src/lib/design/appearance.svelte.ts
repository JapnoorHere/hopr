import { getCurrentWindow } from "@tauri-apps/api/window";
import { inTauri } from "$lib/ipc";
import {
  type Appearance,
  applyAppearance,
  loadAppearance,
  safeStorage,
  saveAppearance,
} from "./theme";
import { themeSpec, themeTokens } from "./themes";

class AppearanceStore {
  current = $state<Appearance>(loadAppearance(safeStorage()));
  /** The theme in use, and its tokens: computed, so readers never wait on the DOM. */
  spec = $derived(themeSpec(this.current.theme, this.current.custom));
  tokens = $derived(themeTokens(this.spec));

  init() {
    applyAppearance(this.current);
    void this.#backdrop();
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
    saveAppearance(this.current, safeStorage());
    if (patch.backdrop !== undefined || patch.theme !== undefined) void this.#backdrop();
  }

  /** Ask Windows for the Mica backdrop (or remove it), matching the theme's light/dark. */
  async #backdrop() {
    if (!inTauri) return;
    try {
      const win = getCurrentWindow();
      const dark = this.tokens.scheme !== "light";
      await win.setTheme(dark ? "dark" : "light");
      if (this.current.backdrop !== "solid")
        await win.setEffects({ effects: [this.current.backdrop as never] });
      else await win.clearEffects();
    } catch {
      // Older Windows or no permission: the frame stays solid, which is the fallback.
    }
  }
}

export const appearance = new AppearanceStore();
