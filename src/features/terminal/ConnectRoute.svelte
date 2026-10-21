<script lang="ts">
import {
  Check,
  Desktop,
  DeviceMobile,
  HardDrives,
  Key,
  LockSimple,
  Password,
  ShieldCheck,
  TerminalWindow,
  X,
} from "phosphor-svelte";
import { pop } from "$lib/motion";
import Count from "$lib/ui/Count.svelte";
import { linkState, nodeState, type Progress } from "./connect";

/** The route a connection takes, lit up as it goes: this computer → jump hosts → server. */
let {
  progress,
  names,
  compact = false,
}: { progress: Progress; names: string[]; compact?: boolean } = $props();

const nodes = $derived(names.map((name, i) => ({ name, i: i + 1 })));
</script>

<div class="route" class:compact aria-hidden="true">
  <div class="node done origin">
    <span class="dot"><Desktop size={compact ? 13 : 15} /></span>
    {#if !compact}<span class="name">This computer</span>{/if}
  </div>
  {#each nodes as n (n.i)}
    {@const link = linkState(progress, n.i)}
    {@const node = nodeState(progress, n.i)}
    {@const rtt = progress.rtts.at(n.i - 1)}
    <div class="link" data-part={link}>
      <span class="fill"></span>
      {#if link === "active"}<span class="beam"></span>{/if}
      {#if rtt !== null && rtt !== undefined && !compact}
        <span class="rtt" in:pop><Count value={rtt} format={(v) => `${Math.round(v)} ms`} /></span>
      {/if}
    </div>
    <div class="node" data-part={node} class:target={n.i === names.length}>
      <span class="dot" class:ring-once={node === "done"}>
        <HardDrives size={compact ? 13 : 15} />
        {#if node === "broken"}
          <span class="badge bad" in:pop><X size={9} weight="bold" /></span>
        {:else if node === "done"}
          <span class="badge ok" in:pop><Check size={9} weight="bold" /></span>
        {:else if node === "active" && n.i === progress.hop}
          {#key progress.stage + (progress.method ?? "")}
            <span class="badge live" in:pop>
              {#if progress.stage === "secure"}<LockSimple size={9} weight="bold" />
              {:else if progress.stage === "verify"}<ShieldCheck size={9} weight="bold" />
              {:else if progress.stage === "login"}
                {#if progress.method === "password"}<Password size={9} weight="bold" />
                {:else if progress.method === "keyboard-interactive"}<DeviceMobile size={9} weight="bold" />
                {:else}<Key size={9} weight="bold" />{/if}
              {:else if progress.stage === "shell"}<TerminalWindow size={9} weight="bold" />{/if}
            </span>
          {/key}
        {/if}
      </span>
      {#if !compact}<span class="name" title={n.name}>{n.name}</span>{/if}
    </div>
  {/each}
</div>

<style>
  .route {
    display: flex;
    align-items: flex-start;
    width: 100%;
    padding: 0 4px;
  }
  .node {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    min-width: 0;
    max-width: 96px;
    flex-shrink: 0;
  }
  .name {
    max-width: 96px;
    overflow: hidden;
    font-size: 11px;
    color: var(--text-faint);
    text-overflow: ellipsis;
    white-space: nowrap;
    transition: color var(--dur) var(--ease-out);
  }
  .dot {
    position: relative;
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    color: var(--text-faint);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 999px;
    transition:
      color var(--dur) var(--ease-out),
      border-color var(--dur) var(--ease-out),
      background var(--dur) var(--ease-out),
      box-shadow var(--dur-slow) var(--ease-out);
  }
  .compact .dot {
    width: 26px;
    height: 26px;
  }
  .node.origin .dot,
  .node[data-part="done"] .dot {
    color: var(--success);
    border-color: color-mix(in oklab, var(--success) 50%, var(--border));
  }
  .node[data-part="done"] .name,
  .node.origin .name {
    color: var(--text-muted);
  }
  .node[data-part="active"] .dot {
    color: var(--accent);
    border-color: var(--accent);
    background: color-mix(in oklab, var(--accent) 10%, var(--surface-1));
    box-shadow: 0 0 0 4px color-mix(in oklab, var(--accent) 14%, transparent);
  }
  .node[data-part="active"] .name,
  .node.target .name {
    color: var(--text);
  }
  .node[data-part="broken"] .dot {
    color: var(--danger);
    border-color: var(--danger);
    background: color-mix(in oklab, var(--danger) 10%, var(--surface-1));
  }
  .badge {
    position: absolute;
    right: -3px;
    bottom: -3px;
    display: grid;
    place-items: center;
    width: 15px;
    height: 15px;
    color: var(--accent-fg);
    border: 2px solid var(--surface-2);
    border-radius: 999px;
  }
  .badge.ok {
    background: var(--success);
  }
  .badge.bad {
    background: var(--danger);
  }
  .badge.live {
    background: var(--accent);
    animation: hopr-badge-pulse 1.2s var(--ease-in-out) infinite;
  }

  .link {
    position: relative;
    flex: 1;
    min-width: 28px;
    height: 2px;
    margin: 16px 6px 0;
    background: var(--border);
    border-radius: 2px;
  }
  .compact .link {
    margin-top: 12px;
  }
  .fill {
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--success);
    transform: scaleX(0);
    transform-origin: left;
    transition: transform var(--dur-slow) var(--ease-out);
  }
  .link[data-part="done"] .fill {
    transform: scaleX(1);
  }
  .link[data-part="broken"] {
    background: repeating-linear-gradient(90deg, var(--danger) 0 5px, transparent 5px 9px);
  }
  /* A light running along the link while the connection opens. */
  .beam {
    position: absolute;
    top: -2px;
    left: 0;
    width: 28%;
    height: 6px;
    border-radius: 999px;
    background: linear-gradient(90deg, transparent, var(--accent), transparent);
    filter: drop-shadow(0 0 4px var(--accent));
    animation: hopr-route-beam 1.1s var(--ease-in-out) infinite;
  }
  .rtt {
    position: absolute;
    left: 50%;
    bottom: 8px;
    transform: translateX(-50%);
    font-size: 10.5px;
    color: var(--text-faint);
    white-space: nowrap;
  }

  @keyframes hopr-route-beam {
    from {
      transform: translateX(-100%);
      opacity: 0;
    }
    20% {
      opacity: 1;
    }
    to {
      transform: translateX(360%);
      opacity: 0;
    }
  }
  @keyframes hopr-badge-pulse {
    50% {
      box-shadow: 0 0 0 4px color-mix(in oklab, var(--accent) 25%, transparent);
    }
  }
  :global(:root[data-motion="reduced"]) .beam,
  :global(:root[data-motion="off"]) .beam {
    animation: none;
    left: 36%;
    opacity: 0.8;
  }
  :global(:root[data-motion="reduced"]) .badge.live {
    animation: none;
  }
</style>
