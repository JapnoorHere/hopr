<script lang="ts">
import { FONT_SIZE, FONTS, SCROLLBACK } from "$features/terminal/prefs";
import { prefs } from "$features/terminal/prefs.svelte";
import { appearance } from "$lib/design/appearance.svelte";
import { PALETTE_IDS, resolvePalette } from "$lib/design/palettes";

const p = $derived(prefs.current);
const pv = $derived(resolvePalette(appearance.current.theme, p.palette));

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

<section class="mt-5 flex flex-col gap-3">
  <h3 class="label-caps text-faint">Terminal</h3>
  <!-- Live preview: the chosen colours, font and cursor on a small sample session. -->
  <div
    class="preview"
    aria-hidden="true"
    style:background={pv.background ?? "var(--bg)"}
    style:color={pv.foreground ?? "var(--text)"}
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
      <span class="pcursor" data-shape={p.cursorStyle} style:background={pv.cursor ?? "var(--accent)"}></span>
    </div>
  </div>
  <div class="field">
    <span class="text-xs text-muted">Colours</span>
    <div class="palettes" role="radiogroup" aria-label="Terminal colours">
      {#each PALETTE_IDS as id (id)}
        {@const pal = resolvePalette(appearance.current.theme, id)}
        <button
          type="button"
          role="radio"
          aria-checked={p.palette === id}
          class="pal"
          class:on={p.palette === id}
          onclick={() => prefs.update({ palette: id })}
          title={pal.feel}
        >
          <span class="strip" style:background={pal.background ?? "var(--bg)"}>
            {#each [pal.ansi.red, pal.ansi.yellow, pal.ansi.green, pal.ansi.cyan, pal.ansi.blue, pal.ansi.magenta] as c, i (i)}
              <span class="chip" style:background={c}></span>
            {/each}
          </span>
          <span class="truncate text-xs text-fg">{id === "theme" ? "Match app theme" : pal.name}</span>
        </button>
      {/each}
    </div>
  </div>
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
  <div class="grid grid-cols-2 gap-2">
    <label class="field">
      <span class="text-xs text-muted">Cursor</span>
      <select class="input" value={p.cursorStyle} onchange={(e) => prefs.update({ cursorStyle: str(e) })}>
        <option value="bar">Bar</option>
        <option value="block">Block</option>
        <option value="underline">Underline</option>
      </select>
    </label>
    <label class="field">
      <span class="text-xs text-muted">Scrollback lines</span>
      <input class="input tabular" type="number" min={SCROLLBACK.min} max={SCROLLBACK.max} step="1000" value={p.scrollback} onchange={(e) => prefs.update({ scrollback: num(e) })} />
    </label>
  </div>
  <label class="check"><input type="checkbox" checked={p.cursorBlink} onchange={(e) => prefs.update({ cursorBlink: on(e) })} /> Blinking cursor</label>
  <label class="check">
    <input type="checkbox" checked={p.cursorGlide} onchange={(e) => prefs.update({ cursorGlide: on(e) })} />
    Cursor glide
  </label>
  <label class="field">
    <span class="text-xs text-muted">Command blocks (needs shell integration)</span>
    <select class="input" value={p.blocks} onchange={(e) => prefs.update({ blocks: str(e) })}>
      <option value="full">Full: separators, exit chips, pinned command</option>
      <option value="separators">Separators only</option>
      <option value="off">Off</option>
    </select>
  </label>
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
  <div class="grid grid-cols-2 gap-2">
    <label class="field">
      <span class="text-xs text-muted">Bell</span>
      <select class="input" value={p.bell} onchange={(e) => prefs.update({ bell: str(e) })}>
        <option value="visual">Flash</option>
        <option value="sound">Sound</option>
        <option value="none">Off</option>
      </select>
    </label>
    <label class="field">
      <span class="text-xs text-muted">Server clipboard writes</span>
      <select class="input" value={p.osc52} onchange={(e) => prefs.update({ osc52: str(e) })}>
        <option value="ask">Ask</option>
        <option value="allow">Allow</option>
        <option value="deny">Block</option>
      </select>
    </label>
  </div>
</section>

<style>
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
  .palettes {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
  }
  .pal {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 5px;
    text-align: left;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    transition:
      border-color var(--dur-fast) var(--ease-out),
      transform var(--dur-instant) var(--ease-out);
  }
  .pal:hover {
    border-color: var(--border-strong);
  }
  .pal:active {
    transform: scale(0.97);
  }
  .pal.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .strip {
    display: flex;
    gap: 3px;
    padding: 6px;
    border-radius: 4px;
  }
  .chip {
    width: 8px;
    height: 8px;
    border-radius: 999px;
  }
</style>
