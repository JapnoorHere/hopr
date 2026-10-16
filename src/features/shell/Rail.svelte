<script lang="ts">
import { GearSix } from "phosphor-svelte";
import IconButton from "$lib/ui/IconButton.svelte";
import { shell } from "./shell.state.svelte";
</script>

<nav
  class="flex w-12 shrink-0 flex-col items-center gap-1 border-r border-border bg-surface-1 py-2"
  aria-label="Primary"
>
  <!-- Feature items (Hosts, Files, Tunnels, Snippets, Keys) are added as their phases ship. -->
  <div class="flex-1"></div>
  <IconButton
    label="Settings"
    shortcut="Ctrl+,"
    tip="right"
    active={shell.drawer === "settings"}
    onclick={() => shell.toggle("settings")}
  >
    <GearSix size={18} weight="light" />
  </IconButton>
</nav>
