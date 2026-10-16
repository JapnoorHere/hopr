<script lang="ts">
import { appearance } from "$lib/design/appearance.svelte";
import { DENSITIES, type Density, THEMES } from "$lib/design/theme";
import Drawer from "$lib/ui/Drawer.svelte";
import { shell } from "./shell.state.svelte";

const densities = Object.keys(DENSITIES) as Density[];
</script>

<Drawer
  title="Settings"
  pinned={shell.pinned}
  onclose={() => shell.close()}
  onpin={() => shell.togglePin()}
>
  <section class="mt-2">
    <h3 class="label-caps mb-2 text-faint">Theme</h3>
    <div class="grid grid-cols-2 gap-2" role="radiogroup" aria-label="Theme">
      {#each THEMES as t (t.id)}
        <button
          type="button"
          role="radio"
          aria-checked={appearance.current.theme === t.id}
          data-theme={t.id}
          class="card"
          class:selected={appearance.current.theme === t.id}
          onclick={() => appearance.update({ theme: t.id })}
        >
          <span class="flex gap-1">
            <span class="h-6 flex-1 rounded-sm bg-surface-2"></span>
            <span class="h-6 w-2 rounded-sm bg-accent"></span>
          </span>
          <span class="mt-2 block text-sm font-medium text-fg">{t.name}</span>
          <span class="block text-xs text-muted">{t.feel}</span>
        </button>
      {/each}
    </div>
  </section>

  <section class="mt-5">
    <h3 class="label-caps mb-2 text-faint">Density</h3>
    <div class="flex rounded-md border border-border bg-surface-1 p-0.5" role="radiogroup" aria-label="Density">
      {#each densities as d (d)}
        <button
          type="button"
          role="radio"
          aria-checked={appearance.current.density === d}
          class="seg"
          class:on={appearance.current.density === d}
          onclick={() => appearance.update({ density: d })}
        >
          {d[0]?.toUpperCase()}{d.slice(1)}
        </button>
      {/each}
    </div>
  </section>
</Drawer>

<style>
  .card {
    padding: 8px;
    text-align: left;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    transition: border-color var(--dur-fast) var(--ease-out);
  }
  .card:hover {
    border-color: var(--border-strong);
  }
  .card.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .seg {
    flex: 1;
    padding: 4px 0;
    font-size: 12px;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
  }
  .seg.on {
    color: var(--text);
    background: var(--surface-3);
  }
</style>
