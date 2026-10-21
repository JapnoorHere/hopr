/**
 * Saved layouts (workspaces R6, tear-off T18): tabs as split trees whose panes are indexes
 * into a list of targets. Targets never carry secrets (a quick-connect password is not
 * part of `TabTarget`).
 */
import type { TabTarget } from "$features/terminal/session.svelte";
import { type Layout, panes, relabel } from "./layout";

export interface SavedTab {
  layout: Layout<number>;
  targets: TabTarget[];
  focused: number;
  zoomed: number | null;
}

export interface SavedWorkspace {
  version: 1;
  tabs: SavedTab[];
  active: number;
}

/** Save one tab. `targetOf` maps a pane key to what it connects to. */
export function saveTab(
  layout: Layout,
  focused: string,
  zoomed: string | null,
  targetOf: (key: string) => TabTarget | undefined,
): SavedTab | null {
  const keys = panes(layout).filter((k) => targetOf(k));
  if (keys.length === 0) return null;
  const index = new Map(keys.map((k, i) => [k, i]));
  const saved = relabel(layout, (k) => index.get(String(k)) ?? null);
  if (!saved) return null;
  return {
    layout: saved,
    targets: keys.map((k) => targetOf(k) as TabTarget),
    focused: index.get(focused) ?? 0,
    zoomed: zoomed !== null ? (index.get(zoomed) ?? null) : null,
  };
}

/** Load a saved tab: `make` opens a pane for a target (or returns null to drop it). */
export function loadTab(
  saved: SavedTab,
  make: (target: TabTarget, index: number) => string | null,
): { layout: Layout; focused: string; zoomed: string | null } | null {
  const keys = saved.targets.map((t, i) => make(t, i));
  const layout = relabel(saved.layout, (i) => keys[Number(i)] ?? null);
  if (!layout) return null;
  const live = panes(layout);
  const focused = keys[saved.focused] ?? null;
  const zoomed = saved.zoomed !== null ? (keys[saved.zoomed] ?? null) : null;
  return {
    layout,
    focused: focused && live.includes(focused) ? focused : (live[0] ?? ""),
    zoomed: zoomed && live.includes(zoomed) ? zoomed : null,
  };
}

function isTarget(t: unknown): t is TabTarget {
  if (!t || typeof t !== "object") return false;
  const o = t as Record<string, unknown>;
  switch (o.kind) {
    case "host":
      return typeof o.hostId === "string" && typeof o.label === "string";
    case "quick":
      return (
        typeof o.host === "string" && typeof o.port === "number" && typeof o.username === "string"
      );
    case "local":
      return typeof o.shellId === "string" && typeof o.label === "string";
    default:
      return false;
  }
}

function isLayout(l: unknown, n: number): l is Layout<number> {
  if (!l || typeof l !== "object") return false;
  const o = l as Record<string, unknown>;
  if (o.kind === "pane") {
    return Number.isInteger(o.id) && (o.id as number) >= 0 && (o.id as number) < n;
  }
  return (
    o.kind === "split" &&
    (o.dir === "row" || o.dir === "column") &&
    typeof o.ratio === "number" &&
    isLayout(o.a, n) &&
    isLayout(o.b, n)
  );
}

/** Parse stored JSON, dropping anything malformed instead of failing. */
export function parseWorkspace(json: string): SavedWorkspace | null {
  let data: unknown;
  try {
    data = JSON.parse(json);
  } catch {
    return null;
  }
  const o = data as Partial<SavedWorkspace> | null;
  if (!o || o.version !== 1 || !Array.isArray(o.tabs)) return null;
  const tabs = o.tabs.filter(
    (t): t is SavedTab =>
      !!t &&
      Array.isArray(t.targets) &&
      t.targets.length > 0 &&
      t.targets.every(isTarget) &&
      isLayout(t.layout, t.targets.length),
  );
  const active = Number.isInteger(o.active) ? (o.active as number) : 0;
  return {
    version: 1,
    tabs,
    active: Math.min(Math.max(0, active), Math.max(0, tabs.length - 1)),
  };
}

/** Panes in a workspace (for "Restore last session (3 tabs, 5 panes)"). */
export function paneCount(ws: SavedWorkspace): number {
  return ws.tabs.reduce((n, t) => n + t.targets.length, 0);
}
