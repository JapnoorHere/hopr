<script lang="ts">
import ProfileSwitcher from "$features/profiles/ProfileSwitcher.svelte";
import { profiles } from "$features/profiles/profiles.svelte";
import { sessions } from "$features/terminal/session.svelte";
import TabStrip from "$features/terminal/TabStrip.svelte";
import { vault } from "$features/vault/vault.svelte";
import HoprMark from "$lib/icons/HoprMark.svelte";
import WindowControls from "./WindowControls.svelte";
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
  <WindowControls />
</header>
