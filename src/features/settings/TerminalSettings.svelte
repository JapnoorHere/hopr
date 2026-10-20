<script lang="ts">
import { FONT_SIZE, FONTS, SCROLLBACK } from "$features/terminal/prefs";
import { prefs } from "$features/terminal/prefs.svelte";

const p = $derived(prefs.current);

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
