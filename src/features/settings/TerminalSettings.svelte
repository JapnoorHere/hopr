<script lang="ts">
import { BG_OPACITY, FONT_SIZE, FONTS, SCROLLBACK } from "$features/terminal/prefs";
import { prefs } from "$features/terminal/prefs.svelte";
import { appearance } from "$lib/design/appearance.svelte";
import { terminalColors } from "$lib/design/themes";

const p = $derived(prefs.current);
const pv = $derived(terminalColors(appearance.spec, { lift: p.bgLift, opacity: p.bgOpacity }));
const solid = $derived(appearance.current.backdrop === "solid");

function num(e: Event): number {
  return Number((e.currentTarget as HTMLInputElement).value);
}
// Select values are validated by parsePrefs, so any string is safe to pass through.
function str(e: Event): never {
  return (e.currentTarget as HTMLSelectElement).value as never;
}
function on(e: Event): boolean {
  return (e.currentTarget as HTMLInputElement).checked;
}
</script>

<div class="group flex flex-col gap-3">
  <h3 class="label-caps text-faint">Colours</h3>
  <p class="-mt-1 text-xs text-muted">The terminal uses your theme's colours. Pick or make one in Appearance.</p>
  <!-- Live preview: the theme's colours, font and cursor on a small sample session. -->
  <div
    class="preview"
    aria-hidden="true"
    style:background={pv.background}
    style:color={pv.foreground}
    style:font-family={`"${p.fontFamily}", monospace`}
    style:font-size="{Math.max(10, p.fontSize - 2)}px"
    style:line-height={p.lineHeight}
  >
    <div><span style:color={pv.ansi.green}>dev@api</span> <span style:color={pv.ansi.blue}>~/app</span> ❯ git status</div>
    <div style:color={pv.ansi.yellow}>  modified: src/server.ts</div>
    <div><span style:color={pv.ansi.red}>error</span>: tests failed (2)</div>
    <div style:color={pv.ansi.cyan}>  https://docs.example.com</div>
    <div>
      <span style:color={pv.ansi.green}>dev@api</span> <span style:color={pv.ansi.blue}>~/app</span> ❯
      <span class="pcursor" data-shape={p.cursorStyle} style:background={pv.cursor}></span>
    </div>
  </div>
</div>

