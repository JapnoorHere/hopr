<script lang="ts">
import { onDestroy } from "svelte";
import { blurFade } from "$lib/motion";
import Kbd from "$lib/ui/Kbd.svelte";
import ConnectRoute from "./ConnectRoute.svelte";
import { type Progress, STEPS, stageLabel, stepState } from "./connect";

/** The card over a connecting tab: the route, what is happening now, and the steps left. */
let {
  progress,
  names,
  address,
  oncancel,
}: { progress: Progress; names: string[]; address: string | null; oncancel: () => void } = $props();

const label = $derived(stageLabel(progress, names));

// Slow connections get a quiet timer, then a hint about what might be wrong.
const started = Date.now();
let elapsed = $state(0);
const tick = setInterval(() => (elapsed = Math.floor((Date.now() - started) / 1000)), 1000);
onDestroy(() => clearInterval(tick));
</script>

<div class="card panel" role="status" aria-live="polite">
  <ConnectRoute {progress} {names} />

  <div class="flex min-h-10 flex-col items-center justify-center gap-0.5 text-center">
    {#key label}
      {#if progress.stage === "ready"}
        <p class="text-sm font-medium text-success" in:blurFade={{ y: 4, blur: 2, duration: 200 }}>{label}</p>
      {:else}
        <p class="shimmer-text text-sm font-medium" in:blurFade={{ y: 4, blur: 2, duration: 200 }}>{label}…</p>
      {/if}
    {/key}
    {#if address}<p class="tabular text-xs text-faint">{address}</p>{/if}
  </div>

  <ol class="steps" aria-label="Steps">
    {#each STEPS as s (s.stage)}
      <li data-part={stepState(progress, s.stage)}>
        <span class="pip"></span>
        <span>{s.label}</span>
      </li>
    {/each}
  </ol>

  <div class="flex items-center gap-2">
    <span class="flex-1 text-xs text-faint">
      {#if elapsed >= 8}
        <span in:blurFade>Still trying after {elapsed} s. The server may be busy, or a firewall may block it.</span>
      {:else if elapsed >= 3}
        <span class="tabular" in:blurFade>{elapsed} s</span>
      {/if}
    </span>
    <button type="button" class="btn-ghost inline-flex items-center gap-2" title="Cancel (Esc)" onclick={oncancel}>
      Cancel <Kbd keys="Esc" />
    </button>
  </div>
</div>

<style>
  .card {
    width: min(420px, calc(100% - 32px));
    gap: 16px;
    padding: 20px 20px 12px;
    box-shadow: var(--shadow-pop);
  }
  .steps {
    display: flex;
    gap: 4px;
    padding: 0;
    margin: 0;
    list-style: none;
  }
  .steps li {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    font-size: 10.5px;
    color: var(--text-faint);
    transition: color var(--dur) var(--ease-out);
  }
  .pip {
    width: 100%;
    height: 3px;
    border-radius: 999px;
    background: var(--border);
    background-image: linear-gradient(var(--success), var(--success));
    background-repeat: no-repeat;
    background-size: 0% 100%;
    transition: background-size var(--dur-slow) var(--ease-out);
  }
  li[data-part="done"] {
    color: var(--text-muted);
  }
  li[data-part="done"] .pip {
    background-size: 100% 100%;
  }
  li[data-part="active"] {
    color: var(--text);
  }
  li[data-part="active"] .pip {
    background-image: linear-gradient(90deg, transparent, var(--accent), transparent);
    background-size: 50% 100%;
    animation: hopr-pip 1.1s var(--ease-in-out) infinite;
  }
  li[data-part="broken"] {
    color: var(--danger);
  }
  li[data-part="broken"] .pip {
    background: var(--danger);
  }
  @keyframes hopr-pip {
    from {
      background-position: -100% 0;
    }
    to {
      background-position: 200% 0;
    }
  }
  :global(:root[data-motion="reduced"]) li[data-part="active"] .pip,
  :global(:root[data-motion="off"]) li[data-part="active"] .pip {
    animation: none;
    background: var(--accent);
    opacity: 0.6;
  }
</style>
