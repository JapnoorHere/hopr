<script lang="ts">
import BackupSettings from "$features/settings/BackupSettings.svelte";
import DesktopSettings from "$features/settings/DesktopSettings.svelte";
import HistorySettings from "$features/settings/HistorySettings.svelte";
import KeyboardSettings from "$features/settings/KeyboardSettings.svelte";
import SyncSettings from "$features/settings/SyncSettings.svelte";
import TerminalSettings from "$features/settings/TerminalSettings.svelte";
import TransfersSettings from "$features/settings/TransfersSettings.svelte";
import WorkspacesSettings from "$features/settings/WorkspacesSettings.svelte";
import SecuritySettings from "$features/vault/SecuritySettings.svelte";
import { appearance } from "$lib/design/appearance.svelte";
import { BACKDROPS, DENSITIES, type Density, MOTIONS, THEMES } from "$lib/design/theme";
import { i18n, LOCALES, type LocalePref, t } from "$lib/i18n/i18n.svelte";
import { slidingPill } from "$lib/motion";
import Drawer from "$lib/ui/Drawer.svelte";
import { shell } from "./shell.state.svelte";

const densities = Object.keys(DENSITIES) as Density[];
</script>

<Drawer
  title={t("settings.title")}
  pinned={shell.pinned}
  onclose={() => shell.close()}
  onpin={() => shell.togglePin()}
>
  <section class="mt-2">
    <h3 class="label-caps mb-2 text-faint">{t("settings.theme")}</h3>
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
    <h3 class="label-caps mb-2 text-faint">{t("settings.density")}</h3>
    <div class="flex rounded-md border border-border bg-surface-1 p-0.5" role="radiogroup" aria-label="Density" use:slidingPill>
      {#each densities as d (d)}
        <button
          type="button"
          role="radio"
          aria-checked={appearance.current.density === d}
          class="seg"
          class:on={appearance.current.density === d}
          onclick={() => appearance.update({ density: d })}
        >
          {t(`density.${d}`)}
        </button>
      {/each}
    </div>
  </section>

  <section class="mt-5">
    <h3 class="label-caps mb-2 text-faint">{t("settings.motion")}</h3>
    <div class="flex rounded-md border border-border bg-surface-1 p-0.5" role="radiogroup" aria-label="Motion" use:slidingPill>
      {#each MOTIONS as m (m)}
        <button
          type="button"
          role="radio"
          aria-checked={appearance.current.motion === m}
          class="seg"
          class:on={appearance.current.motion === m}
          onclick={() => appearance.update({ motion: m })}
        >
          {t(`motion.${m}`)}
        </button>
      {/each}
    </div>
    <p class="mt-1 text-xs text-muted">{t("settings.motionHint")}</p>
  </section>

  <section class="mt-5">
    <h3 class="label-caps mb-2 text-faint">{t("settings.backdrop")}</h3>
    <div
      class="flex rounded-md border border-border bg-surface-1 p-0.5"
      role="radiogroup"
      aria-label="Window backdrop"
      use:slidingPill
    >
      {#each BACKDROPS as b (b)}
        <button
          type="button"
          role="radio"
          aria-checked={appearance.current.backdrop === b}
          class="seg"
          class:on={appearance.current.backdrop === b}
          onclick={() => appearance.update({ backdrop: b })}
        >
          {t(`backdrop.${b}`)}
        </button>
      {/each}
    </div>
    <p class="mt-1 text-xs text-muted">{t("settings.backdropHint")}</p>
  </section>

  <section class="mt-5">
    <h3 class="label-caps mb-2 text-faint">{t("settings.language")}</h3>
    <select
      class="input w-full"
      value={i18n.pref}
      aria-label="Language"
      onchange={(e) => i18n.set(e.currentTarget.value as LocalePref)}
    >
      <option value="system">{t("settings.languageSystem", { name: LOCALES[i18n.systemLocale].name })}</option>
      {#each Object.entries(LOCALES) as [code, l] (code)}<option value={code}>{l.name}</option>{/each}
    </select>
    <p class="mt-1 text-xs text-muted">{t("settings.languageHint")}</p>
  </section>

  <TerminalSettings />
  <WorkspacesSettings />
  <KeyboardSettings />
  <DesktopSettings />
  <TransfersSettings />
  <HistorySettings />
  <SyncSettings />
  <SecuritySettings />
  <BackupSettings />
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
