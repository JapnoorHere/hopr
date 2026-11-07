<script lang="ts">
/** Minimize, maximize/restore and close, for Hopr's frameless windows. */
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CopySimple, Minus, Square, X } from "phosphor-svelte";
import { inTauri } from "$lib/ipc";

const win = inTauri ? getCurrentWindow() : null;
let maximized = $state(false);

$effect(() => {
  if (!win) return;
  let unlisten: (() => void) | undefined;
  let disposed = false;
  const sync = async () => {
    maximized = await win.isMaximized();
  };
  void sync();
  void win.onResized(sync).then((u) => {
    if (disposed) u();
    else unlisten = u;
  });
  return () => {
    disposed = true;
    unlisten?.();
  };
});
</script>

{#if win}
  <div class="flex shrink-0">
    <button type="button" class="wc" aria-label="Minimize" onclick={() => win.minimize()}>
      <Minus size={14} />
    </button>
    <button
      type="button"
      class="wc"
      aria-label={maximized ? "Restore" : "Maximize"}
      onclick={() => win.toggleMaximize()}
    >
      {#if maximized}<CopySimple size={13} />{:else}<Square size={12} />{/if}
    </button>
    <button type="button" class="wc close" aria-label="Close" onclick={() => win.close()}>
      <X size={14} />
    </button>
  </div>
{/if}

<style>
  .wc {
    display: grid;
    place-items: center;
    width: 46px;
    color: var(--text-muted);
    transition:
      background var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out);
  }
  .wc:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  .wc.close:hover {
    background: var(--danger);
    color: var(--accent-fg);
  }
</style>
