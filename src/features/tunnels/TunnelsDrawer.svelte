<script lang="ts">
import { PencilSimple, Plus, Trash } from "phosphor-svelte";
import { library } from "$features/hosts/library.svelte";
import { shell } from "$features/shell/shell.state.svelte";
import { commands, type Tunnel, type TunnelKind } from "$lib/ipc";
import { blurFade, slidingPill, stagger } from "$lib/motion";
import Drawer from "$lib/ui/Drawer.svelte";
import FieldError from "$lib/ui/FieldError.svelte";
import { type FieldErrors, focusFirstInvalid, hasErrors, parsePort } from "$lib/ui/form";
import { describe, exposed, rate, sparkline } from "./format";
import { tunnels } from "./tunnels.svelte";

const KINDS: { id: TunnelKind; label: string; hint: string }[] = [
  { id: "local", label: "Local (-L)", hint: "A port here reaches a service as the host sees it." },
  {
    id: "remote",
    label: "Remote (-R)",
    hint: "A port on the host reaches a service as this computer sees it.",
  },
  {
    id: "dynamic",
    label: "SOCKS (-D)",
    hint: "A SOCKS5 proxy here; apps' connections leave from the host.",
  },
];

const sshHosts = $derived(library.hosts.filter((h) => h.protocol === "ssh"));

// Editor state.
let hostId = $state("");
let label = $state("");
let kind = $state<TunnelKind>("local");
let bindAddr = $state("127.0.0.1");
let bindPort = $state("");
let destHost = $state("localhost");
let destPort = $state("");
let autoStart = $state(false);
let errors = $state<
  FieldErrors<"hostId" | "bindAddr" | "bindPort" | "destHost" | "destPort" | "label" | "form">
>({});
let portBusy = $state(false);
let editor = $state<HTMLFormElement>();
const fix = (f: keyof typeof errors) => () => {
  if (errors[f]) errors[f] = undefined;
};

$effect(() => {
  void tunnels.refresh();
});

function edit(t: Tunnel | null) {
  errors = {};
  portBusy = false;
  tunnels.editing = t?.id ?? "new";
  hostId = t?.hostId ?? sshHosts[0]?.id ?? "";
  label = t?.label ?? "";
  kind = t?.kind ?? "local";
  bindAddr = t?.bindAddr ?? "127.0.0.1";
  bindPort = t ? String(t.bindPort) : "";
  destHost = t?.destHost || "localhost";
  destPort = t?.destPort ? String(t.destPort) : "";
  autoStart = t?.autoStart ?? false;
}

async function checkPort() {
  const p = Number(bindPort);
  portBusy =
    kind !== "remote" &&
    Number.isInteger(p) &&
    p > 0 &&
    !(await commands.portFree(bindAddr || "127.0.0.1", p));
}

async function pickPort() {
  const r = await commands.portPick(bindAddr || "127.0.0.1");
  if (r.status === "ok") {
    bindPort = String(r.data);
    portBusy = false;
  }
}

async function save(e: SubmitEvent) {
  e.preventDefault();
  errors = {};
  const bp = bindPort.trim() === "" || bindPort.trim() === "0" ? 0 : parsePort(bindPort);
  const dp = kind === "dynamic" ? 0 : parsePort(destPort);
  if (!hostId) errors.hostId = "Choose the SSH host to tunnel through";
  if (!bindAddr.trim()) errors.bindAddr = "Enter the address to listen on, e.g. 127.0.0.1";
  if (bp === null)
    errors.bindPort = "Port must be between 1 and 65535 (leave empty to pick a free one)";
  else if (portBusy) errors.bindPort = "That port is in use here; pick another";
  if (kind !== "dynamic") {
    if (!destHost.trim()) errors.destHost = "Enter the destination host";
    if (dp === null) errors.destPort = "Destination port must be between 1 and 65535";
  }
  if (hasErrors(errors)) return focusFirstInvalid(editor);
  const failed = await tunnels.save(tunnels.editing === "new" ? null : tunnels.editing, {
    hostId,
    label,
    kind,
    bindAddr,
    bindPort: bp ?? 0,
    destHost: kind === "dynamic" ? "" : destHost,
    destPort: dp ?? 0,
    autoStart,
  });
  if (failed) {
    const m = failed.toLowerCase();
    errors = m.includes("destination port")
      ? { destPort: failed }
      : m.includes("destination")
        ? { destHost: failed }
        : m.includes("listen") || m.includes("address")
          ? { bindAddr: failed }
          : m.includes("port")
            ? { bindPort: failed }
            : m.includes("name")
              ? { label: failed }
              : { form: failed };
    void focusFirstInvalid(editor);
  }
}

const hostLabel = (id: string) => library.host(id)?.label ?? "deleted host";
const groups = $derived.by(() => {
  const by = new Map<string, Tunnel[]>();
  for (const t of tunnels.list) by.set(t.hostId, [...(by.get(t.hostId) ?? []), t]);
  return [...by.entries()];
});
const hostName = $derived(library.host(hostId)?.label ?? "host");
</script>

