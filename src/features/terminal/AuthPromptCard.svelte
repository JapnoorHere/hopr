<script lang="ts">
import { Key } from "phosphor-svelte";
import type { AuthPrompt } from "$lib/ipc";

let {
  prompt,
  onanswer,
}: { prompt: AuthPrompt; onanswer: (responses: string[] | null, remember: boolean) => void } =
  $props();

// The card is re-created per prompt ({#key} in TerminalPane), so the initial value is enough.
// svelte-ignore state_referenced_locally
let values = $state<string[]>(prompt.fields.map(() => ""));
let remember = $state(false);

function submit(e: SubmitEvent) {
  e.preventDefault();
  const responses = values;
  values = prompt.fields.map(() => "");
  onanswer(responses, remember);
}
</script>

<form class="panel w-96" onsubmit={submit} aria-label={prompt.title}>
  <div class="flex items-start gap-3">
    <span class="mt-0.5 text-accent"><Key size={20} /></span>
    <div class="min-w-0 flex-1">
      <p class="text-sm font-medium text-fg">{prompt.title}</p>
      {#if prompt.instructions}
        <p class="mt-1 text-xs whitespace-pre-line text-muted">{prompt.instructions}</p>
      {/if}
    </div>
  </div>
  {#each prompt.fields as field, i (i)}
    <label class="field">
      <span class="text-xs text-muted">{field.label.replace(/:\s*$/, "")}</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="input"
        type={field.echo ? "text" : "password"}
        bind:value={values[i]}
        autocomplete="off"
        spellcheck="false"
        autofocus={i === 0}
      />
    </label>
  {/each}
  {#if prompt.canRemember}
    <label class="check">
      <input type="checkbox" bind:checked={remember} />
      Save password to the vault
    </label>
  {/if}
  <div class="flex justify-end gap-2">
    <button type="button" class="btn-ghost" onclick={() => onanswer(null, false)}>Cancel</button>
    <button type="submit" class="btn-primary">Continue</button>
  </div>
</form>
