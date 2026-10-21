/**
 * A cursor that glides (terminal revamp T3): xterm's own cursor is made invisible and this
 * layer, drawn over the text, follows it, sliding to each new position and blinking with a
 * fade. It hides whenever xterm's cursor would not show: a program hid it (DECTCEM, as
 * vim and htop do), the view is scrolled back, or it is off the screen.
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

export function attachGlideCursor(t: Terminal): GlideCursor {
  const root = t.element;
  const screen = root?.querySelector<HTMLElement>(".xterm-screen");
  const el = document.createElement("div");
  el.className = "glide-cursor";
  el.setAttribute("aria-hidden", "true");
  root?.appendChild(el);

  let hiddenByApp = false;
  let restTimer: ReturnType<typeof setTimeout> | undefined;
  let lastKey = "";

  const place = () => {
    if (!screen) return;
    // Focus can change before our listeners attach (the pane focuses itself on open).
    el.classList.toggle("unfocused", document.activeElement !== t.textarea);
    const buf = t.buffer.active;
    const row = buf.baseY + buf.cursorY - buf.viewportY;
    const cw = screen.clientWidth / t.cols;
    const ch = screen.clientHeight / t.rows;
    const visible = !hiddenByApp && row >= 0 && row < t.rows && cw > 0;
    el.style.opacity = visible ? "" : "0";
    if (!visible) return;
    el.style.setProperty("--cw", `${cw}px`);
    el.style.setProperty("--ch", `${ch}px`);
    el.style.transform = `translate(${screen.offsetLeft + buf.cursorX * cw}px, ${screen.offsetTop + row * ch}px)`;
    // Moving keeps the cursor solid for a moment, then blinking resumes.
    const key = `${buf.cursorX},${row}`;
    if (key !== lastKey) {
      lastKey = key;
      el.classList.add("moving");
      clearTimeout(restTimer);
      restTimer = setTimeout(() => el.classList.remove("moving"), 520);
    }
  };

  const onFocus = () => el.classList.toggle("unfocused", false);
  const onBlur = () => el.classList.toggle("unfocused", true);
  t.textarea?.addEventListener("focus", onFocus);
  t.textarea?.addEventListener("blur", onBlur);

  const subs: IDisposable[] = [
    t.onCursorMove(place),
    t.onScroll(place),
    t.onRender(place),
    t.onResize(place),
    // Track DECTCEM ourselves; returning false lets xterm handle the code as usual.
    t.parser.registerCsiHandler({ prefix: "?", final: "l" }, (params) => {
      if (hasCursorMode(params)) hiddenByApp = true;
      return false;
    }),
    t.parser.registerCsiHandler({ prefix: "?", final: "h" }, (params) => {
      if (hasCursorMode(params)) hiddenByApp = false;
      return false;
    }),
  ];
  place();

  return {
    set(shape, blink, color) {
      el.dataset.shape = shape;
      el.classList.toggle("blink", blink);
      el.style.setProperty("--cursor", color);
      place();
    },
    dispose() {
      for (const s of subs) s.dispose();
      clearTimeout(restTimer);
      t.textarea?.removeEventListener("focus", onFocus);
      t.textarea?.removeEventListener("blur", onBlur);
      el.remove();
    },
  };
}
