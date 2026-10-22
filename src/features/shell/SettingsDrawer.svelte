<script lang="ts">
/**
 * Settings as its own window (not a side drawer): a section list on the left, every
 * section on one scrolling page on the right, and search that filters sections and cards.
 */
import {
  AppWindow,
  Archive,
  ArrowsClockwise,
  ClockCounterClockwise,
  FolderSimple,
  Keyboard,
  MagnifyingGlass,
  Palette,
  ShieldCheck,
  SquaresFour,
  TerminalWindow,
  X,
} from "phosphor-svelte";
import { tick } from "svelte";
import { fade } from "svelte/transition";
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
import { sink, slidingPill, springIn } from "$lib/motion";
import { motion, motionLevel } from "$lib/platform";
import { shell } from "./shell.state.svelte";

const densities = Object.keys(DENSITIES) as Density[];

/** Sections, in order. `keywords` lets search find a section by words it doesn't show. */
const SECTIONS = [
  {
    id: "appearance",
    label: "Appearance",
    icon: Palette,
    keywords: "theme dark light colour color density motion animation mica backdrop language",
  },
  {
    id: "terminal",
    label: "Terminal",
    icon: TerminalWindow,
    keywords: "font size cursor colours palette scrollback copy paste bell blocks",
  },
  {
    id: "keyboard",
    label: "Keyboard",
    icon: Keyboard,
    keywords: "shortcuts keys bindings hotkeys",
  },
  {
    id: "workspaces",
    label: "Workspaces",
    icon: SquaresFour,
    keywords: "layout tabs restore session",
  },
  {
    id: "transfers",
    label: "Files & transfers",
    icon: FolderSimple,
    keywords: "sftp upload download speed limit parallel",
  },
  {
    id: "history",
    label: "History",
    icon: ClockCounterClockwise,
    keywords: "commands recording shell integration exclude",
  },
  {
    id: "sync",
    label: "Sync",
    icon: ArrowsClockwise,
    keywords: "devices folder onedrive dropbox passphrase",
  },
  {
    id: "security",
    label: "Security",
    icon: ShieldCheck,
    keywords: "vault lock password clipboard windows hello biometric audit",
  },
  {
    id: "backups",
    label: "Backups",
    icon: Archive,
    keywords: "restore export import settings file",
  },
  {
    id: "app",
    label: "App & updates",
    icon: AppWindow,
    keywords: "tray startup quick terminal hotkey update version about",
  },
] as const;
type SectionId = (typeof SECTIONS)[number]["id"];

let query = $state("");
let current = $state<SectionId>("appearance");
let hiddenSections = $state<Partial<Record<SectionId, boolean>>>({});
let content = $state<HTMLDivElement>();
let searchBox = $state<HTMLInputElement>();
const nothing = $derived(SECTIONS.every((s) => hiddenSections[s.id]));

// Search: a section whose name or keywords match shows whole; otherwise only its cards
// whose text matches stay visible.
$effect(() => {
  const q = query.trim().toLowerCase();
  void tick().then(() => {
    const next: Partial<Record<SectionId, boolean>> = {};
    for (const s of SECTIONS) {
      const el = content?.querySelector<HTMLElement>(`[data-section="${s.id}"]`);
      if (!el) continue;
      const whole = !q || `${s.label} ${s.keywords}`.toLowerCase().includes(q);
      let any = false;
      for (const g of el.querySelectorAll<HTMLElement>(".group")) {
        const hit = whole || (g.textContent ?? "").toLowerCase().includes(q);
        g.hidden = !hit;
        any ||= hit;
      }
      next[s.id] = !(whole || any);
    }
    hiddenSections = next;
  });
});

function go(id: SectionId) {
  current = id;
  content
    ?.querySelector(`[data-section="${id}"]`)
    ?.scrollIntoView({ behavior: motionLevel() === "full" ? "smooth" : "auto", block: "start" });
}

/** Light up the section being read in the list. */
function spy() {
  if (!content) return;
  const top = content.getBoundingClientRect().top + 72;
  let at: SectionId = SECTIONS[0].id;
  for (const s of SECTIONS) {
    const el = content.querySelector<HTMLElement>(`[data-section="${s.id}"]`);
    if (el && !hiddenSections[s.id] && el.getBoundingClientRect().top <= top) at = s.id;
  }
  current = at;
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.preventDefault();
    if (query) query = "";
    else shell.close();
  } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
    e.preventDefault();
    searchBox?.focus();
  }
}

$effect(() => {
  searchBox?.focus();
});
</script>

