<script lang="ts">
import { BookmarkSimple, Copy, FilmStrip, Play, Trash } from "phosphor-svelte";
import { library } from "$features/hosts/library.svelte";
import { formatSize } from "$features/sftp/format";
import { shell } from "$features/shell/shell.state.svelte";
import { snippets } from "$features/snippets/snippets.svelte";
import { t } from "$lib/i18n/i18n.svelte";
import type { ConnectionRecord } from "$lib/ipc";
import { slidingPill } from "$lib/motion";
import Drawer from "$lib/ui/Drawer.svelte";
import { type HistoryTab, history } from "./history.svelte";

const TABS: HistoryTab[] = ["commands", "connections", "recordings"];

let confirmClear = $state(false);
let searchTimer: ReturnType<typeof setTimeout> | undefined;

$effect(() => {
  void history.refresh();
});

function search(e: Event) {
  history.query = (e.currentTarget as HTMLInputElement).value;
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => void history.refresh(), 150);
}

function pickHost(e: Event) {
  const v = (e.currentTarget as HTMLSelectElement).value;
  history.host = v === "*" ? null : v;
  void history.refresh();
}

function when(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
}

function duration(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  return m < 60 ? `${m}m ${s % 60}s` : `${Math.floor(m / 60)}h ${m % 60}m`;
}

function span(c: ConnectionRecord): number | null {
  if (!c.endedAt) return null;
  return Math.max(0, Date.parse(c.endedAt) - Date.parse(c.startedAt));
}

const REASONS: Record<string, string> = {
  user_closed: "Closed",
  remote_exited: "Shell exited",
  connection_lost: "Connection lost",
  failed: "Failed",
  unknown: "Hopr quit",
};

const uptime = $derived.by(() => {
  const list = history.connections;
  const total = list.reduce((sum, c) => sum + (span(c) ?? 0), 0);
  const lost = list.filter((c) => c.endReason === "connection_lost").length;
  return { count: list.length, total, lost };
});
</script>

<Drawer
  title={t("history.title")}
  width="480px"
  onclose={() => shell.close()}
