<script lang="ts">
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { ArrowDown, ArrowUp, Check } from "phosphor-svelte";
import { flip } from "svelte/animate";
import type { Transfer } from "$lib/ipc";
import { blurFade, pop, sink, springIn } from "$lib/motion";
import { motion } from "$lib/platform";
import Count from "$lib/ui/Count.svelte";
import { formatSize, formatSpeed } from "./format";
import { transfers } from "./transfers.svelte";

const s = $derived(transfers.summary);

function label(t: Transfer): string {
  if (t.status === "running") {
    return `${formatSize(t.done)} of ${formatSize(t.size)} · ${formatSpeed(t.speed)}`;
  }
  if (t.status === "failed") return t.error ?? "Failed";
  return `${t.status[0]?.toUpperCase()}${t.status.slice(1)}`;
}

function pct(t: Transfer): number {
  if (t.size > 0) return Math.min(100, Math.round((t.done / t.size) * 100));
  return t.status === "done" ? 100 : 0;
}
</script>

<div class="wrap">
  {#if transfers.open}
    <section class="panel" aria-label="Transfers" in:springIn={{ y: 12, scale: 0.97 }} out:sink={{ y: 8, scale: 0.98 }}>
      <header class="flex items-center gap-2 px-3 py-2">
        <h3 class="flex-1 text-sm font-medium text-fg">Transfers</h3>
        <button type="button" class="btn-ghost" onclick={() => transfers.clear()}>Clear finished</button>
        <button type="button" class="btn-ghost" onclick={() => (transfers.open = false)}>Close</button>
      </header>
      <ul class="list">
        {#each [...transfers.list].reverse() as t (t.id)}
          <li
            class="item"
            data-status={t.status}
            data-transfer={t.name}
            in:blurFade={{ y: 6, blur: 0, duration: 180 }}
            animate:flip={{ duration: motion(200) }}
          >
            <span class="text-faint">
              {#if t.status === "done"}
                <span class="grid text-success" in:pop><Check size={13} weight="bold" /></span>
              {:else if t.direction === "upload"}<ArrowUp size={13} />{:else}<ArrowDown size={13} />{/if}
            </span>
            <span class="min-w-0 flex-1">
              <span class="block truncate text-sm text-fg">{t.name}</span>
              <span class="block truncate text-xs text-muted">{t.hostLabel} · {label(t)}</span>
              <span class="bar"><span class="fill" style:width="{pct(t)}%"></span></span>
            </span>
            {#if t.status === "running" || t.status === "queued"}
              <button type="button" class="btn-ghost" onclick={() => transfers.pause(t)}>Pause</button>
              <button type="button" class="btn-ghost" onclick={() => transfers.cancel(t)}>Cancel</button>
            {:else if t.status === "paused"}
              <button type="button" class="btn-ghost" onclick={() => transfers.resume(t)}>Resume</button>
              <button type="button" class="btn-ghost" onclick={() => transfers.cancel(t)}>Cancel</button>
            {:else if t.status === "failed" || t.status === "cancelled"}
              <button type="button" class="btn-ghost" onclick={() => transfers.resume(t)}>Retry</button>
            {:else if t.status === "done" && t.direction === "download"}
              <button type="button" class="btn-ghost" onclick={() => revealItemInDir(t.local)}>Show</button>
            {/if}
          </li>
        {:else}
          <li class="p-3 text-sm text-muted">No transfers yet.</li>
        {/each}
      </ul>
    </section>
  {/if}
  {#if transfers.list.length}
    <button
      type="button"
      class="pill"
      class:failed={s.failed > 0}
      class:done={!s.count && !s.failed}
      in:springIn={{ y: 14, scale: 0.9 }}
      out:sink={{ y: 10, scale: 0.95 }}
      aria-expanded={transfers.open}
      aria-label="Transfers"
      onclick={() => (transfers.open = !transfers.open)}
    >
      <!-- Progress ring: fills as the transfers do; a check when all are done. -->
      <svg class="ring" viewBox="0 0 16 16" aria-hidden="true">
        <circle cx="8" cy="8" r="6" class="track" />
        <circle
          cx="8"
          cy="8"
          r="6"
          class="arc"
          pathLength="100"
          style:stroke-dashoffset={100 - (s.count ? s.percent : s.failed ? 100 : 100)}
        />
      </svg>
      {#if s.count}
        {s.count} transfer{s.count === 1 ? "" : "s"} · <Count value={s.percent} format={(v) => `${Math.round(v)}%`} />{s.speed ? ` · ${formatSpeed(s.speed)}` : ""}
      {:else if s.failed}
        {s.failed} failed
      {:else}
        <span class="inline-flex items-center gap-1" in:blurFade={{ y: 0, blur: 2 }}>Transfers done</span>
      {/if}
    </button>
  {/if}
</div>

<style>
  .wrap {
    position: fixed;
    right: 16px;
    bottom: 36px;
    z-index: 35;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 8px;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px 5px 8px;
    transition:
      border-color var(--dur-slow) var(--ease-out),
      transform var(--dur-instant) var(--ease-out);
    font-size: 12px;
    color: var(--text);
    background: var(--surface-3);
    border: 1px solid var(--border);
    border-left: 2px solid var(--accent);
    border-radius: 999px;
  }
  .pill:active {
    transform: scale(0.96);
  }
  .pill.failed {
    border-left-color: var(--danger);
  }
  .pill.done {
    border-left-color: var(--success);
  }
  .ring {
    width: 14px;
    height: 14px;
    transform: rotate(-90deg);
  }
  .ring circle {
    fill: none;
    stroke-width: 2.2;
  }
  .track {
    stroke: var(--surface-4);
  }
  .arc {
    stroke: var(--accent);
    stroke-dasharray: 100;
    stroke-linecap: round;
    transition:
      stroke-dashoffset var(--dur-slow) var(--ease-out),
      stroke var(--dur-slow) var(--ease-out);
  }
  .pill.done .arc {
    stroke: var(--success);
  }
  .pill.failed .arc {
    stroke: var(--danger);
  }
  .panel {
    width: 420px;
    max-height: 50vh;
    display: flex;
    flex-direction: column;
    background: var(--surface-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  .list {
    overflow-y: auto;
    padding: 0 8px 8px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 4px;
    border-top: 1px solid var(--border);
  }
  .bar {
    display: block;
    height: 3px;
    margin-top: 4px;
    background: var(--surface-4);
    border-radius: 2px;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--accent);
    border-radius: 2px;
    transition:
      width var(--dur-slow) var(--ease-out),
      background var(--dur) var(--ease-out);
  }
  /* A light runs along a transfer that is moving. */
  .item[data-status="running"] .fill {
    background:
      linear-gradient(90deg, transparent, color-mix(in oklab, white 35%, transparent), transparent) 0 0 / 40% 100% no-repeat,
      var(--accent);
    animation: fill-run 1.2s linear infinite;
  }
  .item[data-status="done"] .fill {
    background: var(--success);
  }
  @keyframes fill-run {
    from {
      background-position: -60% 0, 0 0;
    }
    to {
      background-position: 160% 0, 0 0;
    }
  }
  :global(:root[data-motion="reduced"]) .item[data-status="running"] .fill {
    animation: none;
  }
  .item[data-status="failed"] .fill {
    background: var(--danger);
  }
</style>
