<script lang="ts">
import { sessions } from "./session.svelte";
import { parseTarget } from "./target";

const draft = sessions.draft;
let host = $state(draft?.host ?? "");
let port = $state(String(draft?.port ?? 22));
let username = $state(draft?.username ?? "");
let password = $state("");
let error = $state<string | null>(null);

function submit(e: SubmitEvent) {
  e.preventDefault();
  error = null;
  const parsed = parseTarget(host);
  if (!parsed) {
    error = "Enter a host like prod-db, 10.0.0.4 or deploy@prod-db:2222";
    return;
  }
  const user = (username.trim() || parsed.username || "").trim();
  const portNum = parsed.port ?? Number(port);
  if (!user) {
    error = "Enter a username";
    return;
  }
  if (!Number.isInteger(portNum) || portNum < 1 || portNum > 65535) {
    error = "Port must be between 1 and 65535";
    return;
  }
  sessions.open({ host: parsed.host, port: portNum, username: user }, password);
}
</script>

<form class="flex w-full max-w-sm flex-col gap-4" onsubmit={submit} autocomplete="off">
  <div>
    <h1 class="text-lg font-medium text-fg">Quick connect</h1>
    <p class="mt-1 text-sm text-muted">Password login for now. Saved hosts and keys arrive with the vault.</p>
  </div>

  <div class="grid grid-cols-[1fr_88px] gap-3">
    <label class="field">
      <span class="label-caps text-faint">Host</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:value={host} placeholder="deploy@prod-db" spellcheck="false" autofocus required />
    </label>
    <label class="field">
      <span class="label-caps text-faint">Port</span>
      <input class="tabular" bind:value={port} inputmode="numeric" />
    </label>
  </div>
  <label class="field">
    <span class="label-caps text-faint">Username</span>
    <input bind:value={username} placeholder="from host, or type one" spellcheck="false" />
  </label>
  <label class="field">
    <span class="label-caps text-faint">Password</span>
    <input type="password" bind:value={password} />
  </label>

  {#if error}<p class="text-sm text-danger" role="alert">{error}</p>{/if}

  <div class="flex items-center justify-between gap-3">
    {#if sessions.list.length > 0}
      <button type="button" class="btn-ghost" onclick={() => sessions.activate(sessions.list.at(-1)?.key ?? "")}>
        Back to sessions
      </button>
    {:else}<span></span>{/if}
    <button type="submit" class="btn-primary">Connect</button>
  </div>
</form>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .field input {
    width: 100%;
    padding: 8px 10px;
    font-size: 13px;
    color: var(--text);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    outline: none;
    user-select: text;
    transition: border-color var(--dur-fast) var(--ease-out);
  }
  .field input::placeholder {
    color: var(--text-faint);
  }
  .field input:focus {
    border-color: var(--accent);
  }
</style>
