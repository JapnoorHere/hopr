<script lang="ts">
/** Change a remote entry's permissions: tick boxes or type the octal, optionally for a whole folder. */
import type { RemoteEntry } from "$lib/ipc";
import { files } from "./files.svelte";
import { octal, parseOctal } from "./format";

let { entry, onclose }: { entry: RemoteEntry; onclose: () => void } = $props();

// svelte-ignore state_referenced_locally
let mode = $state(entry.mode);
// svelte-ignore state_referenced_locally
let text = $state(octal(entry.mode));
let recursive = $state(false);

const BITS = [
  { who: "Owner", r: 0o400, w: 0o200, x: 0o100 },
  { who: "Group", r: 0o040, w: 0o020, x: 0o010 },
  { who: "Others", r: 0o004, w: 0o002, x: 0o001 },
];

function toggle(bit: number) {
  mode ^= bit;
  text = octal(mode);
}

function typed(e: Event) {
  text = (e.currentTarget as HTMLInputElement).value;
  const m = parseOctal(text);
  if (m !== null) mode = m;
}

async function onsubmit(e: SubmitEvent) {
  e.preventDefault();
  onclose();
  await files.chmod(entry.path, mode, recursive);
}
</script>

<form class="overlay" {onsubmit} aria-label="Permissions">
  <h3 class="text-sm font-medium text-fg">Permissions · {entry.name}</h3>
  <table class="perm-grid">
    <thead><tr><th></th><th>Read</th><th>Write</th><th>Execute</th></tr></thead>
    <tbody>
      {#each BITS as b (b.who)}
        <tr>
          <th>{b.who}</th>
          {#each [b.r, b.w, b.x] as bit, i (i)}
            <td>
              <input
                type="checkbox"
                checked={(mode & bit) !== 0}
                onchange={() => toggle(bit)}
                aria-label="{b.who} {['read', 'write', 'execute'][i]}"
              />
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
  <label class="field">
    <span class="text-xs text-muted">Octal</span>
    <input class="input font-mono" value={text} oninput={typed} aria-label="Octal permissions" />
  </label>
  {#if entry.kind === "dir"}
    <label class="check text-sm"><input type="checkbox" bind:checked={recursive} /> Apply to everything inside</label>
  {/if}
  <div class="flex justify-end gap-2">
    <button type="button" class="btn-ghost" onclick={onclose}>Cancel</button>
    <button type="submit" class="btn-primary" disabled={parseOctal(text) === null}>Apply {octal(mode)}</button>
  </div>
</form>

<style>
  .overlay {
    position: absolute;
    inset: auto 12px 12px;
    z-index: 25;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px;
    background: var(--surface-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  .perm-grid {
    font-size: 12px;
    color: var(--text-muted);
    border-spacing: 12px 4px;
  }
  .perm-grid th {
    font-weight: normal;
    text-align: left;
  }
  .perm-grid td {
    text-align: center;
  }
</style>
