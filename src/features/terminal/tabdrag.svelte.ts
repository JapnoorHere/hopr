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

  begin(tab: Tab, label: string) {
    this.tab = tab;
    this.label = label;
    this.target = { kind: "strip" };
  }

  /** Follow the pointer. `strip` is the tab strip's box; `canLeave` allows dropping outside. */
  update(e: PointerEvent, strip: DOMRect, canLeave: boolean) {
    this.x = e.clientX;
    this.y = e.clientY;
    const outside =
      e.clientX < 0 || e.clientY < 0 || e.clientX > innerWidth || e.clientY > innerHeight;
    if (outside) {
      this.target = canLeave ? { kind: "outside" } : { kind: "none" };
    } else if (e.clientY >= strip.top - STRIP_SLACK && e.clientY <= strip.bottom + STRIP_SLACK) {
      this.target = { kind: "strip" };
    } else {
      this.target = this.#paneAt(e.clientX, e.clientY);
    }
  }

  /** A pane of another tab under the pointer, and the side the drop would take. */
  #paneAt(x: number, y: number): DropTarget {
    const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-pane-key]");
    const session = el ? sessions.session(el.dataset.paneKey ?? "") : undefined;
    if (!el || !session || !this.tab || sessions.tabOf(session) === this.tab) {
      return { kind: "none" };
    }
    const r = el.getBoundingClientRect();
    const side = sideAt((x - r.left) / r.width, (y - r.top) / r.height);
    return { kind: "pane", session, side, zone: zoneOf(r, side) };
  }

  /** Let go: do what the target says. `screen` is the pointer in screen coordinates. */
  async drop(screen: { x: number; y: number }) {
    const tab = this.tab;
    const target = this.target;
    this.tab = null;
    if (!tab) return;
    if (target.kind === "pane") sessions.mergeInto(tab, target.session, target.side);
    else if (target.kind === "outside") await sessions.dropOutside(tab, screen.x, screen.y);
  }

  cancel() {
    this.tab = null;
  }
}

export const tabDrag = new TabDrag();
