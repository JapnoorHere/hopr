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
import { broadcast } from "$features/broadcast/broadcast.svelte";
import { appearance } from "$lib/design/appearance.svelte";
import { resolveCssColor, terminalTheme } from "$lib/design/palettes";
import { inTauri } from "$lib/ipc";
import { isMac } from "$lib/platform";
import { shortcuts } from "$lib/shortcuts";
import { checkPaste } from "./paste";
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
const encoder = new TextEncoder();

function options(p: TerminalPrefs): ITerminalOptions {
  return {
    fontFamily: `"${p.fontFamily}", "Commit Mono", ui-monospace, monospace`,
    fontSize: p.fontSize,
    lineHeight: p.lineHeight,
    cursorStyle: p.cursorStyle,
    cursorBlink: p.cursorBlink,
    scrollback: p.scrollback,
  };
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
    const p = prefs.current;
    await document.fonts.load(`${p.fontSize}px "${p.fontFamily}"`).catch(() => undefined);
    if (disposed) return;

    const t = new Terminal({
      ...options(p),
      allowProposedApi: true,
      macOptionIsMeta: true,
      theme: terminalTheme(appearance.current.theme),
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
    t.attachCustomKeyEventHandler((e) => {
      if (releasing && e.type === "keyup") return false;
      return !(e.type === "keydown" && shortcuts.find(e));
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
    });

    // Shell integration (OSC 133/633/7): commands with exit codes, cwd, prompt marks.
    type Spot = { marker: IMarker; col: number };
    const prompts: IMarker[] = [];
    let inputAt: Spot | null = null;
    let promptAt: IMarker | null = null;
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
    };
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
      if (m.kind === "exec") inputAt = null;
      if (done) {
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
    observer?.disconnect();
    session.detach();
    instance?.dispose();
    term = null;
    fitter = null;
    search = null;
  };
});

$effect(() => {
  const theme = appearance.current.theme;
  if (term) term.options.theme = terminalTheme(theme);
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

$effect(() => {
  if (active && term && !searching) term.focus();
});

const matchColors = $derived.by(() => {
  void appearance.current.theme;
  const css = getComputedStyle(document.documentElement);
  return {
    matchBackground: resolveCssColor(css.getPropertyValue("--warning"), 0.35),
    activeMatchBackground: resolveCssColor(css.getPropertyValue("--accent"), 0.7),
    matchOverviewRuler: resolveCssColor(css.getPropertyValue("--warning")),
    activeMatchColorOverviewRuler: resolveCssColor(css.getPropertyValue("--accent")),
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
  :global(.hopr-exit) {
    pointer-events: none;
    border-left: 2px solid var(--success);
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
