<script lang="ts">
import { toast } from "$lib/ui/toast.svelte";
import "@xterm/xterm/css/xterm.css";
import { Terminal } from "@xterm/xterm";
import { Pause, Play, X } from "phosphor-svelte";
import { prefs } from "$features/terminal/prefs.svelte";
import { wrap } from "$features/vault/vault.svelte";
import { appearance } from "$lib/design/appearance.svelte";
import { terminalTheme } from "$lib/design/palettes";
import { commands } from "$lib/ipc";
import { type Cast, type CastEvent, findText, outputUntil, parseCast, squeezeIdle } from "./cast";
import { history } from "./history.svelte";

let { name }: { name: string } = $props();

const SPEEDS = [0.5, 1, 2, 4, 8];
/** "Skip idle" caps pauses at this many seconds. */
const MAX_IDLE = 1;

let host: HTMLDivElement;
let root: HTMLElement;
let cast = $state.raw<Cast | null>(null);
let error = $state<string | null>(null);
let notice = $state<string | null>(null);
let playing = $state(false);
let speed = $state(1);
let skipIdle = $state(true);
let pos = $state(0);
let query = $state("");
let term: Terminal | null = null;
let next = 0;
let frame = 0;
let last = 0;

const events = $derived<CastEvent[]>(
  cast ? (skipIdle ? squeezeIdle(cast.events, MAX_IDLE) : cast.events) : [],
);
const duration = $derived(events.at(-1)?.[0] ?? 0);

function clock(s: number): string {
  const t = Math.floor(s);
  return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, "0")}`;
}

function size() {
  return { cols: cast?.header.width ?? 80, rows: cast?.header.height ?? 24 };
}

/** Write every event up to `t` that hasn't been written yet. */
function writeUntil(t: number) {
  if (!term) return;
  let chunk = "";
  while (next < events.length) {
    const e = events[next];
    if (!e || e[0] > t) break;
    if (e[1] === "o") chunk += e[2];
    else if (e[1] === "r") {
      const m = /^(\d+)x(\d+)$/.exec(e[2]);
      if (m) {
        if (chunk) term.write(chunk);
        chunk = "";
        term.resize(Number(m[1]), Number(m[2]));
      }
    }
    next++;
  }
  if (chunk) term.write(chunk);
}

function seek(t: number) {
  if (!term) return;
  const at = outputUntil(events, t, size());
  term.reset();
  term.resize(at.cols, at.rows);
  term.write(at.text);
  next = at.next;
  pos = Math.min(Math.max(0, t), duration);
}

function step(now: number) {
  const dt = (now - last) / 1000;
  last = now;
  pos = Math.min(duration, pos + dt * speed);
  writeUntil(pos);
  if (pos >= duration) {
    playing = false;
    return;
  }
  frame = requestAnimationFrame(step);
}

function play() {
  if (!cast) return;
  if (pos >= duration) seek(0);
  playing = true;
  last = performance.now();
  cancelAnimationFrame(frame);
  frame = requestAnimationFrame(step);
}

function pause() {
  playing = false;
  cancelAnimationFrame(frame);
}

function toggle() {
  if (playing) pause();
  else play();
}

function scrub(e: Event) {
  seek(Number((e.currentTarget as HTMLInputElement).value));
}

function setSkipIdle(on: boolean) {
  // Keep the same place in the recording on the new timeline.
  const index = next;
  skipIdle = on;
  pos = events[index - 1]?.[0] ?? 0;
}

function search(e: SubmitEvent) {
  e.preventDefault();
  const t = findText(events, query.trim(), pos);
  if (t === null) {
    notice = `“${query.trim()}” isn't in this recording`;
    return;
  }
  notice = null;
  pause();
  seek(t);
}

/** The whole recording as plain text (what the screen and scrollback would hold). */
function plainText(): Promise<string> {
  const c = cast;
  if (!c) return Promise.resolve("");
  const all = outputUntil(c.events, Number.POSITIVE_INFINITY, size());
  const headless = new Terminal({
    cols: all.cols,
    rows: all.rows,
    scrollback: 100_000,
    allowProposedApi: true,
  });
  return new Promise((resolve) => {
    headless.write(all.text, () => {
      const buf = headless.buffer.active;
      const lines: string[] = [];
      for (let i = 0; i < buf.length; i++)
        lines.push(buf.getLine(i)?.translateToString(true) ?? "");
      headless.dispose();
      resolve(`${lines.join("\n").trimEnd()}\n`);
    });
  });
}

async function exportAs(kind: "cast" | "text") {
  const text = kind === "text" ? await plainText() : null;
  const r = wrap(await commands.recordingExport(name, text));
  if (!r.ok) toast.error(r.error.message);
  else if (r.data) toast.success(`Saved to ${r.data}`);
}

