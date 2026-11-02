<script lang="ts">
/** Everything you can do with one saved host, behind its ⋯ button. */
import type { Snippet } from "svelte";
import { files } from "$features/sftp/files.svelte";
import { shell } from "$features/shell/shell.state.svelte";
import type { Host } from "$lib/ipc";
import Menu, { type MenuTriggerProps } from "$lib/ui/Menu.svelte";
import { sshCommand } from "./chain";
import { library } from "./library.svelte";

interface Props {
  host: Host;
  open: boolean;
  onopenchange: (open: boolean) => void;
  onconnect: (h: Host) => void;
  trigger: Snippet<[MenuTriggerProps]>;
}

let { host: h, open, onopenchange, onconnect, trigger }: Props = $props();

async function copy(text: string, what: string) {
  await navigator.clipboard.writeText(text);
  library.flash(`${what} copied`);
}

async function duplicate() {
  const r = await library.duplicate(h);
  if (r.ok) library.flash(`Created “${r.data.label}”`);
  else library.flash(r.error.message, "error");
}

async function browse() {
  shell.drawer = "files";
  await files.open(h.id);
}
</script>

<Menu label="Actions for {h.label}" {open} {onopenchange} {trigger}>
  <button type="button" role="menuitem" class="menu-item" onclick={() => onconnect(h)}>Connect</button>
  <button type="button" role="menuitem" class="menu-item" onclick={() => (library.editing = h.id)}>Edit</button>
  <button type="button" role="menuitem" class="menu-item" onclick={duplicate}>Duplicate</button>
  <button type="button" role="menuitem" class="menu-item" onclick={browse}>Browse files</button>
  <hr class="menu-sep" />
  <button type="button" role="menuitem" class="menu-item" onclick={() => copy(sshCommand(h, library.hosts), "SSH command")}>
    Copy SSH command
  </button>
  <button type="button" role="menuitem" class="menu-item" onclick={() => copy(h.hostname, "Hostname")}>Copy hostname</button>
  {#if h.hasPassword}
    <button type="button" role="menuitem" class="menu-item" onclick={() => library.copyPassword(h)}>Copy password</button>
  {/if}
  <hr class="menu-sep" />
  <button
    type="button"
    role="menuitem"
    class="menu-item"
    onclick={() => (library.deploying = { hostId: h.id, keyId: null })}>Deploy key…</button
  >
  <button type="button" role="menuitem" class="menu-item" onclick={() => library.ping([h.id])}>Check reachability</button>
  <hr class="menu-sep" />
  <button type="button" role="menuitem" class="menu-item danger" onclick={() => library.remove([h.id])}>Delete</button>
</Menu>