<div class="group flex flex-col gap-3">
  <h3 class="label-caps text-faint">Background</h3>
  <div class="grid grid-cols-2 gap-3">
    <label class="field">
      <span class="text-xs text-muted">Brightness ({p.bgLift ? `+${p.bgLift}%` : "theme's own"})</span>
      <input type="range" min="0" max="100" step="5" value={p.bgLift} oninput={(e) => prefs.update({ bgLift: num(e) })} />
    </label>
    <label class="field">
      <span class="text-xs text-muted">Opacity ({p.bgOpacity}%)</span>
      <input
        type="range"
        min={BG_OPACITY.min}
        max={BG_OPACITY.max}
        step="5"
        value={p.bgOpacity}
        oninput={(e) => prefs.update({ bgOpacity: num(e) })}
      />
    </label>
  </div>
  {#if p.bgOpacity < 100 && solid}
    <div class="hint">
      <span class="flex-1">See-through shows the window backdrop, which is solid right now.</span>
      <button type="button" class="pill" onclick={() => appearance.update({ backdrop: "acrylic" })}>Use Acrylic</button>
      <button type="button" class="pill" onclick={() => appearance.update({ backdrop: "mica" })}>Use Mica</button>
    </div>
  {:else}
    <p class="-mt-1 text-xs text-faint">
      Brightness lifts a very dark background. Below 100% opacity the window backdrop ({appearance.current.backdrop}) shows through.
    </p>
  {/if}
</div>

<div class="group flex flex-col gap-3">
  <h3 class="label-caps text-faint">Text</h3>
  <label class="field">
    <span class="text-xs text-muted">Font</span>
    <select class="input" value={p.fontFamily} onchange={(e) => prefs.update({ fontFamily: str(e) })}>
      {#each FONTS as f (f)}<option value={f}>{f.replace(" Variable", "")}</option>{/each}
    </select>
  </label>
  <div class="grid grid-cols-2 gap-2">
    <label class="field">
      <span class="text-xs text-muted">Size ({p.fontSize}px)</span>
      <input type="range" min={FONT_SIZE.min} max={FONT_SIZE.max} value={p.fontSize} oninput={(e) => prefs.update({ fontSize: num(e) })} />
    </label>
    <label class="field">
      <span class="text-xs text-muted">Line height ({p.lineHeight})</span>
      <input type="range" min="1" max="2" step="0.05" value={p.lineHeight} oninput={(e) => prefs.update({ lineHeight: num(e) })} />
    </label>
  </div>
  <label class="field">
    <span class="text-xs text-muted">Scrollback lines</span>
    <input class="input tabular" type="number" min={SCROLLBACK.min} max={SCROLLBACK.max} step="1000" value={p.scrollback} onchange={(e) => prefs.update({ scrollback: num(e) })} />
  </label>
</div>

<div class="group flex flex-col gap-3">
  <h3 class="label-caps text-faint">Cursor</h3>
  <label class="field">
      <span class="text-xs text-muted">Shape</span>
      <select class="input" value={p.cursorStyle} onchange={(e) => prefs.update({ cursorStyle: str(e) })}>
        <option value="bar">Bar</option>
        <option value="block">Block</option>
        <option value="underline">Underline</option>
      </select>
    </label>
  <label class="check"><input type="checkbox" checked={p.cursorBlink} onchange={(e) => prefs.update({ cursorBlink: on(e) })} /> Blinking cursor</label>
  <label class="check">
    <input type="checkbox" checked={p.cursorGlide} onchange={(e) => prefs.update({ cursorGlide: on(e) })} />
    Cursor glide
  </label>
</div>

<div class="group flex flex-col gap-3">
  <h3 class="label-caps text-faint">Command blocks</h3>
  <label class="field">
    <span class="text-xs text-muted">Separators and exit chips between commands (needs shell integration)</span>
    <select class="input" value={p.blocks} onchange={(e) => prefs.update({ blocks: str(e) })}>
      <option value="full">Full: separators, exit chips, pinned command</option>
      <option value="separators">Separators only</option>
      <option value="off">Off</option>
    </select>
  </label>
</div>

<div class="group flex flex-col gap-3">
  <h3 class="label-caps text-faint">Copy &amp; paste</h3>
  <label class="check"><input type="checkbox" checked={p.copyOnSelect} onchange={(e) => prefs.update({ copyOnSelect: on(e) })} /> Copy on select</label>
  <label class="check">
    <input type="checkbox" checked={p.smartCopyPaste} onchange={(e) => prefs.update({ smartCopyPaste: on(e) })} />
    Smart Ctrl+C / Ctrl+V
  </label>
  <p class="-mt-1 pl-6 text-xs text-faint">
    Ctrl+C copies selected text, and interrupts when nothing is selected. Ctrl+V pastes. Ctrl+Shift+C / V always work.
  </p>
  <label class="check">
    <input type="checkbox" checked={p.rightClickCopyPaste} onchange={(e) => prefs.update({ rightClickCopyPaste: on(e) })} />
    Right-click copies or pastes
  </label>
  <label class="field">
      <span class="text-xs text-muted">Server clipboard writes (OSC 52)</span>
      <select class="input" value={p.osc52} onchange={(e) => prefs.update({ osc52: str(e) })}>
        <option value="ask">Ask</option>
        <option value="allow">Allow</option>
        <option value="deny">Block</option>
      </select>
  </label>
</div>

<div class="group flex flex-col gap-3">
  <h3 class="label-caps text-faint">Bell</h3>
    <label class="field">
      <span class="text-xs text-muted">When a program rings the bell</span>
      <select class="input" value={p.bell} onchange={(e) => prefs.update({ bell: str(e) })}>
        <option value="visual">Flash</option>
        <option value="sound">Sound</option>
        <option value="none">Off</option>
      </select>
    </label>
</div>

<style>
  .hint {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    font-size: 12px;
    color: var(--text-muted);
    background: color-mix(in oklab, var(--accent) 8%, transparent);
    border: 1px solid color-mix(in oklab, var(--accent) 25%, var(--border));
    border-radius: var(--radius);
  }
  .pill {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
    color: var(--accent-fg);
    background: var(--accent);
    border-radius: 999px;
    transition: filter var(--dur-fast) var(--ease-out);
  }
  .pill:hover {
    filter: brightness(1.08);
  }
  .preview {
    padding: 10px 12px;
    overflow: hidden;
    white-space: pre;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    transition:
      background var(--dur-slow) var(--ease-out),
      color var(--dur-slow) var(--ease-out);
  }
  .pcursor {
    display: inline-block;
    width: 2px;
    height: 1.1em;
    vertical-align: text-bottom;
    border-radius: 1px;
    animation: pcursor-blink 1.1s ease-in-out infinite;
  }
  .pcursor[data-shape="block"] {
    width: 0.6em;
    opacity: 0.55;
  }
  .pcursor[data-shape="underline"] {
    width: 0.6em;
    height: 2px;
    vertical-align: baseline;
  }
  @keyframes pcursor-blink {
    50% {
      opacity: 0.15;
    }
  }
</style>
