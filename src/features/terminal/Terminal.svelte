<script lang="ts">
import "@xterm/xterm/css/xterm.css";
import { openUrl } from "@tauri-apps/plugin-opener";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { SerializeAddon } from "@xterm/addon-serialize";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { type IMarker, type ITerminalOptions, Terminal } from "@xterm/xterm";
import { untrack } from "svelte";
import { broadcast } from "$features/broadcast/broadcast.svelte";
import { appearance } from "$lib/design/appearance.svelte";
import { INVISIBLE, resolveCssColor, terminalTheme } from "$lib/design/palettes";
import { terminalColors } from "$lib/design/themes";
import { inTauri } from "$lib/ipc";
import { isMac, motionLevel } from "$lib/platform";
import { shortcuts } from "$lib/shortcuts";
import { toast } from "$lib/ui/toast.svelte";
import { attachGlideCursor, type GlideCursor } from "./glidecursor";
import { checkPaste, smartKey } from "./paste";
import { decodeOsc52, type TerminalPrefs } from "./prefs";
import { prefs } from "./prefs.svelte";
import SearchBar from "./SearchBar.svelte";
import type { TerminalSession } from "./session.svelte";
import { CommandTracker, type Mark, parse133, parse633, parseOsc7 } from "./shellint";

/** Prompt marks kept for jumping (older ones are dropped with their scrollback anyway). */
const MAX_PROMPTS = 500;

let { session, active }: { session: TerminalSession; active: boolean } = $props();

let host: HTMLDivElement;
let term = $state.raw<Terminal | null>(null);
let fitter = $state.raw<FitAddon | null>(null);
let search = $state.raw<SearchAddon | null>(null);
let searching = $state(false);
let pendingPaste = $state<{ text: string; lines: number } | null>(null);
let pendingClipboard = $state<string | null>(null);
let flash = $state(false);
/** The gliding cursor layer, while it is on (terminal revamp T3). */
let glide: GlideCursor | null = null;
const encoder = new TextEncoder();

function options(p: TerminalPrefs): ITerminalOptions {
  return {
    fontFamily: `"${p.fontFamily}", "Commit Mono", ui-monospace, monospace`,
    fontSize: p.fontSize,
    lineHeight: p.lineHeight,
    cursorStyle: p.cursorStyle,
    cursorBlink: p.cursorBlink,
    scrollback: p.scrollback,
    // Matte look: bold stays its colour, an unfocused pane shows a hollow cursor.
    drawBoldTextInBrightColors: false,
    cursorInactiveStyle: "outline",
  };
}

/** xterm theme: the app theme's colours, with the terminal background lifted if asked. */
function xtermTheme(p: TerminalPrefs) {
  return terminalTheme(terminalColors(appearance.spec, { lift: p.bgLift, opacity: 100 }));
}

/** Capture-phase paste guard: runs before xterm's own textarea handler. */
function onPaste(e: ClipboardEvent) {
  const text = e.clipboardData?.getData("text/plain") ?? "";
  const check = checkPaste(text);
  if (!check.confirm) return;
  e.preventDefault();
  e.stopPropagation();
  pendingPaste = { text, lines: check.lines };
}

async function pasteFromClipboard() {
  const text = await navigator.clipboard.readText().catch(() => "");
  if (!text) return;
  const check = checkPaste(text);
  if (check.confirm) pendingPaste = { text, lines: check.lines };
  else term?.paste(text);
}

function confirmPaste(ok: boolean) {
  const p = pendingPaste;
  pendingPaste = null;
  if (ok && p) term?.paste(p.text);
  term?.focus();
}

function confirmClipboard(ok: boolean) {
  const text = pendingClipboard;
  pendingClipboard = null;
  if (ok && text) void navigator.clipboard.writeText(text);
  term?.focus();
}

function onStripKey(e: KeyboardEvent, cancel: () => void) {
  if (e.key === "Escape") {
    e.preventDefault();
    cancel();
  }
}

