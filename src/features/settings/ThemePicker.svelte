<script lang="ts">
/**
 * Theme gallery and the custom theme editor. A theme colours the whole app and its
 * terminals; the custom theme starts from any built-in one and every colour can change.
 */
import { ArrowCounterClockwise, ClipboardText, Copy, PaintBrush } from "phosphor-svelte";
import { appearance } from "$lib/design/appearance.svelte";
import type { Ansi16 } from "$lib/design/palettes";
import {
  ANSI_KEYS,
  type CustomTheme,
  customFrom,
  customSpec,
  isDark,
  isThemeId,
  normalizeHex,
  parseCustom,
  THEMES,
  type ThemeId,
  type ThemeSpec,
  themeStyle,
} from "$lib/design/themes";
import { toast } from "$lib/ui/toast.svelte";

const DARK = THEMES.filter((th) => isDark(th));
const LIGHT = THEMES.filter((th) => !isDark(th));
const cur = $derived(appearance.current);
/** The theme the custom one starts from when there is none yet. */
const startId = $derived<ThemeId>(isThemeId(cur.theme) ? cur.theme : "ember");
const custom = $derived<CustomTheme>(cur.custom ?? customFrom(startId));
const editing = $derived(cur.theme === "custom");

const INTERFACE = [
  { key: "bg", label: "Background" },
  { key: "text", label: "Text" },
  { key: "accent", label: "Accent" },
] as const;
const ANSI_NAMES = ["Black", "Red", "Green", "Yellow", "Blue", "Magenta", "Cyan", "White"];

function useCustom() {
  appearance.update({ theme: "custom", custom });
}

function set(patch: Partial<CustomTheme>) {
  appearance.update({ theme: "custom", custom: { ...custom, ...patch } });
}

function setAnsi(k: keyof Ansi16, v: string) {
  set({ ansi: { ...custom.ansi, [k]: v } });
}

function startFrom(e: Event) {
  const id = (e.currentTarget as HTMLSelectElement).value;
  if (isThemeId(id)) appearance.update({ theme: "custom", custom: customFrom(id) });
}

/** The hex field: takes six hex digits, with or without the hash; anything else snaps back. */
function hexInput(e: Event, apply: (v: string) => void) {
  const el = e.currentTarget as HTMLInputElement;
  const v = normalizeHex(el.value);
  if (v) apply(v);
  else el.value = el.defaultValue;
}

async function copyJson() {
  await navigator.clipboard.writeText(JSON.stringify(custom, null, 2));
  toast.success("Theme copied as JSON");
}

async function pasteJson() {
  try {
    const parsed = parseCustom(JSON.parse(await navigator.clipboard.readText()));
    if (!parsed) throw new Error("not a theme");
    appearance.update({ theme: "custom", custom: parsed });
    toast.success("Theme pasted");
  } catch {
    toast.error("The clipboard doesn't hold a Hopr theme (JSON with bg, text, accent and ansi)");
  }
}

const dots = (s: ThemeSpec) => [
  s.ansi.red,
  s.ansi.yellow,
  s.ansi.green,
  s.ansi.cyan,
  s.ansi.blue,
  s.ansi.magenta,
];
</script>

