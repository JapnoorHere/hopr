/**
 * Shell integration on an xterm (OSC 133/633/7): prompt marks to jump between, command blocks
 * with exit chips and a pinned header, click-to-move on the line being typed, and commands
 * read back from typing when the shell sends no marks.
 */
import type { IMarker, Terminal } from "@xterm/xterm";
import { readLines } from "./buffer";
import { prefs } from "./prefs.svelte";
import { CommandTracker, type Mark, parse133, parse633, parseOsc7 } from "./shellint";
import type { TerminalSession } from "./terminal-session.svelte";

/** Prompt marks kept for jumping (older ones are dropped with their scrollback anyway). */
const MAX_PROMPTS = 500;
const encoder = new TextEncoder();

export interface ShellIntegration {
  /** Scroll to the previous (-1) or next (1) prompt. */
  jumpPrompt(dir: -1 | 1): void;
  hasPrompts(): boolean;
  /** What the user typed, so commands can be read back when the shell sends no marks. */
  typed(s: string): void;
}

export function attachShellIntegration(
  t: Terminal,
  host: HTMLElement,
  session: TerminalSession,
): ShellIntegration {
  // Shell integration (OSC 133/633/7): commands with exit codes, cwd, prompt marks.
  type Spot = { marker: IMarker; col: number };
  const prompts: IMarker[] = [];
  let inputAt: Spot | null = null;
  let promptAt: IMarker | null = null;
  /** Where the running command's output starts (OSC 133;C). */
  let outAt: IMarker | null = null;
  const spot = (): Spot | null => {
    const marker = t.registerMarker(0);
    return marker ? { marker, col: t.buffer.active.cursorX } : null;
  };
  /** Text from a spot to the cursor: what was typed at the prompt. */
  const readFrom = (s: Spot): string => {
    const buf = t.buffer.active;
    if (s.marker.isDisposed || s.marker.line < 0) return "";
    const end = buf.baseY + buf.cursorY;
    let out = "";
    for (let i = s.marker.line; i <= end; i++) {
      const line = buf.getLine(i);
      if (!line) break;
      const text = line.translateToString(true, i === s.marker.line ? s.col : 0);
      out += (i > s.marker.line && !line.isWrapped ? "\n" : "") + text;
    }
    return out.trim();
  };
  const addPrompt = (m: IMarker) => {
    prompts.push(m);
    if (prompts.length > MAX_PROMPTS) prompts.shift()?.dispose();
    // Command blocks: a hairline above every prompt after the first.
    if (prompts.length > 1 && prefs.current.blocks !== "off") {
      const sep = t.registerDecoration({ marker: m, x: 0, width: t.cols, layer: "bottom" });
      sep?.onRender((el) => el.classList.add("hopr-sep"));
    }
  };

  // Command blocks (terminal revamp T4): each finished command, with where its output is.
  type Block = {
    prompt: IMarker;
    out: IMarker | null;
    end: IMarker | null;
    command: string;
    exit: number | null;
    durationMs: number;
  };
  const blocks: Block[] = [];
  const blockOutput = (b: Block): string =>
    !b.out || b.out.isDisposed || !b.end || b.end.isDisposed
      ? ""
      : readLines(t, b.out.line, b.end.line);
  const took = (ms: number) =>
    ms >= 60_000 ? `${Math.round(ms / 60_000)}m` : `${(ms / 1000).toFixed(1)}s`;
  const addBlock = (b: Block) => {
    blocks.push(b);
    if (blocks.length > MAX_PROMPTS) blocks.shift();
    if (prefs.current.blocks !== "full" || b.prompt.isDisposed) return;
    const time = b.durationMs >= 1000 ? ` ${took(b.durationMs)}` : "";
    const label =
      b.exit === null ? `done${time}` : b.exit === 0 ? `✓${time}` : `✗ ${b.exit}${time}`;
    const width = label.length + 2;
    const chip = t.registerDecoration({
      marker: b.prompt,
      x: Math.max(0, t.cols - width - 1),
      width,
      layer: "top",
    });
    chip?.onRender((el) => {
      if (el.dataset.ready) return;
      el.dataset.ready = "1";
      el.classList.add("hopr-chip");
      el.dataset.exit = b.exit === null ? "" : String(b.exit);
      el.textContent = label;
      el.title = `${b.command} · click for actions`;
      el.addEventListener("mousedown", (e) => e.stopPropagation());
      el.addEventListener("click", (e) => {
        session.blockMenu = {
          command: b.command,
          output: blockOutput(b),
          x: e.clientX,
          y: e.clientY,
        };
      });
    });
  };
  // Scrolled back into the output of a command: pin that command at the top of the pane.
  const updateSticky = () => {
    const buf = t.buffer.active;
    const top = buf.viewportY;
    let found: Block | undefined;
    if (prefs.current.blocks === "full" && top < buf.baseY) {
      for (let i = blocks.length - 1; i >= 0; i--) {
        const b = blocks.at(i);
        if (!b || b.prompt.isDisposed) continue;
        if (b.prompt.line < top) {
          if ((b.end?.line ?? Number.POSITIVE_INFINITY) > top) found = b;
          break;
        }
      }
    }
    const next = found
      ? { command: found.command, exit: found.exit, line: found.prompt.line }
      : null;
    if (next?.line !== session.sticky?.line) session.sticky = next;
  };
  t.onScroll(updateSticky);

  // Click on the line being typed to move the cursor there (arrow keys are sent). Only
  // at a prompt that shell integration recognised, never in full-screen programs.
  host.addEventListener("mouseup", (e) => {
    if (e.button !== 0 || e.shiftKey || e.ctrlKey || e.altKey || e.metaKey) return;
    if (!inputAt || t.hasSelection() || t.modes.mouseTrackingMode !== "none") return;
    const buf = t.buffer.active;
    if (buf.type === "alternate" || buf.viewportY !== buf.baseY) return;
    const screen = t.element?.querySelector<HTMLElement>(".xterm-screen");
    if (!screen) return;
    const r = screen.getBoundingClientRect();
    const col = Math.floor(((e.clientX - r.left) / r.width) * t.cols);
    const row = Math.floor(((e.clientY - r.top) / r.height) * t.rows);
    if (row !== buf.cursorY) return;
    const start = inputAt.marker.line === buf.baseY + buf.cursorY ? inputAt.col : 0;
    const target = Math.max(start, Math.min(col, t.cols - 1));
    const dx = target - buf.cursorX;
    if (dx === 0) return;
    session.write(encoder.encode((dx > 0 ? "\x1b[C" : "\x1b[D").repeat(Math.abs(dx))));
  });
  const markExit = (marker: IMarker | null, exit: number | null) => {
    if (!marker || marker.isDisposed) return;
    const deco = t.registerDecoration({ marker, x: 0, width: 1, layer: "top" });
    deco?.onRender((el) => {
      el.classList.add("hopr-exit");
      el.dataset.exit = exit === null ? "" : String(exit);
      el.title = exit === null ? "Finished" : `Exit code ${exit}`;
    });
  };
  const tracker = new CommandTracker(() => (inputAt ? readFrom(inputAt) : ""));
  const onMark = (m: Mark | null): boolean => {
    if (!m) return true;
    if (m.kind === "prompt" && !tracker.running) {
      promptAt = t.registerMarker(0) ?? null;
      if (promptAt) addPrompt(promptAt);
    } else if (m.kind === "input" && !tracker.running) {
      inputAt = spot();
      // Shells that send only B still get a prompt mark.
      if (!promptAt && inputAt) {
        promptAt = inputAt.marker;
        addPrompt(promptAt);
      }
    } else if (m.kind === "cwd") {
      session.cwd = m.path;
    }
    const done = tracker.mark(m);
    session.integrated = tracker.integrated;
    if (m.kind === "exec") {
      inputAt = null;
      outAt = t.registerMarker(0) ?? null;
    }
    if (done) {
      if (promptAt && done.command) {
        addBlock({
          prompt: promptAt,
          out: outAt,
          end: t.registerMarker(0) ?? null,
          command: done.command,
          exit: done.exit,
          durationMs: done.durationMs,
        });
      }
      outAt = null;
      markExit(promptAt, done.exit);
      promptAt = null;
      session.commandFinished(done, true);
    }
    return true;
  };
  t.parser.registerOscHandler(133, (data) => onMark(parse133(data)));
  t.parser.registerOscHandler(633, (data) => onMark(parse633(data)));
  t.parser.registerOscHandler(7, (data) => {
    const path = parseOsc7(data);
    return onMark(path ? { kind: "cwd", path } : null);
  });

  // Without shell integration, commands are read back from what was typed: from the
  // first key after a prompt to Enter (approximate: no exit code, no full-screen apps).
  let typedAt: Spot | null = null;
  const forgetTyped = () => {
    typedAt?.marker.dispose();
    typedAt = null;
  };
  const watchTyped = (s: string) => {
    if (tracker.integrated || t.buffer.active.type === "alternate") return forgetTyped();
    if (s === "\r") {
      const command = typedAt ? readFrom(typedAt) : "";
      forgetTyped();
      if (command)
        session.commandFinished({ command, exit: null, cwd: session.cwd, durationMs: 0 }, false);
    } else if (s.includes("\r") || s === "\x03" || s === "\x04") {
      forgetTyped(); // pasted lines, Ctrl+C, Ctrl+D
    } else if (!typedAt && s.charCodeAt(0) >= 0x20) {
      typedAt = spot();
    }
  };

  return {
    jumpPrompt: (dir) => {
      const top = t.buffer.active.viewportY;
      const lines = prompts.filter((m) => !m.isDisposed && m.line >= 0).map((m) => m.line);
      const target = dir < 0 ? lines.filter((l) => l < top).at(-1) : lines.find((l) => l > top);
      if (target !== undefined) t.scrollToLine(target);
      else if (dir > 0) t.scrollToBottom();
    },
    hasPrompts: () => prompts.some((m) => !m.isDisposed),
    typed: watchTyped,
  };
}
