<script lang="ts">
import { X } from "phosphor-svelte";
import { library } from "$features/hosts/library.svelte";
import { saveText } from "$lib/download";
import { pop } from "$lib/motion";
import FieldError from "$lib/ui/FieldError.svelte";
import { grouprun, toCsv, toJson } from "./grouprun.svelte";

let groupFilter = $state("*");
let filter = $state("");
let preview = $state(false);
let confirmProd = $state(false);
let expanded = $state<string | null>(null);

const candidates = $derived(
  library.hosts.filter(
    (h) =>
      h.protocol === "ssh" &&
      (groupFilter === "*" || (h.groupId ?? "") === groupFilter) &&
      (!filter ||
        `${h.label} ${h.hostname} ${h.tags.join(" ")}`
          .toLowerCase()
          .includes(filter.toLowerCase())),
  ),
);
const chosen = $derived(library.hosts.filter((h) => grouprun.selected.includes(h.id)));
const prodChosen = $derived(chosen.filter((h) => h.environment === "prod"));
const done = $derived(
  grouprun.results.filter((r) => r.state === "ok" || r.state === "failed" || r.state === "skipped")
    .length,
);

function toggle(id: string, on: boolean) {
  grouprun.selected = on ? [...grouprun.selected, id] : grouprun.selected.filter((x) => x !== id);
}

function selectShown(on: boolean) {
  const ids = candidates.map((h) => h.id);
  grouprun.selected = on
    ? [...new Set([...grouprun.selected, ...ids])]
    : grouprun.selected.filter((x) => !ids.includes(x));
}

let errors = $state<{ command?: string; hosts?: string }>({});

async function start() {
  errors = {};
  if (!grouprun.command.trim()) errors.command = "Type the command to run";
  if (grouprun.selected.length === 0) errors.hosts = "Tick at least one host on the left";
  if (errors.command || errors.hosts) {
    preview = false;
    return;
  }
  if (prodChosen.length && !confirmProd) {
    confirmProd = true;
    return;
  }
  confirmProd = false;
  preview = false;
  await grouprun.run();
}

function stamp() {
  return new Date().toISOString().slice(0, 19).replaceAll(":", "");
}
</script>