function close() {
  pause();
  history.replaying = null;
}

function onKey(e: KeyboardEvent) {
  if ((e.target as HTMLElement).closest("input, select")) return;
  // Escape is handled by the app (it closes the replay before any drawer).
  if (e.key === " ") {
    e.preventDefault();
    toggle();
  } else if (e.key === "ArrowRight") seek(pos + 5);
  else if (e.key === "ArrowLeft") seek(pos - 5);
}

$effect(() => {
  let disposed = false;
  (async () => {
    const r = wrap(await commands.recordingRead(name));
    if (disposed) return;
    if (!r.ok) {
      error = r.error.message;
      return;
    }
    try {
      cast = parseCast(r.data);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      return;
    }
    const p = prefs.current;
    term = new Terminal({
      cols: cast.header.width,
      rows: cast.header.height,
      fontFamily: `"${p.fontFamily}", "Commit Mono", ui-monospace, monospace`,
      fontSize: p.fontSize,
      lineHeight: p.lineHeight,
      scrollback: 10_000,
      disableStdin: true,
      cursorBlink: false,
      allowProposedApi: true,
      theme: terminalTheme(appearance.current.theme),
    });
    term.open(host);
    // Take focus from the terminal underneath, so keys (Space, Esc) reach the player.
    root.focus();
    history.playerText = () => {
      const buf = term?.buffer.active;
      if (!buf) return "";
      const lines: string[] = [];
      for (let i = 0; i < buf.length; i++)
        lines.push(buf.getLine(i)?.translateToString(true) ?? "");
      return lines.join("\n").trimEnd();
    };
    seek(0);
    play();
  })();
  return () => {
    disposed = true;
    cancelAnimationFrame(frame);
    term?.dispose();
    term = null;
    history.playerText = null;
    history.player = null;
  };
});

// Mirror the player for the dev E2E hooks.
$effect(() => {
  history.player = { pos, duration, playing, speed };
});
</script>

<svelte:window onkeydown={onKey} />

<section class="replay" aria-label="Replay" tabindex="-1" bind:this={root}>
  <header class="flex items-center gap-2 border-b border-border px-4 py-2">
    <h2 class="min-w-0 flex-1 truncate text-sm font-medium">
      Replay · {cast?.header.title ?? name}
    </h2>
    <button type="button" class="btn-ghost" disabled={!cast} onclick={() => exportAs("cast")}>Export .cast</button>
    <button type="button" class="btn-ghost" disabled={!cast} onclick={() => exportAs("text")}>Export text</button>
    <button type="button" class="tool" aria-label="Close replay" onclick={close}><X size={15} /></button>
  </header>
  {#if error}<p class="px-4 py-2 text-sm text-danger" role="alert">{error}</p>{/if}
  <div class="screen"><div bind:this={host}></div></div>
  <footer class="flex flex-wrap items-center gap-3 border-t border-border px-4 py-2">
    <button type="button" class="tool" aria-label={playing ? "Pause" : "Play"} disabled={!cast} onclick={toggle}>
      {#if playing}<Pause size={15} weight="fill" />{:else}<Play size={15} weight="fill" />{/if}
    </button>
    <span class="tabular w-24 text-xs text-muted" aria-label="Time">{clock(pos)} / {clock(duration)}</span>
    <input
      class="min-w-40 flex-1"
      type="range"
      min="0"
      max={duration}
      step="any"
      value={pos}
      oninput={scrub}
      aria-label="Position"
    />
    <select class="input w-20" bind:value={speed} aria-label="Speed">
      {#each SPEEDS as s (s)}<option value={s}>{s}×</option>{/each}
    </select>
    <label class="flex items-center gap-1 text-xs text-muted">
      <input type="checkbox" checked={skipIdle} onchange={(e) => setSkipIdle(e.currentTarget.checked)} />
      Skip idle
    </label>
    <form class="flex items-center gap-1" onsubmit={search}>
      <input class="input w-44" bind:value={query} placeholder="Find in recording" aria-label="Search recording" />
    </form>
    {#if notice}<span class="text-xs text-muted" role="status">{notice}</span>{/if}
  </footer>
</section>

<style>
  .replay {
    position: absolute;
    inset: 0;
    z-index: 16;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .screen {
    min-height: 0;
    flex: 1;
    overflow: auto;
    padding: 12px 16px;
  }
  .tool {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    color: var(--text-muted);
    border-radius: 4px;
  }
  .tool:hover:not(:disabled) {
    color: var(--text);
    background: var(--surface-3);
  }
</style>