function openLink(event: MouseEvent, uri: string) {
  // Links open on Ctrl/Cmd+click only, so plain clicks still select text.
  if (!(isMac ? event.metaKey : event.ctrlKey)) return;
  if (!/^https?:\/\//i.test(uri)) return;
  if (inTauri) void openUrl(uri);
  else window.open(uri, "_blank", "noopener");
}

function bell() {
  session.bellAt = Date.now();
  const mode = prefs.current.bell;
  if (mode === "visual") {
    flash = true;
    setTimeout(() => (flash = false), 150);
  } else if (mode === "sound") {
    try {
      const ctx = new AudioContext();
      const osc = ctx.createOscillator();
      osc.frequency.value = 880;
      osc.connect(ctx.destination);
      osc.start();
      osc.stop(ctx.currentTime + 0.08);
      osc.onended = () => void ctx.close();
    } catch {
      // No audio device: stay silent.
    }
  }
}

$effect(() => {
  let disposed = false;
  let observer: ResizeObserver | undefined;
  let instance: Terminal | undefined;

  (async () => {
    // xterm measures glyphs on open; measuring before the font loads gives a wrong grid.
    // Read once, untracked: later setting changes apply live below and must not rebuild
    // the terminal (that would wipe its scrollback).
    const p = untrack(() => prefs.current);
    await document.fonts.load(`${p.fontSize}px "${p.fontFamily}"`).catch(() => undefined);
    if (disposed) return;

    const t = new Terminal({
      ...options(p),
      allowProposedApi: true,
      macOptionIsMeta: true,
      theme: xtermTheme(prefs.current),
    });
    instance = t;
    const fit = new FitAddon();
    const finder = new SearchAddon();
    const serializer = new SerializeAddon();
    t.loadAddon(fit);
    t.loadAddon(finder);
    t.loadAddon(serializer);
    t.loadAddon(new Unicode11Addon());
    t.loadAddon(new WebLinksAddon(openLink));
    t.unicode.activeVersion = "11";
    t.open(host);
    // App shortcuts (registry) go to the app, not the shell.
    // A shortcut that moves focus (split, new tab) leaves its key-up to another element,
    // and xterm keeps thinking a key is held: it then drops text that arrives without key
    // events (IME, emoji panel, dictation, paste-as-typing). Release it on blur with a
    // key-up the handler below refuses: xterm clears the flag before asking, and a refused
    // key-up doesn't pull focus back to the terminal.
    let releasing = false;
    /** Copy the selection and clear it, so the next Ctrl+C interrupts as usual. */
    const copySelection = () => {
      const text = t.getSelection();
      if (!text) return false;
      void navigator.clipboard.writeText(text);
      t.clearSelection();
      toast.success(`Copied ${text.length} character${text.length === 1 ? "" : "s"}`);
      return true;
    };
    t.attachCustomKeyEventHandler((e) => {
      if (releasing && e.type === "keyup") return false;
      // Ctrl+Up / Ctrl+Down: jump between commands (with shell integration).
      if (
        e.type === "keydown" &&
        e.ctrlKey &&
        !e.shiftKey &&
        !e.altKey &&
        (e.key === "ArrowUp" || e.key === "ArrowDown") &&
        session.hasPrompts
      ) {
        e.preventDefault();
        session.jumpPrompt(e.key === "ArrowUp" ? -1 : 1);
        return false;
      }
      const smart = smartKey(e, prefs.current.smartCopyPaste, t.hasSelection());
      if (smart) {
        e.preventDefault();
        if (smart === "copy") copySelection();
        else void pasteFromClipboard();
        return false;
      }
      return !(e.type === "keydown" && shortcuts.find(e));
    });
    // Right-click: copy the selection, or paste when there is none (classic Windows console).
    host.addEventListener("contextmenu", (e) => {
      if (!prefs.current.rightClickCopyPaste) return;
      e.preventDefault();
      if (!copySelection()) void pasteFromClipboard();
    });
    t.textarea?.addEventListener("blur", () => {
      releasing = true;
      t.textarea?.dispatchEvent(new KeyboardEvent("keyup"));
      releasing = false;
    });
    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => webgl.dispose()); // falls back to the DOM renderer
      t.loadAddon(webgl);
    } catch {
      // No WebGL2: the DOM renderer keeps working.
    }

    const fitIfVisible = () => {
      if (host && host.clientWidth > 0 && host.clientHeight > 0) fit.fit();
    };
    fitIfVisible();

    session.attach({
      write: (data, done) => t.write(data, done),
      snapshot: (maxLines = Number.POSITIVE_INFINITY) => {
        const buf = t.buffer.active;
        // Count back from the last line with text (or the cursor), not from the blank
        // rows below it, so "the last N lines" means the last N lines of output.
        let end = Math.max(buf.baseY + buf.cursorY, 0);
        for (let i = buf.length - 1; i > end; i--) {
          if (buf.getLine(i)?.translateToString(true)) {
            end = i;
            break;
          }
        }
        const lines: string[] = [];
        for (let i = Math.max(0, end + 1 - maxLines); i <= end; i++)
          lines.push(buf.getLine(i)?.translateToString(true) ?? "");
        return lines.join("\n").trimEnd();
      },
      find: () => {
        searching = true;
      },
      copy: () => {
        const text = t.getSelection();
        if (text) void navigator.clipboard.writeText(text);
      },
      paste: () => void pasteFromClipboard(),
      // Capped so a huge scrollback can't stall the move to a new window.
      serialize: () => serializer.serialize({ scrollback: 5000 }),
      jumpPrompt: (dir) => {
        const top = t.buffer.active.viewportY;
        const lines = prompts.filter((m) => !m.isDisposed && m.line >= 0).map((m) => m.line);
        const target = dir < 0 ? lines.filter((l) => l < top).at(-1) : lines.find((l) => l > top);
        if (target !== undefined) t.scrollToLine(target);
        else if (dir > 0) t.scrollToBottom();
      },
      hasPrompts: () => prompts.some((m) => !m.isDisposed),
      viewportY: () => t.buffer.active.viewportY,
      scrollToLine: (line) => t.scrollToLine(line),
    });

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
    const blockOutput = (b: Block): string => {
      if (!b.out || b.out.isDisposed || !b.end || b.end.isDisposed) return "";
      const buf = t.buffer.active;
      const lines: string[] = [];
      for (let i = b.out.line; i <= b.end.line; i++) {
        lines.push(buf.getLine(i)?.translateToString(true) ?? "");
      }
      return lines.join("\n").trimEnd();
    };
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

    t.onData((s) => {
      const bytes = encoder.encode(s);
      session.write(bytes);
      broadcast.mirror(session, bytes);
      watchTyped(s);
    });
    t.onBinary((s) => session.write(Uint8Array.from(s, (c) => c.charCodeAt(0) & 0xff)));
    t.onResize(({ cols, rows }) => session.resize(cols, rows));
    t.onBell(bell);
    t.onSelectionChange(() => {
      if (!prefs.current.copyOnSelect) return;
      const text = t.getSelection();
      if (text) void navigator.clipboard.writeText(text);
    });
    // OSC 52: a remote program asking to set the local clipboard (T16).
    t.parser.registerOscHandler(52, (data) => {
      const text = decodeOsc52(data);
      const policy = prefs.current.osc52;
      if (text === null || policy === "deny") return true;
      if (policy === "allow") void navigator.clipboard.writeText(text);
      else pendingClipboard = text;
      return true;
    });

    observer = new ResizeObserver(() => requestAnimationFrame(fitIfVisible));
    observer.observe(host);

    term = t;
    fitter = fit;
    search = finder;
    void session.start(t.cols, t.rows);
  })();

  return () => {
    disposed = true;
    glide?.dispose();
    glide = null;
    observer?.disconnect();
    session.detach();
    instance?.dispose();
    term = null;
    fitter = null;
    search = null;
  };
});