>
  <div class="flex flex-col gap-2 pt-1">
    <div class="tabs" role="tablist" aria-label="History" use:slidingPill>
      {#each TABS as tab (tab)}
        <button
          type="button"
          role="tab"
          class="tab"
          aria-selected={history.tab === tab}
          onclick={() => history.show(tab)}>{t(`history.${tab}`)}</button
        >
      {/each}
    </div>

    {#if history.tab !== "recordings"}
      <div class="flex gap-2">
        {#if history.tab === "commands"}
          <input
            class="input min-w-0 flex-1"
            value={history.query}
            oninput={search}
            placeholder={t("history.search")}
            aria-label="Search history"
            spellcheck="false"
          />
        {/if}
        <select
          class="input"
          class:flex-1={history.tab === "connections"}
          class:w-40={history.tab === "commands"}
          value={history.host ?? "*"}
          onchange={pickHost}
          aria-label="Host filter"
        >
          <option value="*">{t("history.allHosts")}</option>
          {#if history.tab === "commands"}<option value="">Quick connect &amp; local</option>{/if}
          {#each library.hosts as h (h.id)}<option value={h.id}>{h.label}</option>{/each}
        </select>
      </div>
    {/if}

    {#if history.error}<p class="text-xs text-danger" role="alert">{history.error}</p>{/if}

    {#if history.tab === "commands"}
      {#if history.commands.length === 0}
        <p class="py-6 text-center text-sm text-muted">
          {history.query ? "No commands match." : "Commands you run show up here."}
          <span class="mt-1 block text-xs text-faint">
            Exact with shell integration (Settings → History), read from typing otherwise.
          </span>
        </p>
      {/if}
      <ul class="flex flex-col" aria-label="Commands">
        {#each history.commands as c (c.id)}
          <li class="row" data-command={c.command}>
            <div class="min-w-0 flex-1">
              <code class="block truncate font-mono text-sm text-fg" title={c.command}>{c.command}</code>
              <span class="block truncate text-xs text-muted">
                {c.hostLabel}{#if c.cwd} · {c.cwd}{/if} · {when(c.startedAt)}{#if c.durationMs !== null} · {duration(
                    c.durationMs,
                  )}{/if}
                {#if !c.exact}<span class="text-faint" title="Read from typing, not shell integration"> · approx.</span>{/if}
              </span>
            </div>
            {#if c.exitCode !== null}
              <span class="exit" class:bad={c.exitCode !== 0} title="Exit code">{c.exitCode}</span>
            {/if}
            <button type="button" class="act" aria-label="Run again" title="Run again" onclick={() => history.runAgain(c)}>
              <Play size={14} />
            </button>
            <button
              type="button"
              class="act"
              aria-label="Save as snippet"
              title="Save as snippet"
              onclick={() => {
                snippets.newFrom(c.command);
                shell.drawer = "snippets";
              }}
            >
              <BookmarkSimple size={14} />
            </button>
            <button type="button" class="act" aria-label="Copy command" title="Copy" onclick={() => history.copy(c)}>
              <Copy size={14} />
            </button>
            <button type="button" class="act" aria-label="Forget command" title="Forget" onclick={() => history.remove(c)}>
              <Trash size={14} />
            </button>
          </li>
        {/each}
      </ul>
      <div class="mt-2 flex items-center gap-2">
        {#if confirmClear}
          <span class="flex-1 text-xs text-fg">Forget every command and connection?</span>
          <button type="button" class="btn-ghost" onclick={() => (confirmClear = false)}>Cancel</button>
          <button
            type="button"
            class="btn-danger"
            onclick={async () => {
              confirmClear = false;
              await history.clear();
            }}>Clear history</button
          >
        {:else}
          <span class="flex-1"></span>
          <button type="button" class="btn-ghost" onclick={() => (confirmClear = true)}>{t("history.clear")}</button>
        {/if}
      </div>
    {:else if history.tab === "connections"}
      <p class="text-xs text-muted" aria-label="Uptime summary">
        {uptime.count} connection{uptime.count === 1 ? "" : "s"} · {duration(uptime.total)} connected{#if uptime.lost} ·
          <span class="text-warning">{uptime.lost} dropped</span>{/if}
      </p>
      <ul class="flex flex-col" aria-label="Connections">
        {#each history.connections as c (c.id)}
          {@const ms = span(c)}
          <li class="row" data-connection={c.hostLabel}>
            <span class="dot" class:lost={c.endReason === "connection_lost"} class:open={!c.endedAt}></span>
            <div class="min-w-0 flex-1">
              <span class="block truncate text-sm text-fg">{c.hostLabel}</span>
              <span class="block truncate text-xs text-muted">
                {when(c.startedAt)} · {ms === null ? "open" : duration(ms)} · {REASONS[c.endReason ?? ""] ?? "Connected"}
                · ↓{formatSize(c.bytesIn)} ↑{formatSize(c.bytesOut)}
              </span>
            </div>
            <span class="text-xs text-faint">{c.kind}</span>
          </li>
        {/each}
      </ul>
      {#if history.connections.length === 0}<p class="py-6 text-center text-sm text-muted">No connections yet.</p>{/if}
    {:else}
      <p class="text-xs text-muted">
        Record a tab from the palette (Record this session) or record every session in Settings → History. Output only:
        what you type, including passwords, is never recorded.
      </p>
      <ul class="flex flex-col" aria-label="Recordings">
        {#each history.recordings as r (r.name)}
          <li class="row" data-recording={r.name}>
            <FilmStrip size={16} class="text-faint" />
            <div class="min-w-0 flex-1">
              <span class="block truncate text-sm text-fg">{r.title}</span>
              <span class="block truncate text-xs text-muted">
                {when(r.startedAt)} · {duration(r.duration * 1000)} · {formatSize(r.size)}
              </span>
            </div>
            <button type="button" class="act" aria-label="Replay" title="Replay" onclick={() => (history.replaying = r.name)}>
              <Play size={14} />
            </button>
            <button
              type="button"
              class="act"
              aria-label="Delete recording"
              title="Delete"
              onclick={() => history.deleteRecording(r.name)}
            >
              <Trash size={14} />
            </button>
          </li>
        {/each}
      </ul>
      {#if history.recordings.length === 0}<p class="py-6 text-center text-sm text-muted">No recordings yet.</p>{/if}
    {/if}
  </div>
</Drawer>

<style>
  .tabs {
    display: flex;
    padding: 2px;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .tab {
    flex: 1;
    padding: 4px 0;
    font-size: 12px;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
  }
  .tab[aria-selected="true"] {
    color: var(--text);
    background: var(--surface-3);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 4px;
    border-bottom: 1px solid var(--border);
  }
  .act {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    color: var(--text-muted);
    border-radius: 4px;
    opacity: 0.6;
  }
  .row:hover .act,
  .act:focus-visible {
    opacity: 1;
  }
  .act:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .exit {
    min-width: 22px;
    padding: 0 5px;
    font-family: var(--font-mono);
    font-size: 11px;
    text-align: center;
    color: var(--success);
    border: 1px solid currentColor;
    border-radius: 999px;
  }
  .exit.bad {
    color: var(--danger);
  }
  .dot {
    width: 8px;
    height: 8px;
    flex-shrink: 0;
    background: var(--text-faint);
    border-radius: 50%;
  }
  .dot.open {
    background: var(--success);
  }
  .dot.lost {
    background: var(--warning);
  }
</style>