<svelte:window onkeydowncapture={(e) => e.key === "Escape" && onKey(e)} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" transition:fade={{ duration: motion(160) }} onclick={() => shell.close()}>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <div
    class="page"
    role="dialog"
    aria-modal="true"
    aria-label={t("settings.title")}
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={onKey}
    in:springIn={{ y: 14, scale: 0.97 }}
    out:sink={{ y: 8, scale: 0.98, duration: 140 }}
  >
    <nav class="nav" aria-label="Settings sections">
      <h2 class="px-2 pb-3 text-base font-semibold text-fg">{t("settings.title")}</h2>
      <label class="search">
        <MagnifyingGlass size={14} class="shrink-0 text-faint" />
        <input
          bind:this={searchBox}
          bind:value={query}
          placeholder="Search settings"
          aria-label="Search settings"
          spellcheck="false"
        />
        {#if query}
          <button type="button" class="clear" aria-label="Clear search" onclick={() => (query = "")}>
            <X size={12} />
          </button>
        {/if}
      </label>
      <div class="mt-2 flex flex-col gap-0.5">
        {#each SECTIONS as s (s.id)}
          {@const Icon = s.icon}
          <button
            type="button"
            class="link"
            class:on={current === s.id}
            class:dim={hiddenSections[s.id]}
            disabled={hiddenSections[s.id]}
            onclick={() => go(s.id)}
          >
            <Icon size={16} weight={current === s.id ? "fill" : "regular"} />
            <span class="truncate">{s.label}</span>
          </button>
        {/each}
      </div>
    </nav>

    <div class="content" bind:this={content} onscroll={spy}>
      <button type="button" class="close" aria-label="Close settings" title="Close (Esc)" onclick={() => shell.close()}>
        <X size={16} />
      </button>

      <section data-section="appearance" hidden={hiddenSections.appearance}>
        <h2 class="title">Appearance</h2>
        <p class="lead">How Hopr looks and moves.</p>
        <div class="group">
          <h3 class="label-caps mb-2 text-faint">{t("settings.theme")}</h3>
          <div class="grid grid-cols-2 gap-2 sm:grid-cols-4" role="radiogroup" aria-label="Theme">
            {#each THEMES as th (th.id)}
              <button
                type="button"
                role="radio"
                aria-checked={appearance.current.theme === th.id}
                data-theme={th.id}
                class="card"
                class:selected={appearance.current.theme === th.id}
                onclick={() => appearance.update({ theme: th.id })}
              >
                <span class="flex gap-1">
                  <span class="h-6 flex-1 rounded-sm bg-surface-2"></span>
                  <span class="h-6 w-2 rounded-sm bg-accent"></span>
                </span>
                <span class="mt-2 block text-sm font-medium text-fg">{th.name}</span>
                <span class="block text-xs text-muted">{th.feel}</span>
              </button>
            {/each}
          </div>
        </div>
        <div class="group">
          <h3 class="label-caps mb-2 text-faint">{t("settings.density")}</h3>
          <div class="seg-row" role="radiogroup" aria-label="Density" use:slidingPill>
            {#each densities as d (d)}
              <button
                type="button"
                role="radio"
                aria-checked={appearance.current.density === d}
                class="seg"
                class:on={appearance.current.density === d}
                onclick={() => appearance.update({ density: d })}>{t(`density.${d}`)}</button
              >
            {/each}
          </div>
        </div>
        <div class="group">
          <h3 class="label-caps mb-2 text-faint">{t("settings.motion")}</h3>
          <div class="seg-row" role="radiogroup" aria-label="Motion" use:slidingPill>
            {#each MOTIONS as m (m)}
              <button
                type="button"
                role="radio"
                aria-checked={appearance.current.motion === m}
                class="seg"
                class:on={appearance.current.motion === m}
                onclick={() => appearance.update({ motion: m })}>{t(`motion.${m}`)}</button
              >
            {/each}
          </div>
          <p class="mt-2 text-xs text-muted">{t("settings.motionHint")}</p>
        </div>
        <div class="group">
          <h3 class="label-caps mb-2 text-faint">{t("settings.backdrop")}</h3>
          <div class="seg-row" role="radiogroup" aria-label="Window backdrop" use:slidingPill>
            {#each BACKDROPS as b (b)}
              <button
                type="button"
                role="radio"
                aria-checked={appearance.current.backdrop === b}
                class="seg"
                class:on={appearance.current.backdrop === b}
                onclick={() => appearance.update({ backdrop: b })}>{t(`backdrop.${b}`)}</button
              >
            {/each}
          </div>
          <p class="mt-2 text-xs text-muted">{t("settings.backdropHint")}</p>
        </div>
        <div class="group">
          <h3 class="label-caps mb-2 text-faint">{t("settings.language")}</h3>
          <select
            class="input w-full max-w-sm"
            value={i18n.pref}
            aria-label="Language"
            onchange={(e) => i18n.set(e.currentTarget.value as LocalePref)}
          >
            <option value="system">{t("settings.languageSystem", { name: LOCALES[i18n.systemLocale].name })}</option>
            {#each Object.entries(LOCALES) as [code, l] (code)}<option value={code}>{l.name}</option>{/each}
          </select>
          <p class="mt-2 text-xs text-muted">{t("settings.languageHint")}</p>
        </div>
      </section>

      <section data-section="terminal" hidden={hiddenSections.terminal}>
        <h2 class="title">Terminal</h2>
        <p class="lead">Colours, type, cursor and how copy and paste behave.</p>
        <TerminalSettings />
      </section>
      <section data-section="keyboard" hidden={hiddenSections.keyboard}>
        <h2 class="title">Keyboard</h2>
        <p class="lead">Shortcuts for everything in Hopr.</p>
        <div class="group"><KeyboardSettings /></div>
      </section>
      <section data-section="workspaces" hidden={hiddenSections.workspaces}>
        <h2 class="title">Workspaces</h2>
        <p class="lead">Saved layouts of tabs and splits.</p>
        <div class="group"><WorkspacesSettings /></div>
      </section>
      <section data-section="transfers" hidden={hiddenSections.transfers}>
        <h2 class="title">Files &amp; transfers</h2>
        <p class="lead">How many files move at once, and how fast.</p>
        <div class="group"><TransfersSettings /></div>
      </section>
      <section data-section="history" hidden={hiddenSections.history}>
        <h2 class="title">History</h2>
        <p class="lead">Command history, recordings and shell integration.</p>
        <div class="group"><HistorySettings /></div>
      </section>
      <section data-section="sync" hidden={hiddenSections.sync}>
        <h2 class="title">Sync</h2>
        <p class="lead">Keep your hosts and keys the same on every computer.</p>
        <div class="group"><SyncSettings /></div>
      </section>
      <section data-section="security" hidden={hiddenSections.security}>
        <h2 class="title">Security</h2>
        <p class="lead">The vault, auto-lock and clipboard safety.</p>
        <div class="group"><SecuritySettings /></div>
      </section>
      <section data-section="backups" hidden={hiddenSections.backups}>
        <h2 class="title">Backups</h2>
        <p class="lead">Encrypted backups, and moving settings between computers.</p>
        <div class="group"><BackupSettings /></div>
      </section>
      <section data-section="app" hidden={hiddenSections.app}>
        <h2 class="title">App &amp; updates</h2>
        <p class="lead">Tray, quick terminal and updates.</p>
        <div class="group"><DesktopSettings /></div>
      </section>

      {#if nothing}
        <p class="py-16 text-center text-sm text-muted">No settings match “{query}”.</p>
      {/if}
    </div>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 45;
    display: grid;
    place-items: center;
    padding: 24px;
    background: color-mix(in oklab, var(--bg) 55%, transparent);
    backdrop-filter: blur(3px);
  }
  .page {
    display: grid;
    grid-template-columns: 230px minmax(0, 1fr);
    width: min(1040px, 100%);
    height: min(760px, 100%);
    overflow: hidden;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-pop);
    outline: none;
  }
  .nav {
    display: flex;
    flex-direction: column;
    padding: 18px 12px;
    overflow-y: auto;
    background: var(--surface-1);
    border-right: 1px solid var(--border);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    transition: border-color var(--dur-fast) var(--ease-out);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  .search input {
    min-width: 0;
    flex: 1;
    font-size: 13px;
    color: var(--text);
    background: transparent;
    outline: none;
  }
  .search input::placeholder {
    color: var(--text-faint);
  }
  .clear {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    color: var(--text-faint);
    border-radius: 999px;
  }
  .clear:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .link {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    font-size: 13px;
    text-align: left;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
    transition:
      background var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out),
      opacity var(--dur) var(--ease-out);
  }
  .link:hover:not(:disabled) {
    color: var(--text);
    background: var(--surface-2);
  }
  .link.on {
    color: var(--text);
    background: color-mix(in oklab, var(--accent) 13%, transparent);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .link.dim {
    opacity: 0.35;
  }
  .content {
    position: relative;
    padding: 28px 36px 48px;
    overflow-y: auto;
    scroll-padding-top: 20px;
  }
  .close {
    position: sticky;
    top: 0;
    float: right;
    z-index: 2;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    margin: -12px -20px 0 0;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
  }
  .close:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  section + section {
    margin-top: 36px;
  }
  .title {
    font-size: 18px;
    font-weight: 600;
    color: var(--text);
  }
  .lead {
    margin: 2px 0 14px;
    font-size: 13px;
    color: var(--text-muted);
  }
  /* Each topic is a card; the pieces inside keep their own small headings. */
  .content :global(.group) {
    padding: 16px 18px;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .content :global(.group + .group) {
    margin-top: 10px;
  }
  .group :global(> section),
  .group :global(> section:first-child) {
    margin-top: 0;
  }
  .card {
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
  .seg-row {
    display: flex;
    max-width: 420px;
    padding: 2px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .seg {
    flex: 1;
    padding: 5px 0;
    font-size: 12px;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
  }
  .seg.on {
    color: var(--text);
    background: var(--surface-3);
  }
  @media (max-width: 760px) {
    .page {
      grid-template-columns: 1fr;
    }
    .nav {
      display: none;
    }
  }
</style>
