/**
 * Split-pane layout as a binary tree. Panes are rendered flat (absolute rects computed here),
 * so splitting or closing never remounts a terminal and its scrollback survives.
 */

export type Layout<Id = string> =
  | { kind: "pane"; id: Id }
  | { kind: "split"; dir: "row" | "column"; ratio: number; a: Layout<Id>; b: Layout<Id> };

export type Direction = "left" | "right" | "up" | "down";

/** A rectangle in container fractions (0..1). */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export const MIN_RATIO = 0.1;

export function pane(id: string): Layout {
  return { kind: "pane", id };
}

/** Split pane `id`, putting `newId` to its right (`row`) or below it (`column`). */
export function split(root: Layout, id: string, dir: "row" | "column", newId: string): Layout {
  if (root.kind === "pane") {
    return root.id === id ? { kind: "split", dir, ratio: 0.5, a: root, b: pane(newId) } : root;
  }
  return { ...root, a: split(root.a, id, dir, newId), b: split(root.b, id, dir, newId) };
}

/** Remove pane `id`; its sibling takes the space. Returns null when nothing is left. */
export function remove(root: Layout, id: string): Layout | null {
  if (root.kind === "pane") return root.id === id ? null : root;
  const a = remove(root.a, id);
  const b = remove(root.b, id);
  if (!a) return b;
  if (!b) return a;
  return { ...root, a, b };
}

/** Swap pane `oldId` for `newId` in place (reconnect into the same slot). */
export function replace(root: Layout, oldId: string, newId: string): Layout {
  if (root.kind === "pane") return root.id === oldId ? pane(newId) : root;
  return { ...root, a: replace(root.a, oldId, newId), b: replace(root.b, oldId, newId) };
}

/** Pane ids in reading order. */
export function panes(root: Layout): string[] {
  return root.kind === "pane" ? [root.id] : [...panes(root.a), ...panes(root.b)];
}

function halves(n: Extract<Layout, { kind: "split" }>, r: Rect): [Rect, Rect] {
  if (n.dir === "row") {
    const wa = r.w * n.ratio;
    return [
      { x: r.x, y: r.y, w: wa, h: r.h },
      { x: r.x + wa, y: r.y, w: r.w - wa, h: r.h },
    ];
  }
  const ha = r.h * n.ratio;
  return [
    { x: r.x, y: r.y, w: r.w, h: ha },
    { x: r.x, y: r.y + ha, w: r.w, h: r.h - ha },
  ];
}

/** Every pane's rectangle. */
export function rects(root: Layout, r: Rect = { x: 0, y: 0, w: 1, h: 1 }): Map<string, Rect> {
  const out = new Map<string, Rect>();
  const walk = (n: Layout, rect: Rect) => {
    if (n.kind === "pane") {
      out.set(n.id, rect);
      return;
    }
    const [ra, rb] = halves(n, rect);
    walk(n.a, ra);
    walk(n.b, rb);
  };
  walk(root, r);
  return out;
}

/** A draggable divider: the split node, its whole rect, and where the line sits. */
export interface Divider {
  node: Extract<Layout, { kind: "split" }>;
  area: Rect;
  /** Position of the line along the split axis, as a container fraction. */
  at: number;
}

export function dividers(root: Layout, r: Rect = { x: 0, y: 0, w: 1, h: 1 }): Divider[] {
  if (root.kind === "pane") return [];
  const [ra, rb] = halves(root, r);
  const at = root.dir === "row" ? ra.x + ra.w : ra.y + ra.h;
  return [{ node: root, area: r, at }, ...dividers(root.a, ra), ...dividers(root.b, rb)];
}

/** New ratio for a divider dragged to `pos` (container fraction along its axis). */
export function ratioFor(d: Divider, pos: number): number {
  const start = d.node.dir === "row" ? d.area.x : d.area.y;
  const size = d.node.dir === "row" ? d.area.w : d.area.h;
  const ratio = size > 0 ? (pos - start) / size : 0.5;
  return Math.min(1 - MIN_RATIO, Math.max(MIN_RATIO, ratio));
}

/** The pane next to `id` in a direction: overlapping panes first, then the nearest. */
export function neighbor(root: Layout, id: string, dir: Direction): string | null {
  const all = rects(root);
  const from = all.get(id);
  if (!from) return null;
  const eps = 1e-6;
  const horizontal = dir === "left" || dir === "right";
  let best: { id: string; score: number } | null = null;
  for (const [other, r] of all) {
    if (other === id) continue;
    const gap =
      dir === "left"
        ? from.x - (r.x + r.w)
        : dir === "right"
          ? r.x - (from.x + from.w)
          : dir === "up"
            ? from.y - (r.y + r.h)
            : r.y - (from.y + from.h);
    if (gap < -eps) continue; // not on that side
    const overlap = horizontal
      ? Math.min(r.y + r.h, from.y + from.h) - Math.max(r.y, from.y)
      : Math.min(r.x + r.w, from.x + from.w) - Math.max(r.x, from.x);
    const score = (overlap > eps ? 0 : 10) + gap - overlap * 0.01;
    if (!best || score < best.score) best = { id: other, score };
  }
  return best?.id ?? null;
}

/** Rename every pane id (to save a layout by pane index, or to load one back). Panes whose
 *  new id is `null` are removed, their siblings taking the space. */
export function relabel<T extends string | number>(
  root: Layout<string | number>,
  f: (id: string | number) => T | null,
): Layout<T> | null {
  if (root.kind === "pane") {
    const id = f(root.id);
    return id === null ? null : { kind: "pane", id };
  }
  const a = relabel(root.a, f);
  const b = relabel(root.b, f);
  if (!a) return b;
  if (!b) return a;
  return { ...root, a, b };
}
