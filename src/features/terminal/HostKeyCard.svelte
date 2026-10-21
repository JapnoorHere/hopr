<script lang="ts">
import { ShieldCheck, ShieldWarning } from "phosphor-svelte";
import type { HostKeyDecision, HostKeyPrompt } from "$lib/ipc";

let { prompt, onanswer }: { prompt: HostKeyPrompt; onanswer: (d: HostKeyDecision) => void } =
  $props();

let understood = $state(false);
const target = $derived(prompt.port === 22 ? prompt.host : `${prompt.host}:${prompt.port}`);
</script>

{#if !prompt.changed}
  <div class="panel w-[26rem]" role="dialog" aria-label="Verify host key">
    <div class="flex items-start gap-3">
      <span class="mt-0.5 text-accent"><ShieldCheck size={20} /></span>
      <div class="min-w-0 flex-1">
        <p class="text-sm font-medium text-fg">First connection to {target}</p>
        <p class="mt-1 text-xs text-muted">
          Check that this fingerprint matches the server's before trusting it. Hopr will warn you if it
          ever changes.
        </p>
      </div>
    </div>
    <div class="fp">
      <span class="label-caps text-faint">{prompt.keyType}</span>
      <code class="font-mono text-xs break-all text-fg select-text" data-testid="host-fingerprint"
        >{prompt.fingerprint}</code
      >
    </div>
    <div class="flex justify-end gap-2">
      <button type="button" class="btn-ghost" onclick={() => onanswer("reject")}>Cancel</button>
      <button type="button" class="btn-ghost" onclick={() => onanswer("once")}>Connect once</button>
      <!-- svelte-ignore a11y_autofocus -->
      <button type="button" class="btn-primary" autofocus onclick={() => onanswer("trust")}>
        Trust and connect
      </button>
    </div>
  </div>
{:else}
  <div class="panel danger w-[28rem]" role="alertdialog" aria-label="Host key changed">
    <div class="flex items-start gap-3">
      <span class="mt-0.5 text-danger"><ShieldWarning size={22} weight="fill" /></span>
      <div class="min-w-0 flex-1">
        <p class="text-sm font-medium text-fg">The host key for {target} has changed</p>
        <p class="mt-1 text-xs text-muted">
          Someone could be intercepting this connection. It is also what happens when a server is
          reinstalled. Only continue if you know why the key changed.
        </p>
      </div>
    </div>
    <div class="fp">
      <span class="label-caps text-faint">Trusted before</span>
      {#each prompt.previous as fp (fp)}
        <code class="font-mono text-xs break-all text-muted line-through select-text">{fp}</code>
      {/each}
      <span class="label-caps mt-2 text-faint">Presented now ({prompt.keyType})</span>
      <code class="font-mono text-xs break-all text-fg select-text">{prompt.fingerprint}</code>
    </div>
    <label class="check">
      <input type="checkbox" bind:checked={understood} />
      I've confirmed the new key with the server's owner
    </label>
    <div class="flex justify-end gap-2">
      <button type="button" class="btn-danger" disabled={!understood} onclick={() => onanswer("trust")}>
        Replace key and connect
      </button>
      <!-- svelte-ignore a11y_autofocus -->
      <button type="button" class="btn-primary" autofocus onclick={() => onanswer("reject")}>
        Don't connect
      </button>
    </div>
  </div>
{/if}

<style>
  .fp {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .danger {
    border-color: color-mix(in oklab, var(--danger) 55%, var(--border));
    box-shadow: 0 0 0 1px color-mix(in oklab, var(--danger) 30%, transparent);
  }
</style>