{#snippet card(th: ThemeSpec, selected: boolean, onpick: () => void)}
  <button
    type="button"
    role="radio"
    aria-checked={selected}
    class="card"
    class:selected
    style={themeStyle(th)}
    onclick={onpick}
  >
    <span class="flex gap-1">
      <span class="flex h-6 flex-1 items-center gap-0.5 rounded-sm bg-surface-2 px-1.5">
        {#each dots(th) as c, i (i)}<span class="dot" style:background={c}></span>{/each}
      </span>
      <span class="h-6 w-2 rounded-sm bg-accent"></span>
    </span>
    <span class="mt-2 block truncate text-sm font-medium text-fg">{th.name}</span>
    <span class="block truncate text-xs text-muted">{th.feel}</span>
  </button>
{/snippet}

<div class="group">
  <h3 class="label-caps mb-1 text-faint">Theme</h3>
  <p class="mb-3 text-xs text-muted">Colours the whole app and every terminal.</p>
  <div role="radiogroup" aria-label="Theme" class="flex flex-col gap-3">
    <div>
      <p class="sub">Dark</p>
      <div class="grid grid-cols-2 gap-2 sm:grid-cols-4">
        {#each DARK as th (th.id)}
          {@render card(th, cur.theme === th.id, () => appearance.update({ theme: th.id }))}
        {/each}
      </div>
    </div>
    <div>
      <p class="sub">Light</p>
      <div class="grid grid-cols-2 gap-2 sm:grid-cols-4">
        {#each LIGHT as th (th.id)}
          {@render card(th, cur.theme === th.id, () => appearance.update({ theme: th.id }))}
        {/each}
      </div>
    </div>
    <div>
      <p class="sub">Your own</p>
      <div class="grid grid-cols-2 gap-2 sm:grid-cols-4">
        {#if cur.custom}
          {@render card(customSpec(cur.custom), editing, useCustom)}
        {:else}
          <button type="button" role="radio" aria-checked="false" class="card new" onclick={useCustom}>
            <PaintBrush size={18} class="text-accent" />
            <span class="mt-2 block text-sm font-medium text-fg">Custom</span>
            <span class="block text-xs text-muted">Start from {THEMES.find((x) => x.id === startId)?.name}</span>
          </button>
        {/if}
      </div>
    </div>
  </div>
</div>

{#if editing}
  <div class="group">
    <div class="mb-3 flex flex-wrap items-center gap-2">
      <h3 class="label-caps flex-1 text-faint">Custom theme</h3>
      <button type="button" class="btn-ghost" onclick={copyJson}><Copy size={14} /> Copy JSON</button>
      <button type="button" class="btn-ghost" onclick={pasteJson}><ClipboardText size={14} /> Paste JSON</button>
    </div>
    <div class="flex flex-col gap-4">
      <label class="field">
        <span class="text-xs text-muted">Start from</span>
        <div class="flex gap-2">
          <select class="input flex-1" value="" onchange={startFrom} aria-label="Start from a theme">
            <option value="" disabled>Pick a theme to copy its colours…</option>
            {#each THEMES as th (th.id)}<option value={th.id}>{th.name}</option>{/each}
          </select>
          <button
            type="button"
            class="btn-ghost"
            title="Reset to {THEMES.find((x) => x.id === startId)?.name}"
            onclick={() => set(customFrom(startId))}
          >
            <ArrowCounterClockwise size={14} /> Reset
          </button>
        </div>
      </label>

      <div>
        <p class="sub">Interface</p>
        <div class="grid grid-cols-1 gap-2 sm:grid-cols-3">
          {#each INTERFACE as f (f.key)}
            <div class="colour">
              <input
                type="color"
                class="swatch"
                aria-label={f.label}
                value={custom[f.key]}
                oninput={(e) => set({ [f.key]: (e.currentTarget as HTMLInputElement).value })}
              />
              <span class="min-w-0 flex-1">
                <span class="block text-xs text-muted">{f.label}</span>
                {#key custom[f.key]}
                  <input
                    class="hex"
                    value={custom[f.key]}
                    spellcheck="false"
                    aria-label="{f.label} hex"
                    onchange={(e) => hexInput(e, (v) => set({ [f.key]: v }))}
                  />
                {/key}
              </span>
            </div>
          {/each}
        </div>
        <p class="mt-2 text-xs text-faint">
          Light or dark follows the background. Surfaces, borders and status colours are worked out from these.
        </p>
      </div>

      <div>
        <p class="sub">Terminal colours</p>
        <div class="ansi">
          {#each [0, 8] as offset (offset)}
            <span class="row-label">{offset ? "Bright" : "Normal"}</span>
            {#each ANSI_KEYS.slice(offset, offset + 8) as k, i (k)}
              <label class="ansi-cell" title="{offset ? 'Bright ' : ''}{ANSI_NAMES[i]} · {custom.ansi[k]}">
                <input
                  type="color"
                  class="swatch"
                  aria-label="{offset ? 'Bright ' : ''}{ANSI_NAMES[i]}"
                  value={custom.ansi[k]}
                  oninput={(e) => setAnsi(k, (e.currentTarget as HTMLInputElement).value)}
                />
                <span class="text-[10.5px] text-faint">{ANSI_NAMES[i]}</span>
              </label>
            {/each}
          {/each}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .sub {
    margin-bottom: 6px;
    font-size: 11.5px;
    color: var(--text-muted);
  }
  .card {
    min-width: 0;
    padding: 8px;
    text-align: left;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    transition:
      border-color var(--dur-fast) var(--ease-out),
      transform var(--dur-instant) var(--ease-out);
  }
  .card:hover {
    border-color: var(--border-strong);
  }
  .card:active {
    transform: scale(0.98);
  }
  .card.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .card.new {
    border-style: dashed;
    background: transparent;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 999px;
  }
  .btn-ghost {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 10px;
    font-size: 12.5px;
    color: var(--text-muted);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    transition:
      color var(--dur-fast) var(--ease-out),
      background var(--dur-fast) var(--ease-out);
  }
  .btn-ghost:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .colour {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .hex {
    width: 100%;
    font-family: var(--font-mono);
    font-size: 12.5px;
    color: var(--text);
    background: transparent;
    outline: none;
  }
  /* Colour inputs drawn as rounded swatches; clicking opens the colour picker. */
  .swatch {
    flex-shrink: 0;
    width: 28px;
    height: 28px;
    padding: 0;
    cursor: pointer;
    background: none;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    appearance: none;
    transition: transform var(--dur-fast) var(--ease-spring);
  }
  .swatch:hover {
    transform: scale(1.08);
  }
  .swatch:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .swatch::-webkit-color-swatch-wrapper {
    padding: 0;
  }
  .swatch::-webkit-color-swatch {
    border: none;
    border-radius: calc(var(--radius-sm) - 1px);
  }
  .ansi {
    display: grid;
    grid-template-columns: auto repeat(8, minmax(0, 1fr));
    gap: 8px 6px;
    align-items: center;
  }
  .row-label {
    padding-right: 6px;
    font-size: 11.5px;
    color: var(--text-muted);
  }
  .ansi-cell {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    min-width: 0;
  }
  .ansi-cell .swatch {
    width: 100%;
    max-width: 40px;
    height: 26px;
  }
</style>
