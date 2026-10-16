<script lang="ts">
import "@xterm/xterm/css/xterm.css";
import { FitAddon } from "@xterm/addon-fit";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import { appearance } from "$lib/design/appearance.svelte";
import { terminalTheme } from "$lib/design/palettes";
import type { TerminalSession } from "./session.svelte";

let { session, active }: { session: TerminalSession; active: boolean } = $props();

let host: HTMLDivElement;
let term = $state.raw<Terminal | null>(null);
const encoder = new TextEncoder();

$effect(() => {
  let disposed = false;
  let observer: ResizeObserver | undefined;
  let instance: Terminal | undefined;

  (async () => {
    // xterm measures glyphs on open; measuring before the font loads gives a wrong grid.
    await document.fonts.load('14px "Commit Mono"').catch(() => undefined);
    if (disposed) return;

    const t = new Terminal({
      fontFamily: '"Commit Mono", "Geist Mono Variable", ui-monospace, monospace',
      fontSize: 14,
      lineHeight: 1.2,
      cursorBlink: true,
      cursorStyle: "bar",
      scrollback: 10_000,
      allowProposedApi: true,
      macOptionIsMeta: true,
      theme: terminalTheme(appearance.current.theme),
    });
    instance = t;
    const fit = new FitAddon();
    t.loadAddon(fit);
    t.loadAddon(new Unicode11Addon());
    t.unicode.activeVersion = "11";
    t.open(host);
    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => webgl.dispose()); // falls back to the DOM renderer
      t.loadAddon(webgl);
    } catch {
      // No WebGL2: the DOM renderer keeps working.
    }

    const fitIfVisible = () => {
      if (host.clientWidth > 0 && host.clientHeight > 0) fit.fit();
    };
    fitIfVisible();

    session.attach({
      write: (data, done) => t.write(data, done),
      snapshot: () => {
        const buf = t.buffer.active;
        const lines: string[] = [];
        for (let i = 0; i < buf.length; i++)
          lines.push(buf.getLine(i)?.translateToString(true) ?? "");
        return lines.join("\n").trimEnd();
      },
    });
    t.onData((s) => session.write(encoder.encode(s)));
    t.onBinary((s) => session.write(Uint8Array.from(s, (c) => c.charCodeAt(0) & 0xff)));
    t.onResize(({ cols, rows }) => session.resize(cols, rows));

    observer = new ResizeObserver(() => requestAnimationFrame(fitIfVisible));
    observer.observe(host);

    term = t;
    void session.start(t.cols, t.rows);
  })();

  return () => {
    disposed = true;
    observer?.disconnect();
    session.detach();
    instance?.dispose();
    term = null;
  };
});

$effect(() => {
  const theme = appearance.current.theme;
  if (term) term.options.theme = terminalTheme(theme);
});

$effect(() => {
  if (active && term) term.focus();
});
</script>

<div class="h-full w-full" bind:this={host}></div>
