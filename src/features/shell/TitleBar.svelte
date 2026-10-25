<script lang="ts">
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CopySimple, Minus, Square, X } from "phosphor-svelte";
import ProfileSwitcher from "$features/profiles/ProfileSwitcher.svelte";
import { profiles } from "$features/profiles/profiles.svelte";
import { sessions } from "$features/terminal/session.svelte";
import TabStrip from "$features/terminal/TabStrip.svelte";
import { vault } from "$features/vault/vault.svelte";
import HoprMark from "$lib/icons/HoprMark.svelte";
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

<header
  class="frame flex h-10 shrink-0 items-stretch border-b border-border bg-surface-1"
  data-tauri-drag-region
>
  <div class="grid w-12 shrink-0 place-items-center text-accent" data-tauri-drag-region>
    <HoprMark size={20} />
  </div>
  <!-- The profile chip shows outside terminal mode: the hosts screen and the lock screen. -->
  {#if !profiles.picking && (!vault.unlocked || !sessions.activeTab)}<ProfileSwitcher />{/if}
  {#if vault.unlocked}<TabStrip />{/if}
  <!-- The empty area right of the tabs doubles as the window drag region. -->
  <div class="min-w-0 flex-1" data-tauri-drag-region></div>

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
</header>

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
