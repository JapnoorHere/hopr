<script lang="ts">
/**
 * Settings as its own window (not a side drawer): a section list on the left, every
 * section on one scrolling page on the right, and search that filters sections and cards.
 */
import { CaretRight, MagnifyingGlass, X } from "phosphor-svelte";
import { onDestroy, tick } from "svelte";
import ProfilesSettings from "$features/profiles/ProfilesSettings.svelte";
import BackupSettings from "$features/settings/BackupSettings.svelte";
import DesktopSettings from "$features/settings/DesktopSettings.svelte";
import HistorySettings from "$features/settings/HistorySettings.svelte";
import KeyboardSettings from "$features/settings/KeyboardSettings.svelte";
import SyncSettings from "$features/settings/SyncSettings.svelte";
import TerminalSettings from "$features/settings/TerminalSettings.svelte";
import ThemePicker from "$features/settings/ThemePicker.svelte";
import TransfersSettings from "$features/settings/TransfersSettings.svelte";
import WorkspacesSettings from "$features/settings/WorkspacesSettings.svelte";
import SecuritySettings from "$features/vault/SecuritySettings.svelte";
import { appearance } from "$lib/design/appearance.svelte";
import { DENSITIES, type Density, MOTIONS } from "$lib/design/theme";
import { PageFinder } from "$lib/find-in-page";
import { i18n, LOCALES, type LocalePref, t } from "$lib/i18n/i18n.svelte";
import { sink, slidingPill, springIn } from "$lib/motion";
import { motion, motionLevel } from "$lib/platform";
import Modal from "$lib/ui/Modal.svelte";
import Segmented from "$lib/ui/Segmented.svelte";
import { SECTIONS, type SectionId } from "./settings-sections";
import { shell } from "./shell.state.svelte";

const densityOptions = $derived(
  (Object.keys(DENSITIES) as Density[]).map((d) => ({ id: d, label: t(`density.${d}`) })),
);
const motionOptions = $derived(MOTIONS.map((m) => ({ id: m, label: t(`motion.${m}`) })));

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
    void tick().then(() => findAll(q));
  });
});

// Find in page: every match is highlighted; Enter / Shift+Enter step through them.
const finder = new PageFinder("settings");
let found = $state({ at: -1, count: 0 });

function findAll(q: string) {
  finder.find(content, q, "[hidden], select, .bar");
  found = { at: finder.at, count: finder.count };
}

function step(by: number) {
  const r = finder.step(by)?.getBoundingClientRect();
  found = { at: finder.at, count: finder.count };
  if (!r || !content) return;
  const box = content.getBoundingClientRect();
  if (r.top < box.top + 24 || r.bottom > box.bottom - 24) {
    content.scrollBy({
      top: r.top - box.top - box.height / 3,
      behavior: motionLevel() === "full" ? "smooth" : "auto",
    });
  }
}

function onSearchKey(e: KeyboardEvent) {
  if (e.key === "Enter") {
    e.preventDefault();
    step(e.shiftKey ? -1 : 1);
  }
}

onDestroy(() => finder.clear());

/**
 * A jump from the list is under way: the clicked item stays lit while the page scrolls past
 * the sections in between. Cleared when the scroll settles, or as soon as you scroll yourself.
 */
let jumping = false;
let jumpTimer: ReturnType<typeof setTimeout> | undefined;

function go(id: SectionId) {
  current = id;
  jumping = true;
  clearTimeout(jumpTimer);
  // Fallback in case "scrollend" never fires (already in place, or nothing to scroll).
  jumpTimer = setTimeout(() => (jumping = false), 1200);
  content
    ?.querySelector(`[data-section="${id}"]`)
    ?.scrollIntoView({ behavior: motionLevel() === "full" ? "smooth" : "auto", block: "start" });
}

