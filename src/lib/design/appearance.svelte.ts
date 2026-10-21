import {
  type Appearance,
  applyAppearance,
  loadAppearance,
  safeStorage,
  saveAppearance,
} from "./theme";

class AppearanceStore {
  current = $state<Appearance>(loadAppearance(safeStorage()));

  init() {
    applyAppearance(this.current);
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
  }
}

export const appearance = new AppearanceStore();