// Font, cursor and scrollback apply live (T10); a new cell size needs a refit.
$effect(() => {
  const o = options(prefs.current);
  if (!term) return;
  Object.assign(term.options, o);
  const fit = fitter;
  requestAnimationFrame(() => {
    if (host && host.clientWidth > 0) fit?.fit();
  });
});

// Colours and the cursor: with glide on, xterm's cursor is made invisible (the character
// under it keeps its colour) and the glide layer draws it instead.
$effect(() => {
  const t = term;
  const p = prefs.current;
  const theme = xtermTheme(p);
  void appearance.current.motion;
  if (!t) return;
  if (p.cursorGlide && motionLevel() === "full") {
    glide ??= attachGlideCursor(t);
    glide.set(p.cursorStyle, p.cursorBlink, theme.cursor ?? theme.foreground ?? INVISIBLE);
    t.options.theme = { ...theme, cursor: INVISIBLE, cursorAccent: theme.foreground };
    t.options.cursorBlink = false;
  } else {
    glide?.dispose();
    glide = null;
    t.options.theme = theme;
    t.options.cursorBlink = p.cursorBlink;
  }
});

$effect(() => {
  if (active && term && !searching) term.focus();
});

const matchColors = $derived.by(() => {
  const tk = appearance.tokens;
  return {
    matchBackground: resolveCssColor(tk.warning, 0.35),
    activeMatchBackground: resolveCssColor(tk.accent, 0.7),
    matchOverviewRuler: resolveCssColor(tk.warning),
    activeMatchColorOverviewRuler: resolveCssColor(tk.accent),
  };
});

