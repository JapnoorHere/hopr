<script lang="ts">
import { CheckCircle, Info, WarningCircle, X } from "phosphor-svelte";
import { flip } from "svelte/animate";
import { pop, sink, springIn } from "$lib/motion";
import { motion } from "$lib/platform";
import { toast } from "./toast.svelte";
</script>

<div class="toaster" aria-live="polite">
  {#each toast.list as t (t.id)}
    <div
      class="toast"
      data-kind={t.kind}
      role={t.kind === "error" ? "alert" : "status"}
      onpointerenter={() => toast.hold(t.id)}
      onpointerleave={() => toast.release(t.id)}
      in:springIn={{ y: 18, scale: 0.97 }}
      out:sink={{ y: 8, scale: 0.97 }}
      animate:flip={{ duration: motion(220) }}
    >
      <span class="icon" in:pop={{ delay: 90 }}>
        {#if t.kind === "success"}<CheckCircle size={16} weight="fill" />
        {:else if t.kind === "error"}<WarningCircle size={16} weight="fill" />
        {:else}<Info size={16} weight="fill" />{/if}
      </span>
      <span class="min-w-0 flex-1 text-sm text-fg">{t.text}</span>
      {#if t.action}
        <button
          type="button"
          class="btn-ghost"
          onclick={() => {
            t.action?.run();
            toast.dismiss(t.id);
          }}>{t.action.label}</button
        >
      {/if}
      <button type="button" class="close" aria-label="Dismiss" onclick={() => toast.dismiss(t.id)}>
        <X size={13} />
      </button>
      {#if t.armed && t.life}
        {#key t.armed}
          <span class="life" style:animation-duration="{t.life}ms" aria-hidden="true"></span>
        {/key}
      {/if}
    </div>
  {/each}
</div>

<style>
  .toaster {
    position: fixed;
    left: 50%;
    bottom: 40px;
    z-index: 60;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    width: min(440px, calc(100vw - 32px));
    transform: translateX(-50%);
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 6px 6px 12px;
    background: var(--surface-3);
    border: 1px solid var(--border);
    border-left: 2px solid var(--accent);
    border-radius: var(--radius);
    box-shadow: var(--shadow-pop);
    pointer-events: auto;
    position: relative;
    overflow: hidden;
  }
  /* How long the toast stays: a thin line that shrinks; paused while hovered. */
  .life {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 2px;
    background: color-mix(in oklab, currentColor 30%, transparent);
    color: var(--accent);
    transform-origin: left;
    animation: toast-life linear forwards;
  }
  .toast:hover .life {
    animation-play-state: paused;
  }
  .toast[data-kind="success"] .life {
    color: var(--success);
  }
  .toast[data-kind="error"] .life {
    color: var(--danger);
  }
  @keyframes toast-life {
    to {
      transform: scaleX(0);
    }
  }
  :global(:root[data-motion="off"]) .life {
    display: none;
  }
  .toast[data-kind="success"] {
    border-left-color: var(--success);
  }
  .toast[data-kind="error"] {
    border-left-color: var(--danger);
  }
  .icon {
    display: flex;
    color: var(--accent);
  }
  .toast[data-kind="success"] .icon {
    color: var(--success);
  }
  .toast[data-kind="error"] .icon {
    color: var(--danger);
  }
  .close {
    display: flex;
    padding: 6px;
    color: var(--text-faint);
    border-radius: var(--radius-sm);
  }
  .close:hover {
    color: var(--text);
    background: var(--surface-2);
  }
</style>
