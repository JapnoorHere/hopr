/**
 * Dragging a tab, like a browser: along the strip it reorders; down onto a pane it drops
 * beside that pane as a split; out of the window it moves to the Hopr window whose tab bar
 * is under the pointer, or to a new window there.
 */
import { type Side, sideAt } from "$features/workspace/layout";
import { sessions, type Tab } from "./session.svelte";
import type { TerminalSession } from "./terminal-session.svelte";

/** Pointer travel before a press becomes a drag (a click stays a click). */
export const START_PX = 6;
/** How far above or below the strip the pointer can stray while still reordering. */
const STRIP_SLACK = 28;
/** A drop this close to a pane's edge (as a fraction of it) splits; nearer the middle it
 *  moves the tab to a new window, so that works in a maximized window too. */
const EDGE = 0.25;

/** Where a drop lands, as screen geometry for the drop preview. */
export type DropTarget =
  | { kind: "strip" }
  | { kind: "pane"; session: TerminalSession; side: Side; zone: Box }
  | { kind: "outside" }
  | { kind: "none" };

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** The half of a pane a drop on `side` takes. */
export function zoneOf(r: Box, side: Side): Box {
  const halfW = r.width / 2;
  const halfH = r.height / 2;
  switch (side) {
    case "left":
      return { left: r.left, top: r.top, width: halfW, height: r.height };
    case "right":
      return { left: r.left + halfW, top: r.top, width: halfW, height: r.height };
    case "top":
      return { left: r.left, top: r.top, width: r.width, height: halfH };
    case "bottom":
      return { left: r.left, top: r.top + halfH, width: r.width, height: halfH };
  }
}

/** Where tab `dragged` would move in the strip, from the pointer and the other tabs' centres. */
export function stripIndex(x: number, centres: number[]): number {
  return centres.filter((c) => c < x).length;
}

class TabDrag {
  tab = $state<Tab | null>(null);
  /** The dragged tab's name, for the chip that follows the pointer. */
  label = $state("");
  x = $state(0);
  y = $state(0);
  target = $state<DropTarget>({ kind: "strip" });
  /** Whether a drop can move the tab out to another window. */
  #canLeave = false;
  /** The dragged tab was showing: off the strip, a neighbour shows so there is a pane to
   *  split beside; back on the strip (or on a drop that does not split) it shows again. */
  #wasActive = false;

  begin(tab: Tab, label: string) {
    this.tab = tab;
    this.label = label;
    this.target = { kind: "strip" };
    this.#wasActive = sessions.activeKey === tab.key;
  }

  /** Show the dragged tab (on) or the tab beside it (off), when it was the one showing. */
  #show(dragged: boolean) {
    const tab = this.tab;
    if (!tab || !this.#wasActive) return;
    if (dragged) {
      sessions.activate(tab.key);
      return;
    }
    if (sessions.activeKey !== tab.key) return;
    const i = sessions.tabs.indexOf(tab);
    const next = sessions.tabs[i - 1] ?? sessions.tabs[i + 1];
    if (next) sessions.activate(next.key);
  }

  /** Follow the pointer. `strip` is the tab strip's box; `canLeave` allows dropping outside. */
  update(e: PointerEvent, strip: DOMRect, canLeave: boolean) {
    this.x = e.clientX;
    this.y = e.clientY;
    this.#canLeave = canLeave;
    const outside =
      e.clientX < 0 || e.clientY < 0 || e.clientX > innerWidth || e.clientY > innerHeight;
    if (outside) {
      this.target = canLeave ? { kind: "outside" } : { kind: "none" };
    } else if (e.clientY >= strip.top - STRIP_SLACK && e.clientY <= strip.bottom + STRIP_SLACK) {
      this.#show(true);
      this.target = { kind: "strip" };
    } else {
      this.#show(false);
      this.target = this.#paneAt(e.clientX, e.clientY);
    }
  }

  /** The pane of another tab nearest the pointer (the one under it, or the closest when
   *  the pointer is over the rail, status bar or a gap), and the side the drop would take. */
  #paneAt(x: number, y: number): DropTarget {
    const elsewhere: DropTarget = this.#canLeave ? { kind: "outside" } : { kind: "none" };
    let best: { session: TerminalSession; r: DOMRect; d: number } | null = null;
    for (const el of document.querySelectorAll<HTMLElement>("[data-pane-key]")) {
      const session = sessions.session(el.dataset.paneKey ?? "");
      const r = el.getBoundingClientRect();
      if (!session || !this.tab || sessions.tabOf(session) === this.tab) continue;
      if (r.width === 0 || r.height === 0) continue;
      const dx = Math.max(r.left - x, 0, x - r.right);
      const dy = Math.max(r.top - y, 0, y - r.bottom);
      const d = Math.hypot(dx, dy);
      if (!best || d < best.d) best = { session, r, d };
    }
    if (!best) return elsewhere;
    const { session, r } = best;
    const fx = Math.min(1, Math.max(0, (x - r.left) / r.width));
    const fy = Math.min(1, Math.max(0, (y - r.top) / r.height));
    if (Math.min(fx, 1 - fx, fy, 1 - fy) > EDGE) return elsewhere;
    const side = sideAt(fx, fy);
    return { kind: "pane", session, side, zone: zoneOf(r, side) };
  }

  /** Let go: do what the target says. `screen` is the pointer in screen coordinates. */
  async drop(screen: { x: number; y: number }) {
    const tab = this.tab;
    const target = this.target;
    if (target.kind !== "pane") this.#show(true);
    this.tab = null;
    if (!tab) return;
    if (target.kind === "pane") sessions.mergeInto(tab, target.session, target.side);
    else if (target.kind === "outside") await sessions.dropOutside(tab, screen.x, screen.y);
  }

  cancel() {
    this.#show(true);
    this.tab = null;
  }
}

export const tabDrag = new TabDrag();
