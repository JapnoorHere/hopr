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
  }

  update(patch: Partial<Appearance>) {
    this.current = { ...this.current, ...patch };
    applyAppearance(this.current);
    saveAppearance(this.current, safeStorage());
  }
}

export const appearance = new AppearanceStore();
