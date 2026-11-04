<script lang="ts">
/** "Are you sure?" in place: a message, Cancel, and the action. Danger for things that can't be undone. */
import type { Snippet } from "svelte";
import { blurFade } from "$lib/motion";

interface Props {
  tone?: "warning" | "danger";
  confirmLabel: string;
  onconfirm: () => void;
  oncancel: () => void;
  /** The question. */
  children: Snippet;
}

let { tone = "warning", confirmLabel, onconfirm, oncancel, children }: Props = $props();
</script>

<div class="bar" class:danger={tone === "danger"} role="alertdialog" in:blurFade={{ y: -2, duration: 140 }}>
  <span class="msg">{@render children()}</span>
  <span class="actions">
    <button type="button" class="btn-ghost" onclick={oncancel}>Cancel</button>
    <button type="button" class={tone === "danger" ? "btn-danger" : "btn-primary"} onclick={onconfirm}>
      {confirmLabel}
    </button>
  </span>
</div>

<style>
  .bar {
    --tone: var(--warning);
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 6px 6px 6px 12px;
    font-size: 13px;
    color: var(--text-muted);
    background: color-mix(in oklab, var(--tone) 10%, transparent);
    border-left: 2px solid var(--tone);
    border-radius: var(--radius-sm);
  }
  .bar.danger {
    --tone: var(--danger);
  }
  .msg {
    flex: 1 1 180px;
    min-width: 0;
  }
  .actions {
    display: flex;
    gap: 6px;
    margin-left: auto;
  }
</style>
