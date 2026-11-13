/** Where the sidebar sits and whether it hides until the pointer reaches its edge. Saved
 *  per device; every window follows a change. */
import { readJson, writeJson } from "$lib/storage";

export type RailSide = "left" | "right";

export interface LayoutPrefs {
  railSide: RailSide;
  /** Hidden until the pointer touches that edge of the window. */
  railAutoHide: boolean;
}

const KEY = "hopr.layout";
const DEFAULTS: LayoutPrefs = { railSide: "left", railAutoHide: false };

export function parseLayout(raw: unknown): LayoutPrefs {
  const r = (raw && typeof raw === "object" ? raw : {}) as Partial<
    Record<keyof LayoutPrefs, unknown>
  >;
  return {
    railSide: r.railSide === "right" ? "right" : "left",
    railAutoHide: typeof r.railAutoHide === "boolean" ? r.railAutoHide : DEFAULTS.railAutoHide,
  };
}

class LayoutStore {
  current = $state<LayoutPrefs>(readJson(KEY, parseLayout, DEFAULTS));

  update(patch: Partial<LayoutPrefs>) {
    this.current = parseLayout({ ...this.current, ...patch });
    writeJson(KEY, this.current);
  }

  /** Another window changed it. */
  reload() {
    this.current = readJson(KEY, parseLayout, DEFAULTS);
  }
}

export const layout = new LayoutStore();
