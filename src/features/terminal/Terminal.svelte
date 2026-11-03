<script lang="ts">
import "@xterm/xterm/css/xterm.css";
import { openUrl } from "@tauri-apps/plugin-opener";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { SerializeAddon } from "@xterm/addon-serialize";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { type ITerminalOptions, Terminal } from "@xterm/xterm";
import { untrack } from "svelte";
import { broadcast } from "$features/broadcast/broadcast.svelte";
import { appearance } from "$lib/design/appearance.svelte";
import { INVISIBLE, resolveCssColor, terminalTheme } from "$lib/design/palettes";
import { terminalColors } from "$lib/design/themes";
import { inTauri } from "$lib/ipc";
import { isMac, motionLevel } from "$lib/platform";
import { shortcuts } from "$lib/shortcuts";
import { toast } from "$lib/ui/toast.svelte";
import { lastLines } from "./buffer";
import { attachGlideCursor, type GlideCursor } from "./glidecursor";
import { attachShellIntegration } from "./integration";
import { checkPaste, smartKey } from "./paste";
import { decodeOsc52, type TerminalPrefs } from "./prefs";
import { prefs } from "./prefs.svelte";
import SearchBar from "./SearchBar.svelte";
import type { TerminalSession } from "./terminal-session.svelte";

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
  return terminalTheme(terminalColors(appearance.spec, p.bgLift));
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

    const shell = attachShellIntegration(t, host, session);
    session.attach({
      write: (data, done) => t.write(data, done),
      snapshot: (maxLines) => lastLines(t, maxLines),
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
      jumpPrompt: shell.jumpPrompt,
      hasPrompts: shell.hasPrompts,
      viewportY: () => t.buffer.active.viewportY,
      scrollToLine: (line) => t.scrollToLine(line),
    });

    t.onData((s) => {
      const bytes = encoder.encode(s);
      session.write(bytes);
      broadcast.mirror(session, bytes);
      shell.typed(s);
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