<Drawer title="Tunnels" width="480px" onclose={() => shell.close()}>
  <div class="flex flex-col gap-3 pt-1">
    {#if tunnels.editing === null}
      <button type="button" class="btn-primary self-start" disabled={sshHosts.length === 0} onclick={() => edit(null)}>
        <Plus size={14} /> New tunnel
      </button>
      {#if sshHosts.length === 0}
        <p class="text-xs text-muted">Tunnels run through a saved SSH host. Add one from Hosts first.</p>
      {/if}
    {/if}

    {#if tunnels.editing !== null}
      <form class="panel flex flex-col gap-2" onsubmit={save} aria-label="Tunnel editor" novalidate bind:this={editor}>
        <div class="seg-row" role="radiogroup" aria-label="Tunnel kind" use:slidingPill>
          {#each KINDS as k (k.id)}
            <button
              type="button"
              role="radio"
              aria-checked={kind === k.id}
              class="seg"
              class:on={kind === k.id}
              onclick={() => (kind = k.id)}>{k.label}</button
            >
          {/each}
        </div>
        <p class="text-xs text-muted">{KINDS.find((k) => k.id === kind)?.hint}</p>
        <!-- Live diagram of where traffic goes. -->
        <p class="diagram" aria-label="Route">
          {#if kind === "local"}
            <span>this computer :{bindPort || "auto"}</span> → <span>{hostName}</span> →
            <span>{destHost || "?"}:{destPort || "?"}</span>
          {:else if kind === "remote"}
            <span>{hostName} :{bindPort || "auto"}</span> → <span>this computer</span> →
            <span>{destHost || "?"}:{destPort || "?"}</span>
          {:else}
            <span>apps → SOCKS :{bindPort || "auto"}</span> → <span>{hostName}</span> → <span>anywhere</span>
          {/if}
        </p>
        <label class="field">
          <span class="label-caps text-faint">Through host</span>
          <select class="input" bind:value={hostId} aria-label="Tunnel host" aria-invalid={!!errors.hostId} onchange={fix("hostId")}>
            {#each sshHosts as h (h.id)}<option value={h.id}>{h.label}</option>{/each}
          </select>
          <FieldError message={errors.hostId} />
        </label>
        <div class="grid grid-cols-[1fr_96px] gap-2">
          <label class="field">
            <span class="label-caps text-faint">{kind === "remote" ? "Listen on (host)" : "Listen on"}</span>
            <input
              class="input font-mono text-xs"
              bind:value={bindAddr}
              aria-label="Bind address"
              spellcheck="false"
              aria-invalid={!!errors.bindAddr}
              oninput={fix("bindAddr")}
            />
          </label>
          <label class="field">
            <span class="label-caps text-faint">Port</span>
            <input
              class="input tabular"
              bind:value={bindPort}
              onblur={checkPort}
              inputmode="numeric"
              placeholder="auto"
              aria-label="Bind port"
              aria-invalid={!!errors.bindPort || portBusy}
              oninput={fix("bindPort")}
            />
          </label>
        </div>
        <FieldError message={errors.bindAddr} />
        <FieldError message={errors.bindPort} />
        {#if kind !== "remote"}
          <div class="flex items-center gap-2 text-xs">
            <button type="button" class="link" onclick={pickPort}>Pick a free port</button>
            {#if portBusy && !errors.bindPort}<span class="text-danger" role="alert">That port is in use here.</span>{/if}
          </div>
        {/if}
        {#if exposed(bindAddr)}
          <p class="warn text-xs" role="note">
            {bindAddr} accepts connections from other computers on your network, not just this one.
          </p>
        {/if}
        {#if kind !== "dynamic"}
          <div class="grid grid-cols-[1fr_96px] gap-2">
            <label class="field">
              <span class="label-caps text-faint">
                {kind === "local" ? "Destination (as the host sees it)" : "Destination (as this computer sees it)"}
              </span>
              <input
                class="input font-mono text-xs"
                bind:value={destHost}
                aria-label="Destination host"
                spellcheck="false"
                aria-invalid={!!errors.destHost}
                oninput={fix("destHost")}
              />
            </label>
            <label class="field">
              <span class="label-caps text-faint">Port</span>
              <input
                class="input tabular"
                bind:value={destPort}
                inputmode="numeric"
                aria-label="Destination port"
                aria-invalid={!!errors.destPort}
                oninput={fix("destPort")}
              />
            </label>
          </div>
          <FieldError message={errors.destHost} />
          <FieldError message={errors.destPort} />
        {/if}
        <label class="field">
          <span class="label-caps text-faint">Name (optional)</span>
          <input class="input" bind:value={label} placeholder="Postgres" aria-label="Tunnel name" aria-invalid={!!errors.label} oninput={fix("label")} />
          <FieldError message={errors.label} />
        </label>
        <label class="check text-sm"><input type="checkbox" bind:checked={autoStart} /> Start when Hopr unlocks</label>
        <FieldError message={errors.form} />
        <div class="flex justify-end gap-2">
          <button type="button" class="btn-ghost" onclick={() => (tunnels.editing = null)}>Cancel</button>
          <button type="submit" class="btn-primary">Save</button>
        </div>
      </form>
    {/if}

    {#if tunnels.list.length === 0 && tunnels.editing === null}
      <p class="py-6 text-center text-sm text-muted">
        No tunnels yet. Forward a database port, expose a local dev server to a host, or browse through a host with a
        SOCKS proxy.
      </p>
    {/if}

    {#each groups as [hid, list] (hid)}
      <section aria-label="Tunnels via {hostLabel(hid)}">
        <h3 class="label-caps mb-1 text-faint">{hostLabel(hid)}</h3>
        {#each list as t, i (t.id)}
          {@const s = tunnels.statusOf(t.id)}
          {@const on = s.state === "running"}
          <!-- A running tunnel has light moving along its border, like traffic. -->
          <div
            class="row"
            class:on
            class:beam-border={on}
            data-tunnel={t.label || describe(t, null)}
            data-state={s.state}
            in:blurFade={{ delay: stagger(i), y: 6, blur: 0, duration: 200 }}
          >
            <button
              type="button"
              role="switch"
              class="switch"
              aria-checked={tunnels.running(t.id)}
              aria-label="{tunnels.running(t.id) ? 'Stop' : 'Start'} {t.label || describe(t, null)}"
              onclick={() => tunnels.toggle(t.id)}><span></span></button
            >
            <span class="badge">{t.kind === "local" ? "L" : t.kind === "remote" ? "R" : "D"}</span>
            <div class="min-w-0 flex-1">
              <span class="block truncate text-sm text-fg">{t.label || describe(t, s.boundPort)}</span>
              <span class="block truncate font-mono text-xs text-muted">
                {t.label ? describe(t, s.boundPort) : ""}
                {#if s.state === "starting"}connecting…{/if}
                {#if s.state === "failed"}<span class="text-danger">{s.error}</span>{/if}
                {#if on}↑{rate(s.upRate)} ↓{rate(s.downRate)} · {s.active} open{/if}
              </span>
            </div>
            {#if on && (tunnels.history[t.id]?.length ?? 0) > 1}
              <svg class="spark" viewBox="0 0 60 18" aria-hidden="true">
                <polyline points={sparkline(tunnels.history[t.id] ?? [], 60, 18)} />
              </svg>
            {/if}
            <button type="button" class="act" aria-label="Edit tunnel" onclick={() => edit(t)}>
              <PencilSimple size={14} />
            </button>
            <button type="button" class="act" aria-label="Delete tunnel" onclick={() => tunnels.remove(t.id)}>
              <Trash size={14} />
            </button>
          </div>
        {/each}
      </section>
    {/each}
  </div>
</Drawer>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 4px;
    border-bottom: 1px solid var(--border);
  }
  .badge {
    width: 18px;
    font-family: var(--font-mono);
    font-size: 11px;
    text-align: center;
    color: var(--text-muted);
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .switch {
    position: relative;
    width: 28px;
    height: 16px;
    flex-shrink: 0;
    background: var(--surface-3);
    border-radius: 999px;
    transition: background var(--dur-fast) var(--ease-out);
  }
  .switch span {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 12px;
    height: 12px;
    background: var(--text-muted);
    border-radius: 50%;
    transition: transform var(--dur-fast) var(--ease-out);
  }
  .switch[aria-checked="true"] {
    background: color-mix(in oklab, var(--success) 45%, transparent);
  }
  .switch[aria-checked="true"] span {
    background: var(--success);
    transform: translateX(12px);
  }
  .row.on .switch {
    box-shadow: 0 0 6px color-mix(in oklab, var(--success) 60%, transparent);
  }
  .spark {
    width: 60px;
    height: 18px;
  }
  /* The traffic line is uncovered left to right when it first appears. */
  .spark {
    animation: spark-in var(--dur-story) var(--ease-out) 1 backwards;
  }
  @keyframes spark-in {
    from {
      clip-path: inset(0 100% 0 0);
    }
  }
  .spark polyline {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.2;
  }
  .act {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    color: var(--text-muted);
    border-radius: 4px;
  }
  .act:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .diagram {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-muted);
  }
  .diagram span {
    padding: 1px 4px;
    color: var(--text);
    background: var(--surface-2);
    border-radius: 3px;
  }
  .seg-row {
    display: flex;
    padding: 2px;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .seg {
    flex: 1;
    padding: 4px 0;
    font-size: 12px;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
  }
  .seg.on {
    color: var(--text);
    background: var(--surface-3);
  }
  .link {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .warn {
    padding: 6px 8px;
    background: color-mix(in oklab, var(--warning) 14%, transparent);
    border-left: 2px solid var(--warning);
    border-radius: var(--radius-sm);
  }
</style>