function closeSearch() {
  searching = false;
  search?.clearDecorations();
  term?.focus();
}
</script>

<div class="relative h-full w-full" class:flash>
  <div class="h-full w-full" bind:this={host} onpastecapture={onPaste}></div>
  {#if searching && search}
    <SearchBar addon={search} decorations={matchColors} onclose={closeSearch} />
  {/if}
  {#if pendingPaste}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="strip"
      role="alertdialog"
      tabindex="-1"
      aria-label="Confirm paste"
      onkeydown={(e) => onStripKey(e, () => confirmPaste(false))}
    >
      <span class="flex-1 text-sm text-fg">
        Paste {pendingPaste.lines} line{pendingPaste.lines === 1 ? "" : "s"}{session.environment === "prod"
          ? " into a production host"
          : ""}?
        <span class="text-faint">Each line runs as a command.</span>
      </span>
      <button type="button" class="btn-ghost" onclick={() => confirmPaste(false)}>Cancel</button>
      <!-- svelte-ignore a11y_autofocus -->
      <button type="button" class="btn-primary" autofocus onclick={() => confirmPaste(true)}>Paste</button>
    </div>
  {:else if pendingClipboard !== null}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="strip"
      role="alertdialog"
      tabindex="-1"
      aria-label="Allow clipboard write"
      onkeydown={(e) => onStripKey(e, () => confirmClipboard(false))}
    >
      <span class="min-w-0 flex-1 truncate text-sm text-fg">
        The server wants to copy {pendingClipboard.length} characters to your clipboard.
        <span class="text-faint">“{pendingClipboard.slice(0, 40)}{pendingClipboard.length > 40 ? "…" : ""}”</span>
      </span>
      <button type="button" class="btn-ghost" onclick={() => confirmClipboard(false)}>Deny</button>
      <!-- svelte-ignore a11y_autofocus -->
      <button type="button" class="btn-primary" autofocus onclick={() => confirmClipboard(true)}>Allow</button>
    </div>
  {/if}
</div>

<style>
  .strip {
    position: absolute;
    left: 0;
    right: 12px;
    top: 4px;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 8px 14px;
    background: var(--surface-3);
    border: 1px solid var(--border);
    border-left: 2px solid var(--warning);
    border-radius: var(--radius);
  }
  .flash {
    animation: bell 150ms var(--ease-out);
  }
  /* Exit markers on prompt lines (xterm decorations live outside this component). */
  /* The gliding cursor (glidecursor.ts): a layer over the text that slides between cells. */
  :global(.glide-cursor) {
    position: absolute;
    top: 0;
    left: 0;
    z-index: 6;
    width: var(--cw);
    height: var(--ch);
    pointer-events: none;
    will-change: transform;
    transition:
      transform 80ms cubic-bezier(0.2, 0.8, 0.2, 1),
      opacity 120ms ease-out;
  }
  :global(.glide-cursor)::before {
    content: "";
    position: absolute;
    border-radius: 1.5px;
    background: var(--cursor);
    box-shadow: 0 0 10px color-mix(in oklab, var(--cursor) 55%, transparent);
  }
  :global(.glide-cursor[data-shape="bar"])::before {
    top: 1px;
    bottom: 1px;
    left: 0;
    width: 2px;
  }
  :global(.glide-cursor[data-shape="block"])::before {
    inset: 0;
    opacity: 0.5;
    border-radius: 2px;
  }
  :global(.glide-cursor[data-shape="underline"])::before {
    left: 0;
    right: 0;
    bottom: 0;
    height: 2px;
  }
  :global(.glide-cursor.blink:not(.moving))::before {
    animation: glide-blink 1.1s ease-in-out infinite;
  }
  /* An unfocused pane shows a hollow cursor. */
  :global(.glide-cursor.unfocused)::before {
    inset: 0;
    width: auto;
    height: auto;
    opacity: 1;
    background: transparent;
    box-shadow: inset 0 0 0 1px var(--cursor);
    animation: none;
  }
  @keyframes glide-blink {
    50% {
      opacity: 0.12;
    }
  }
  /* Command blocks: a hairline between commands and an exit chip on each prompt line. */
  :global(.hopr-sep) {
    pointer-events: none;
    border-top: 1px solid color-mix(in oklab, var(--text-faint) 28%, transparent);
  }
  :global(.hopr-chip) {
    display: flex;
    align-items: center;
    justify-content: center;
    font: 600 10.5px var(--font-ui);
    color: var(--success);
    background: color-mix(in oklab, var(--success) 15%, transparent);
    border-radius: 999px;
    transform: scale(0.86);
    cursor: pointer;
    pointer-events: auto;
    animation: chip-in 320ms var(--ease-bounce) backwards;
    transition: background var(--dur-fast) var(--ease-out);
  }
  :global(.hopr-chip:not([data-exit="0"])) {
    color: var(--danger);
    background: color-mix(in oklab, var(--danger) 15%, transparent);
  }
  :global(.hopr-chip[data-exit=""]) {
    color: var(--text-muted);
    background: color-mix(in oklab, var(--text-faint) 15%, transparent);
  }
  :global(.hopr-chip:hover) {
    filter: brightness(1.25);
  }
  @keyframes chip-in {
    from {
      opacity: 0;
      transform: scale(0.5);
    }
  }
  :global(.hopr-exit) {
    pointer-events: none;
    border-left: 2px solid var(--success);
    animation: exit-in var(--dur-slow) var(--ease-out) backwards;
  }
  @keyframes exit-in {
    from {
      opacity: 0;
      transform: scaleY(0.4);
    }
  }
  :global(.hopr-exit:not([data-exit="0"])) {
    border-left-color: var(--danger);
  }
  :global(.hopr-exit[data-exit=""]) {
    border-left-color: var(--text-faint);
  }
  @keyframes bell {
    50% {
      filter: brightness(1.35);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .flash {
      animation: none;
    }
  }
</style>