/** Light up the section being read in the list. */
function spy() {
  if (!content || jumping) return;
  const top = content.getBoundingClientRect().top + 72;
  let at: SectionId = SECTIONS[0].id;
  for (const s of SECTIONS) {
    const el = content.querySelector<HTMLElement>(`[data-section="${s.id}"]`);
    if (el && !hiddenSections[s.id] && el.getBoundingClientRect().top <= top) at = s.id;
  }
  current = at;
}

function onKey(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
    e.preventDefault();
    searchBox?.focus();
  }
}

/** Esc clears a search before it closes Settings. */
function clearSearch(): boolean {
  if (!query) return false;
  query = "";
  return true;
}

$effect(() => {
  searchBox?.focus();
});

// Opened for one section (Manage profiles…): go straight there.
$effect(() => {
  const want = shell.settingsSection;
  if (!want || !content) return;
  shell.settingsSection = null;
  const id = SECTIONS.find((x) => x.id === want)?.id;
  if (id) void tick().then(() => go(id));
});
</script>

<Modal label={t("settings.title")} onclose={() => shell.close()} onescape={clearSearch} layer={45}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="page"
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
          onkeydown={onSearchKey}
        />
        {#if query}
          <span class="count" title="Enter for next, Shift+Enter for previous">
            {found.count ? `${found.at + 1 || "–"}/${found.count}` : "0"}
          </span>
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

    <div class="main">
    <!-- A fixed bar over the scrolling cards: where you are, and the way out. -->
    <header class="bar">
      <span class="text-faint">{t("settings.title")}</span>
      <CaretRight size={11} class="text-faint" />
      <span class="truncate text-fg">{SECTIONS.find((x) => x.id === current)?.label}</span>
      <span class="flex-1"></span>
      <span class="mr-1 text-xs text-faint">Esc to close</span>
      <button type="button" class="close" aria-label="Close settings" title="Close (Esc)" onclick={() => shell.close()}>
        <X size={15} />
      </button>
    </header>
    <div
      class="content"
      bind:this={content}
      onscroll={spy}
      onscrollend={() => (jumping = false)}
      onwheel={() => (jumping = false)}
      onpointerdown={() => (jumping = false)}
    >

      <section data-section="appearance" hidden={hiddenSections.appearance}>
        <h2 class="title">Appearance</h2>
        <p class="lead">How Hopr looks and moves.</p>
        <ThemePicker />
        <div class="group">
          <h3 class="label-caps mb-2 text-faint">{t("settings.density")}</h3>
          <div class="max-w-[420px]">
            <Segmented
              label="Density"
              options={densityOptions}
              value={appearance.current.density}
              onchange={(density) => appearance.update({ density })}
            />
          </div>
        </div>
        <div class="group">
          <h3 class="label-caps mb-2 text-faint">{t("settings.motion")}</h3>
          <div class="max-w-[420px]">
            <Segmented
              label="Motion"
              options={motionOptions}
              value={appearance.current.motion}
              onchange={(motion) => appearance.update({ motion })}
            />
          </div>
          <p class="mt-2 text-xs text-muted">{t("settings.motionHint")}</p>
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

      <section data-section="profiles" hidden={hiddenSections.profiles}>
        <h2 class="title">Profiles</h2>
        <p class="lead">Separate vaults on this computer, and which one opens at start.</p>
        <ProfilesSettings />
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
</Modal>

<style>
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
  .count {
    flex-shrink: 0;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
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
  .main {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
  }
  .bar {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 6px;
    height: 48px;
    padding: 0 10px 0 36px;
    font-size: 12.5px;
    border-bottom: 1px solid var(--border);
  }
  .content {
    position: relative;
    min-height: 0;
    flex: 1;
    padding: 24px 36px 48px;
    overflow-y: auto;
    scroll-padding-top: 20px;
  }
  .close {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
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
  @media (max-width: 760px) {
    .page {
      grid-template-columns: 1fr;
    }
    .nav {
      display: none;
    }
  }
</style>
