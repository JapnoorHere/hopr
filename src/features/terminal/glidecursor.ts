/**
 * A cursor that glides (terminal revamp T3): xterm's own cursor is made invisible and this
 * layer, drawn over the text, follows it, sliding to each new position and blinking with a
 * fade. It hides whenever xterm's cursor would not show: a program hid it (DECTCEM, as
 * vim and htop do), the view is scrolled back, or it is off the screen.
 *
 * Fast output moves the cursor thousands of times a second, so the layer is placed at most
 * once per frame, the cell size is measured only when it changes, styles are written only
 * when they differ, and while output streams it jumps instead of sliding.
 */
import type { IDisposable, Terminal } from "@xterm/xterm";

export type CursorShape = "bar" | "block" | "underline";

export interface GlideCursor {
  set(shape: CursorShape, blink: boolean, color: string): void;
  dispose(): void;
}

/** DECTCEM (?25) in a CSI h/l parameter list. */
const hasCursorMode = (params: (number | number[])[]) =>
  params.some((p) => (Array.isArray(p) ? p.includes(25) : p === 25));

/** Moves closer together than this are output streaming, not typing: no slide. */
const STREAM_MS = 60;

interface Cell {
  w: number;
  h: number;
  left: number;
  top: number;
}

export function attachGlideCursor(t: Terminal): GlideCursor {
  const root = t.element;
  const screen = root?.querySelector<HTMLElement>(".xterm-screen");
  const el = document.createElement("div");
  el.className = "glide-cursor";
  el.setAttribute("aria-hidden", "true");
  root?.appendChild(el);

  let hiddenByApp = false;
  let restTimer: ReturnType<typeof setTimeout> | undefined;
  let frame = 0;
  let cell: Cell | null = null;
  let lastKey = "";
  let lastMove = 0;
  let shown = true;

  /** Cell size and offset: layout reads, so only after a resize or a font change. */
  const measure = (): Cell | null => {
    if (!screen || screen.clientWidth === 0) return null;
    const next = {
      w: screen.clientWidth / t.cols,
      h: screen.clientHeight / t.rows,
      left: screen.offsetLeft,
      top: screen.offsetTop,
    };
    el.style.setProperty("--cw", `${next.w}px`);
    el.style.setProperty("--ch", `${next.h}px`);
    lastKey = ""; // the same cell is somewhere else now
    return next;
  };

  const show = (on: boolean) => {
    if (on === shown) return;
    shown = on;
    el.style.opacity = on ? "" : "0";
  };

  const place = () => {
    frame = 0;
    cell ??= measure();
    const buf = t.buffer.active;
    const row = buf.baseY + buf.cursorY - buf.viewportY;
    const visible = !!cell && !hiddenByApp && row >= 0 && row < t.rows;
    show(visible);
    if (!visible || !cell) return;
    const key = `${buf.cursorX},${row}`;
    if (key === lastKey) return;
    lastKey = key;
    // Streaming output: jump with the text instead of trailing behind it.
    const now = performance.now();
    el.classList.toggle("jump", now - lastMove < STREAM_MS);
    lastMove = now;
    el.style.transform = `translate(${cell.left + buf.cursorX * cell.w}px, ${cell.top + row * cell.h}px)`;
    // Moving keeps the cursor solid for a moment, then blinking resumes.
    el.classList.add("moving");
    clearTimeout(restTimer);
    restTimer = setTimeout(() => el.classList.remove("moving"), 520);
  };
  const schedule = () => {
    frame ||= requestAnimationFrame(place);
  };
  const remeasure = () => {
    cell = null;
    schedule();
  };

  const onFocus = () => el.classList.toggle("unfocused", false);
  const onBlur = () => el.classList.toggle("unfocused", true);
  // Focus can change before these listeners attach (the pane focuses itself on open).
  el.classList.toggle("unfocused", document.activeElement !== t.textarea);
  t.textarea?.addEventListener("focus", onFocus);
  t.textarea?.addEventListener("blur", onBlur);
  // The screen changes size with the window and with the font (cell size).
  const observer = new ResizeObserver(remeasure);
  if (screen) observer.observe(screen);

  const subs: IDisposable[] = [
    t.onCursorMove(schedule),
    t.onScroll(schedule),
    t.onResize(remeasure),
    // Track DECTCEM ourselves; returning false lets xterm handle the code as usual.
    t.parser.registerCsiHandler({ prefix: "?", final: "l" }, (params) => {
      if (hasCursorMode(params)) {
        hiddenByApp = true;
        schedule();
      }
      return false;
    }),
    t.parser.registerCsiHandler({ prefix: "?", final: "h" }, (params) => {
      if (hasCursorMode(params)) {
        hiddenByApp = false;
        schedule();
      }
      return false;
    }),
  ];
  schedule();

  return {
    set(shape, blink, color) {
      el.dataset.shape = shape;
      el.classList.toggle("blink", blink);
      el.style.setProperty("--cursor", color);
      remeasure(); // a font change arrives with new settings
    },
    dispose() {
      for (const s of subs) s.dispose();
      cancelAnimationFrame(frame);
      clearTimeout(restTimer);
      observer.disconnect();
      t.textarea?.removeEventListener("focus", onFocus);
      t.textarea?.removeEventListener("blur", onBlur);
      el.remove();
    },
  };
}
