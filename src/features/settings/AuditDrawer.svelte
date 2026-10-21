<script lang="ts">
import { shell } from "$features/shell/shell.state.svelte";
import { saveText } from "$lib/download";
import { type AuditEvent, commands, inTauri } from "$lib/ipc";
import Drawer from "$lib/ui/Drawer.svelte";
import { AUDIT_FILTERS, type AuditFilter, auditCsv, auditTitle, matchesFilter } from "./audit";

let events = $state<AuditEvent[]>([]);
let filter = $state<AuditFilter>("all");
let query = $state("");
let loading = $state(true);

$effect(() => {
  void load();
});

async function load() {
  loading = true;
  if (inTauri) {
    const r = await commands.auditList(2000);
    if (r.status === "ok") events = r.data;
  }
  loading = false;
}

const shown = $derived(
  events.filter((e) => {
    if (!matchesFilter(e, filter)) return false;
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return [auditTitle(e.kind), e.hostLabel ?? "", e.detail].some((s) =>
      s.toLowerCase().includes(q),
    );
  }),
);

function when(at: string): string {
  const d = new Date(at);
  return Number.isNaN(d.getTime()) ? at : d.toLocaleString();
}

function exportAs(kind: "json" | "csv") {
  const stamp = new Date().toISOString().slice(0, 10);
  if (kind === "json")
    saveText(`hopr-audit-${stamp}.json`, JSON.stringify(shown, null, 2), "application/json");
  else saveText(`hopr-audit-${stamp}.csv`, auditCsv(shown), "text/csv");
}
</script>

<Drawer title="Audit log" pinned={shell.pinned} onclose={() => shell.close()} onpin={() => shell.togglePin()}>
  <div class="flex flex-col gap-2 pt-2">
    <p class="text-xs text-muted">
      Unlocks, connections, failed logins, host-key decisions, copied secrets, key deploys, exports and security
      changes. Stored on this device only; never contains a secret.
    </p>
    <div class="flex gap-2">
      <input class="input flex-1" bind:value={query} placeholder="Filter" aria-label="Filter audit log" spellcheck="false" />
      <select class="input w-36" bind:value={filter} aria-label="Event type">
        {#each AUDIT_FILTERS as f (f.id)}
          <option value={f.id}>{f.label}</option>
        {/each}
      </select>
    </div>
    <div class="flex gap-2">
      <button type="button" class="btn-ghost" onclick={() => load()}>Refresh</button>
      <span class="flex-1"></span>
      <button type="button" class="btn-ghost" disabled={!shown.length} onclick={() => exportAs("csv")}>Export CSV</button>
      <button type="button" class="btn-ghost" disabled={!shown.length} onclick={() => exportAs("json")}>Export JSON</button>
    </div>
  </div>

  <ol class="mt-3 flex flex-col gap-1" aria-label="Audit events">
    {#each shown as e (e.id)}
      <li class="event" data-kind={e.kind}>
        <div class="flex items-baseline gap-2">
          <span class="flex-1 truncate text-sm text-fg">{auditTitle(e.kind)}{e.hostLabel ? ` · ${e.hostLabel}` : ""}</span>
          <time class="tabular shrink-0 text-xs text-faint" datetime={e.at}>{when(e.at)}</time>
        </div>
        {#if e.detail}<p class="truncate text-xs text-muted" title={e.detail}>{e.detail}</p>{/if}
      </li>
    {:else}
      <li class="text-sm text-muted">{loading ? "Loading…" : "Nothing recorded yet."}</li>
    {/each}
  </ol>
</Drawer>

<style>
  .event {
    padding: 6px 10px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-left: 2px solid var(--border-strong);
    border-radius: var(--radius-sm);
  }
  .event[data-kind$="failed"],
  .event[data-kind="host_key.blocked"],
  .event[data-kind="host_key.replace"] {
    border-left-color: var(--danger);
  }
  .event[data-kind="host.copy_password"],
  .event[data-kind^="key."] {
    border-left-color: var(--warning);
  }
</style>