<section class="runner" aria-label="Run on hosts">
  <header class="flex items-center gap-2 border-b border-border px-4 py-2">
    <h2 class="flex-1 text-sm font-medium">Run a command on hosts</h2>
    <button type="button" class="tool" aria-label="Close run on hosts" onclick={() => (grouprun.open = false)}>
      <X size={15} />
    </button>
  </header>
  <div class="body">
    <aside class="hosts" aria-label="Choose hosts">
      <div class="flex gap-2">
        <select class="input w-32" bind:value={groupFilter} aria-label="Group">
          <option value="*">All groups</option>
          <option value="">No group</option>
          {#each library.groups as g (g.id)}<option value={g.id}>{g.name}</option>{/each}
        </select>
        <input class="input min-w-0 flex-1" bind:value={filter} placeholder="Filter" aria-label="Filter hosts" />
      </div>
      <div class="flex gap-3 py-1 text-xs">
        <button type="button" class="link" onclick={() => selectShown(true)}>Select shown</button>
        <button type="button" class="link" onclick={() => selectShown(false)}>Clear shown</button>
        <span class="flex-1 text-right text-muted">{grouprun.selected.length} chosen</span>
      </div>
      <div class="list">
        {#each candidates as h (h.id)}
          <label class="pick">
            <input
              type="checkbox"
              checked={grouprun.selected.includes(h.id)}
              onchange={(e) => toggle(h.id, e.currentTarget.checked)}
              aria-label="Run on {h.label}"
            />
            <span class="min-w-0 flex-1 truncate text-sm">{h.label}</span>
            {#if h.environment === "prod"}<span class="prod">prod</span>{/if}
          </label>
        {/each}
      </div>
    </aside>
    <div class="main">
      <textarea
        class="input font-mono text-xs"
        rows="3"
        bind:value={grouprun.command}
        placeholder="uptime"
        aria-label="Command to run"
        spellcheck="false"
        aria-invalid={!!errors.command}
        oninput={() => (errors.command = undefined)}
      ></textarea>
      <FieldError message={errors.command} />
      <FieldError message={grouprun.selected.length ? null : errors.hosts} />
      <div class="flex flex-wrap items-center gap-3 text-xs">
        <label class="flex items-center gap-1">
          At a time
          <select class="input w-16" bind:value={grouprun.parallel} aria-label="Parallel">
            {#each [1, 2, 4, 8, 16, 32] as n (n)}<option value={n}>{n}</option>{/each}
          </select>
        </label>
        <label class="check"><input type="checkbox" bind:checked={grouprun.stopOnFail} /> Stop after the first failure</label>
        <span class="flex-1"></span>
        <button
          type="button"
          class="btn-ghost"
          disabled={!grouprun.command.trim() || grouprun.selected.length === 0}
          title="Type a command and pick hosts to preview"
          onclick={() => (preview = !preview)}>Preview</button
        >
        {#if grouprun.running}
          <button type="button" class="btn-ghost" onclick={() => grouprun.cancel()}>Cancel</button>
        {:else}
          <button
            type="button"
            class="btn-primary"
            onclick={start}>Run on {grouprun.selected.length}</button
          >
        {/if}
      </div>
      {#if preview}
        <div class="panel text-xs" aria-label="Preview">
          Will run <code>{grouprun.command}</code> on {chosen.length} host{chosen.length === 1 ? "" : "s"},
          {grouprun.parallel} at a time{grouprun.stopOnFail ? ", stopping after the first failure" : ""}:
          {chosen.map((h) => h.label).join(", ")}
        </div>
      {/if}
      {#if confirmProd}
        <div class="confirm" role="alertdialog" aria-label="Confirm production run">
          <span class="flex-1 text-sm">
            This includes {prodChosen.length} production host{prodChosen.length === 1 ? "" : "s"}
            ({prodChosen.map((h) => h.label).join(", ")}). Run anyway?
          </span>
          <button type="button" class="btn-ghost" onclick={() => (confirmProd = false)}>Cancel</button>
          <button type="button" class="btn-danger" onclick={start}>Run</button>
        </div>
      {/if}
      {#if grouprun.error}<p class="text-xs text-danger" role="alert">{grouprun.error}</p>{/if}

      {#if grouprun.results.length}
        <div class="flex items-center gap-2 text-xs text-muted">
          <span class="flex-1" role="status">{done} of {grouprun.results.length} done</span>
          <button
            type="button"
            class="btn-ghost"
            disabled={grouprun.running}
            onclick={() => saveText(`hopr-run-${stamp()}.csv`, toCsv(grouprun.results), "text/csv")}>Export CSV</button
          >
          <button
            type="button"
            class="btn-ghost"
            disabled={grouprun.running}
            onclick={() =>
              saveText(`hopr-run-${stamp()}.json`, toJson(grouprun.ran, grouprun.results), "application/json")}
            >Export JSON</button
          >
        </div>
        <table class="grid" aria-label="Results">
          <thead>
            <tr><th>Host</th><th>State</th><th>Exit</th><th>Time</th><th>Output</th></tr>
          </thead>
          <tbody>
            {#each grouprun.results as r (r.hostId)}
              <tr data-result={r.label} data-state={r.state}>
                <td>{r.label}</td>
                <td>
                  {#key r.state}
                    <span class="state {r.state}" class:running-glow={r.state === "running"} in:pop={{ duration: 300 }}>{r.state}</span>
                  {/key}
                </td>
                <td class="tabular">{r.exitStatus ?? ""}</td>
                <td class="tabular">{r.durationMs === null ? "" : `${(r.durationMs / 1000).toFixed(1)}s`}</td>
                <td class="out">
                  <button
                    type="button"
                    class="outbtn"
                    onclick={() => (expanded = expanded === r.hostId ? null : r.hostId)}
                  >
                    {r.error ?? (r.stdout || r.stderr).split("\n")[0] ?? ""}
                  </button>
                </td>
              </tr>
              {#if expanded === r.hostId}
                <tr>
                  <td colspan="5"><pre class="full">{r.error ?? ""}{r.stdout}{r.stderr ? `\n${r.stderr}` : ""}</pre></td>
                </tr>
              {/if}
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  </div>
</section>

<style>
  .runner {
    position: absolute;
    inset: 0;
    z-index: 15;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .body {
    display: grid;
    min-height: 0;
    flex: 1;
    grid-template-columns: 260px 1fr;
    gap: 12px;
    padding: 12px 16px;
  }
  .hosts,
  .main {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 8px;
  }
  .main {
    overflow: auto;
  }
  .list {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .pick {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px;
  }
  .pick:hover {
    background: var(--surface-2);
  }
  .prod {
    padding: 0 4px;
    font-size: 10px;
    color: var(--danger);
    border: 1px solid currentColor;
    border-radius: 3px;
  }
  .confirm {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    background: color-mix(in oklab, var(--danger) 12%, transparent);
    border-left: 2px solid var(--danger);
    border-radius: var(--radius-sm);
  }
  .grid {
    width: 100%;
    font-size: 12px;
    border-collapse: collapse;
  }
  .grid th {
    padding: 4px 6px;
    font-weight: 500;
    text-align: left;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border);
  }
  .grid td {
    padding: 4px 6px;
    vertical-align: top;
    border-bottom: 1px solid var(--border);
  }
  .state {
    padding: 0 6px;
    border-radius: 999px;
    background: var(--surface-3);
  }
  .state.ok {
    color: var(--success);
  }
  .state.failed {
    color: var(--danger);
  }
  .state.running {
    color: var(--accent);
  }
  .out {
    max-width: 320px;
  }
  .outbtn {
    display: block;
    max-width: 100%;
    overflow: hidden;
    font-family: var(--font-mono);
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .full {
    max-height: 240px;
    overflow: auto;
    margin: 0;
    padding: 8px;
    font-family: var(--font-mono);
    font-size: 11px;
    white-space: pre-wrap;
    background: var(--surface-1);
    border-radius: var(--radius-sm);
  }
  .tool {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    color: var(--text-muted);
    border-radius: 4px;
  }
  .tool:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .link {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .running-glow {
    animation: run-glow 1.2s var(--ease-in-out) infinite;
  }
  @keyframes run-glow {
    50% {
      opacity: 0.55;
    }
  }
  :global(:root[data-motion="reduced"]) .running-glow {
    animation: none;
  }
</style>
