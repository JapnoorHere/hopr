<script lang="ts">
import { untrack } from "svelte";
import { shell } from "$features/shell/shell.state.svelte";
import {
  type AuthKind,
  commands,
  type Environment,
  type HostInput,
  inTauri,
  type Parity,
  type Protocol,
  type ProxyProtocol,
  type SecretChange,
  type SerialFlow,
} from "$lib/ipc";
import { blurFade, expand, slidingPill } from "$lib/motion";
import Drawer from "$lib/ui/Drawer.svelte";
import FieldError from "$lib/ui/FieldError.svelte";
import { type FieldErrors, focusFirstInvalid, hasErrors, parsePort } from "$lib/ui/form";
import { toast } from "$lib/ui/toast.svelte";
import { jumpCandidates } from "./chain";
import { library } from "./library.svelte";

let { id }: { id: string | null } = $props();

// svelte-ignore state_referenced_locally
const existing = id ? library.host(id) : undefined;

let label = $state(existing?.label ?? "");
let protocol = $state<Protocol>(existing?.protocol ?? "ssh");
let proxyCommand = $state(existing?.proxyCommand ?? "");
const serialCfg = existing?.serial;
let baud = $state(String(serialCfg?.baud ?? 9600));
let dataBits = $state(serialCfg?.dataBits ?? 8);
let parity = $state<Parity>(serialCfg?.parity ?? "none");
let stopBits = $state(serialCfg?.stopBits ?? 1);
let serialFlow = $state<SerialFlow>(serialCfg?.flow ?? "none");
let serialPorts = $state<string[]>([]);
let hostname = $state(existing?.hostname ?? "");
let port = $state(String(existing?.port ?? 22));
let username = $state(existing?.username ?? "");
let groupId = $state<string>(existing?.groupId ?? "");
let newGroup = $state("");
let tags = $state(existing?.tags.join(", ") ?? "");
let auth = $state<AuthKind>(existing?.auth ?? "password");
let keyId = $state<string>(existing?.keyId ?? library.keys[0]?.id ?? "");
let jumpHostId = $state<string>(existing?.jumpHostId ?? "");
let agentForwarding = $state(existing?.agentForwarding ?? false);
const jumpOptions = $derived(jumpCandidates(id, library.hosts));
let proxyProtocol = $state<ProxyProtocol | "">(existing?.proxy?.protocol ?? "");
let proxyHost = $state(existing?.proxy?.host ?? "");
let proxyPort = $state(String(existing?.proxy?.port ?? 1080));
let proxyUser = $state(existing?.proxy?.username ?? "");
let proxyPassword = $state("");
let clearProxyPassword = $state(false);
let password = $state("");
let clearPassword = $state(false);
let totp = $state("");
let clearTotp = $state(false);
let environment = $state<Environment>(existing?.environment ?? "none");
let favorite = $state(existing?.favorite ?? false);
let notes = $state(existing?.notes ?? "");
type Field =
  | "hostname"
  | "port"
  | "username"
  | "label"
  | "key"
  | "proxyCommand"
  | "proxyHost"
  | "proxyPort"
  | "proxyUser"
  | "newGroup"
  | "serial"
  | "notes";
let errors = $state<FieldErrors<Field>>({});
/** A save failure that isn't about one field. */
let error = $state<string | null>(null);
let busy = $state(false);
let form = $state<HTMLFormElement>();

// A key added from here (Generate / Import) gets selected; otherwise the first key, once
// there is one.
$effect(() => {
  const added = library.addedKeyId;
  const first = library.keys[0]?.id;
  untrack(() => {
    if (added && library.keys.some((k) => k.id === added)) {
      keyId = added;
      library.addedKeyId = null;
      errors.key = undefined;
    } else if ((!keyId || !library.keys.some((k) => k.id === keyId)) && first) {
      keyId = first;
    }
  });
});

/** Clears a field's error as soon as the user edits it. */
const fix = (f: Field) => () => {
  if (errors[f]) errors[f] = undefined;
};

function validate(): FieldErrors<Field> {
  const e: FieldErrors<Field> = {};
  const host = hostname.trim();
  if (protocol === "serial") {
    if (!host) e.hostname = "Enter the serial port, e.g. COM3 or /dev/ttyUSB0";
  } else {
    if (!host) e.hostname = "Enter the hostname or IP address";
    else if (/\s/.test(host)) e.hostname = "A hostname can't contain spaces";
    if (parsePort(port) === null) e.port = "Port must be between 1 and 65535";
    if (protocol === "ssh" && !username.trim()) e.username = "Enter the user to log in as";
  }
  if (username.length > 64) e.username = "Usernames are at most 64 characters";
  if (protocol === "ssh") {
    if (auth === "key" && !keyId) {
      e.key = library.keys.length
        ? "Choose the key to log in with"
        : "Add a key first: generate a new one, or import the one you use now";
    }
    const pc = proxyCommand.trim();
    if (pc && (proxyProtocol || jumpHostId))
      e.proxyCommand = "Use either a ProxyCommand or a jump host / proxy, not both";
    else if (pc.length > 1000) e.proxyCommand = "Keep the ProxyCommand under 1000 characters";
    if (proxyProtocol) {
      if (!proxyHost.trim()) e.proxyHost = "Enter the proxy's hostname or IP address";
      if (parsePort(proxyPort) === null) e.proxyPort = "Proxy port must be between 1 and 65535";
    }
  }
  if (groupId === "__new" && !newGroup.trim()) e.newGroup = "Name the new group";
  return e;
}

/** Puts an error from the backend next to the field it is about, when it names one. */
function fieldOf(message: string): Field | null {
  const m = message.toLowerCase();
  if (m.includes("proxycommand")) return "proxyCommand";
  if (m.includes("proxy port")) return "proxyPort";
  if (m.includes("proxy username")) return "proxyUser";
  if (m.includes("proxy")) return "proxyHost";
  if (m.includes("serial port") || m.includes("hostname")) return "hostname";
  if (m.includes("baud") || m.includes("data bits") || m.includes("stop bits")) return "serial";
  if (m.includes("username")) return "username";
  if (m.includes("port")) return "port";
  if (m.includes("label")) return "label";
  if (m.includes("key")) return "key";
  if (m.includes("notes")) return "notes";
  if (m.includes("group")) return "newGroup";
  return null;
}

const ENVS: { id: Environment; label: string }[] = [
  { id: "none", label: "None" },
  { id: "dev", label: "Dev" },
  { id: "staging", label: "Staging" },
  { id: "prod", label: "Prod" },
];
const PROTOCOLS: { id: Protocol; label: string }[] = [
  { id: "ssh", label: "SSH" },
  { id: "telnet", label: "Telnet" },
  { id: "serial", label: "Serial" },
];
const BAUDS = [1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];

/** Switching protocol moves the port to its usual default, unless it was changed. */
function pickProtocol(next: Protocol) {
  if (next === "telnet" && port === "22") port = "23";
  if (next === "ssh" && port === "23") port = "22";
  protocol = next;
  if (next === "serial" && inTauri) {
    void commands.serialPorts().then((r) => {
      if (r.status === "ok") serialPorts = r.data;
    });
  }
}

const AUTHS: { id: AuthKind; label: string }[] = [
  { id: "password", label: "Password" },
  { id: "key", label: "Key" },
  { id: "agent", label: "Agent" },
];

function secret(value: string, clear: boolean): SecretChange {
  if (value) return { action: "set", value };
  return clear ? { action: "clear" } : { action: "keep" };
}

async function save(connect: boolean) {
  error = null;
  errors = validate();
  if (hasErrors(errors)) return focusFirstInvalid(form);
  const portNum = protocol === "serial" ? 22 : (parsePort(port) ?? 22);
  const proxyPortNum = parsePort(proxyPort) ?? 1080;
  busy = true;
  let group: string | null = groupId || null;
  if (groupId === "__new") {
    const g = await library.createGroup(newGroup);
    if (!g.ok) {
      busy = false;
      errors = { newGroup: g.error.message };
      return focusFirstInvalid(form);
    }
    group = g.data.id;
  }
  const ssh = protocol === "ssh";
  const input: HostInput = {
    label,
    protocol,
    hostname,
    port: portNum,
    serial:
      protocol === "serial"
        ? { baud: Number(baud), dataBits, parity, stopBits, flow: serialFlow }
        : null,
    proxyCommand: ssh && proxyCommand.trim() ? proxyCommand : null,
    username,
    groupId: group,
    tags: tags.split(",").map((t) => t.trim()),
    auth,
    keyId: auth === "key" ? keyId || null : null,
    jumpHostId: ssh ? jumpHostId || null : null,
    agentForwarding,
    proxy:
      ssh && proxyProtocol
        ? {
            protocol: proxyProtocol,
            host: proxyHost,
            port: proxyPortNum,
            username: proxyUser || null,
          }
        : null,
    proxyPassword: secret(proxyPassword, clearProxyPassword),
    favorite,
    environment,
    notes,
    password: secret(password, clearPassword),
    totpSeed: secret(totp.replace(/\s+/g, ""), clearTotp),
  };
  const res = await library.save(id, input);
  busy = false;
  if (!res.ok) {
    const field = fieldOf(res.error.message);
    if (field) {
      errors = { [field]: res.error.message };
      return focusFirstInvalid(form);
    }
    error = res.error.message;
    toast.error(res.error.message);
    return;
  }
  toast.success(id ? `Saved “${res.data.label}”` : `Added “${res.data.label}”`);
  password = "";
  totp = "";
  proxyPassword = "";
  library.editing = null;
  if (connect) {
    const h = res.data;
    library.connect(h);
  }
}

async function remove() {
  if (!id) return;
  const res = await library.remove([id]);
  if (res.ok) library.editing = null;
  else toast.error(res.error.message);
}

function onsubmit(e: SubmitEvent) {
  e.preventDefault();
  void save(false);
}
</script>

<Drawer title={existing ? `Edit ${existing.label}` : "New host"} pinned={false} onclose={() => (library.editing = null)}>
  <form class="flex flex-col gap-4 pt-2" {onsubmit} autocomplete="off" novalidate bind:this={form}>
    <div class="seg-row" role="radiogroup" aria-label="Protocol" use:slidingPill>
      {#each PROTOCOLS as pr (pr.id)}
        <button
          type="button"
          role="radio"
          aria-checked={protocol === pr.id}
          class="seg"
          class:on={protocol === pr.id}
          onclick={() => pickProtocol(pr.id)}>{pr.label}</button
        >
      {/each}
    </div>
    {#if protocol === "serial"}
      <label class="field">
        <span class="label-caps text-faint">Serial port</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="input font-mono"
          bind:value={hostname}
          list="hopr-serial-ports"
          placeholder="COM3 or /dev/ttyUSB0"
          spellcheck="false"
          aria-label="Serial port"
          autofocus
          aria-invalid={!!errors.hostname}
          oninput={fix("hostname")}
        />
        <FieldError message={errors.hostname} />
        <datalist id="hopr-serial-ports">
          {#each serialPorts as sp (sp)}<option value={sp}></option>{/each}
        </datalist>
        <span class="text-xs text-faint">
          {serialPorts.length ? `Found: ${serialPorts.join(", ")}` : "No serial ports found right now."}
        </span>
      </label>
      <FieldError message={errors.serial} />
      <div class="grid grid-cols-2 gap-2">
        <label class="field">
          <span class="label-caps text-faint">Baud</span>
          <select class="input" bind:value={baud} aria-label="Baud rate">
            {#each BAUDS as b (b)}<option value={String(b)}>{b}</option>{/each}
          </select>
        </label>
        <label class="field">
          <span class="label-caps text-faint">Data bits</span>
          <select class="input" bind:value={dataBits} aria-label="Data bits">
            {#each [8, 7, 6, 5] as d (d)}<option value={d}>{d}</option>{/each}
          </select>
        </label>
        <label class="field">
          <span class="label-caps text-faint">Parity</span>
          <select class="input" bind:value={parity} aria-label="Parity">
            <option value="none">None</option>
            <option value="odd">Odd</option>
            <option value="even">Even</option>
          </select>
        </label>
        <label class="field">
          <span class="label-caps text-faint">Stop bits</span>
          <select class="input" bind:value={stopBits} aria-label="Stop bits">
            <option value={1}>1</option>
            <option value={2}>2</option>
          </select>
        </label>
      </div>
      <label class="field">
        <span class="label-caps text-faint">Flow control</span>
        <select class="input" bind:value={serialFlow} aria-label="Flow control">
          <option value="none">None</option>
          <option value="software">XON/XOFF</option>
          <option value="hardware">RTS/CTS</option>
        </select>
      </label>
    {:else}
      <div class="field">
        <div class="grid grid-cols-[1fr_76px] gap-2">
          <label class="field">
            <span class="label-caps text-faint">Hostname or IP</span>
            <!-- svelte-ignore a11y_autofocus -->
            <input
              class="input"
              bind:value={hostname}
              placeholder="10.0.3.4"
              spellcheck="false"
              autofocus
              aria-invalid={!!errors.hostname}
              oninput={fix("hostname")}
            />
          </label>
          <label class="field">
            <span class="label-caps text-faint">Port</span>
            <input
              class="input tabular"
              bind:value={port}
              inputmode="numeric"
              aria-label="Port"
              aria-invalid={!!errors.port}
              oninput={fix("port")}
            />
          </label>
        </div>
        <FieldError message={errors.hostname} />
        <FieldError message={errors.port} />
      </div>
      <label class="field">
        <span class="label-caps text-faint">Username{protocol === "telnet" ? " (optional)" : ""}</span>
        <input
          class="input"
          bind:value={username}
          placeholder={protocol === "telnet" ? "You log in at the device's prompt" : "deploy"}
          spellcheck="false"
          aria-invalid={!!errors.username}
          oninput={fix("username")}
        />
        <FieldError message={errors.username} />
      </label>
    {/if}
    {#if protocol === "telnet"}
      <p class="warn text-xs" role="note" transition:expand>
        Telnet isn't encrypted: anyone on the network path can read what you type, including passwords. Use it for lab
        gear and devices that can't do SSH.
      </p>
    {/if}
    <label class="field">
      <span class="label-caps text-faint">Label</span>
      <input class="input" bind:value={label} placeholder={hostname || "prod-db"} aria-invalid={!!errors.label} oninput={fix("label")} />
      <FieldError message={errors.label} />
    </label>

    {#if protocol === "ssh"}
    <div class="field">
      <span class="label-caps text-faint">Authentication</span>
      <div class="seg-row" role="radiogroup" aria-label="Authentication" use:slidingPill>
        {#each AUTHS as a (a.id)}
          <button
            type="button"
            role="radio"
            aria-checked={auth === a.id}
            class="seg"
            class:on={auth === a.id}
            onclick={() => {
              auth = a.id;
              errors.key = undefined;
            }}>{a.label}</button
          >
        {/each}
      </div>
    </div>

    {#if auth === "password"}
      <div class="field" in:blurFade={{ y: 4, blur: 0, duration: 180 }}>
        <label class="field">
          <span class="label-caps text-faint">Password</span>
          <input
            class="input"
            type="password"
            bind:value={password}
            placeholder={existing?.hasPassword && !clearPassword ? "Saved (leave blank to keep)" : "Ask when connecting"}
          />
        </label>
        {#if existing?.hasPassword}
          <label class="check text-xs"><input type="checkbox" bind:checked={clearPassword} /> Forget saved password</label>
        {/if}
      </div>
    {:else if auth === "key"}
      <div class="field" in:blurFade={{ y: 4, blur: 0, duration: 180 }}>
        <span class="label-caps text-faint">Key</span>
        {#if library.keys.length}
          <select class="input" bind:value={keyId} aria-label="Key" aria-invalid={!!errors.key} onchange={fix("key")}>
            {#each library.keys as k (k.id)}
              <option value={k.id}>{k.label} ({k.algorithm})</option>
            {/each}
          </select>
          <FieldError message={errors.key} />
          <div class="flex flex-wrap gap-2">
            <button type="button" class="btn-secondary" onclick={() => shell.openKeys("generate")}>New key</button>
            <button type="button" class="btn-secondary" onclick={() => shell.openKeys("import")}>Import key</button>
            <button type="button" class="btn-ghost" onclick={() => shell.openKeys("list")}>Manage keys</button>
          </div>
        {:else}
          <div class="no-keys" data-invalid={!!errors.key}>
            <p class="text-sm text-fg">No SSH keys in Hopr yet</p>
            <p class="text-xs text-muted">
              Generate a new key here, or import the private key you already use (for example ~/.ssh/id_ed25519).
            </p>
            <div class="flex flex-wrap gap-2">
              <button type="button" class="btn-primary" onclick={() => shell.openKeys("generate")}>Generate key</button>
              <button type="button" class="btn-secondary" onclick={() => shell.openKeys("import")}>Import key</button>
            </div>
          </div>
          <FieldError message={errors.key} />
        {/if}
      </div>
    {:else}
      <p class="text-xs text-muted" in:blurFade={{ y: 4, blur: 0, duration: 180 }}>
        Uses keys loaded in your SSH agent (the Windows OpenSSH agent or Pageant; SSH_AUTH_SOCK elsewhere).
      </p>
    {/if}

    <div class="field">
      <label class="field">
        <span class="label-caps text-faint">Connect through</span>
        <select class="input" bind:value={jumpHostId}>
          <option value="">Directly</option>
          {#each jumpOptions as j (j.id)}
            <option value={j.id}>{j.label} ({j.username}@{j.hostname})</option>
          {/each}
        </select>
      </label>
      <span class="text-xs text-faint">A saved host to jump through first (like ProxyJump).</span>
    </div>
    <label class="field">
      <span class="label-caps text-faint">ProxyCommand (optional)</span>
      <input
        class="input font-mono text-xs"
        bind:value={proxyCommand}
        placeholder="cloudflared access ssh --hostname %h"
        spellcheck="false"
        aria-label="ProxyCommand"
        aria-invalid={!!errors.proxyCommand}
        oninput={fix("proxyCommand")}
      />
      <FieldError message={errors.proxyCommand} />
      <span class="text-xs text-faint">Runs locally; SSH goes over its input and output. %h host, %p port, %r user.</span>
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={agentForwarding} /> Forward my SSH agent to this host
    </label>

    <div class="field">
      <label class="field">
        <span class="label-caps text-faint">Proxy</span>
        <select class="input" bind:value={proxyProtocol} aria-label="Proxy">
          <option value="">None</option>
          <option value="socks5">SOCKS5</option>
          <option value="http">HTTP CONNECT</option>
        </select>
      </label>
      {#if proxyProtocol}
        <div class="flex flex-col gap-2" transition:expand>
        <div class="grid grid-cols-[1fr_76px] gap-2">
          <input
            class="input"
            bind:value={proxyHost}
            placeholder="proxy.corp"
            spellcheck="false"
            aria-label="Proxy host"
            aria-invalid={!!errors.proxyHost}
            oninput={fix("proxyHost")}
          />
          <input
            class="input tabular"
            bind:value={proxyPort}
            inputmode="numeric"
            aria-label="Proxy port"
            aria-invalid={!!errors.proxyPort}
            oninput={fix("proxyPort")}
          />
        </div>
        <FieldError message={errors.proxyHost} />
        <FieldError message={errors.proxyPort} />
        <div class="grid grid-cols-2 gap-2">
          <input class="input" bind:value={proxyUser} placeholder="Username (optional)" spellcheck="false" aria-label="Proxy username" aria-invalid={!!errors.proxyUser} oninput={fix("proxyUser")} />
          <input
            class="input"
            type="password"
            bind:value={proxyPassword}
            aria-label="Proxy password"
            placeholder={existing?.hasProxyPassword && !clearProxyPassword ? "Saved" : "Password"}
          />
        </div>
        <FieldError message={errors.proxyUser} />
        {#if existing?.hasProxyPassword}
          <label class="check text-xs"><input type="checkbox" bind:checked={clearProxyPassword} /> Forget proxy password</label>
        {/if}
        <span class="text-xs text-faint">
          {jumpHostId ? "With a jump host, the proxy of the first host in the chain is used." : "The proxy resolves the hostname, so names only it can see work."}
        </span>
        </div>
      {/if}
    </div>

    <div class="field">
      <label class="field">
        <span class="label-caps text-faint">TOTP seed (optional)</span>
        <input
          class="input font-mono"
          bind:value={totp}
          placeholder={existing?.hasTotp && !clearTotp ? "Saved (leave blank to keep)" : "Base32; fills verification codes"}
          spellcheck="false"
        />
      </label>
      {#if existing?.hasTotp}
        <label class="check text-xs"><input type="checkbox" bind:checked={clearTotp} /> Forget TOTP seed</label>
      {/if}
    </div>
    {/if}

    <div class="field">
      <span class="label-caps text-faint">Environment</span>
      <div class="seg-row" role="radiogroup" aria-label="Environment" use:slidingPill>
        {#each ENVS as env (env.id)}
          <button
            type="button"
            role="radio"
            aria-checked={environment === env.id}
            class="seg"
            class:on={environment === env.id}
            data-env={env.id}
            onclick={() => (environment = env.id)}>{env.label}</button
          >
        {/each}
      </div>
    </div>

    <div class="field">
      <label class="field">
        <span class="label-caps text-faint">Group</span>
        <select class="input" bind:value={groupId}>
          <option value="">No group</option>
          {#each library.groups as g (g.id)}
            <option value={g.id}>{g.name}</option>
          {/each}
          <option value="__new">New group…</option>
        </select>
      </label>
      {#if groupId === "__new"}
        <!-- svelte-ignore a11y_autofocus -->
        <input
          transition:expand
          class="input"
          bind:value={newGroup}
          placeholder="Group name"
          aria-label="New group name"
          autofocus
          aria-invalid={!!errors.newGroup}
          oninput={fix("newGroup")}
        />
        <FieldError message={errors.newGroup} />
      {/if}
    </div>
    <label class="field">
      <span class="label-caps text-faint">Tags</span>
      <input class="input" bind:value={tags} placeholder="db, eu-west" spellcheck="false" />
    </label>
    <label class="field">
      <span class="label-caps text-faint">Notes</span>
      <textarea class="input" bind:value={notes} rows="3" aria-invalid={!!errors.notes} oninput={fix("notes")}></textarea>
      <FieldError message={errors.notes} />
    </label>
    <label class="check"><input type="checkbox" bind:checked={favorite} /> Pin to Favorites</label>

    {#if hasErrors(errors)}
      <p class="text-xs text-danger">Fix the highlighted {Object.values(errors).filter(Boolean).length === 1 ? "field" : "fields"} above.</p>
    {:else if error}
      <FieldError message={error} />
    {/if}

    <div class="flex flex-wrap items-center gap-2">
      {#if existing}
        <button type="button" class="btn-danger" onclick={remove}>Delete</button>
      {/if}
      <span class="flex-1"></span>
      <button type="submit" class="btn-ghost" disabled={busy}>Save</button>
      <button type="button" class="btn-primary" disabled={busy} onclick={() => save(true)}>Save & connect</button>
    </div>
  </form>
</Drawer>

<style>
  .warn {
    padding: 6px 8px;
    color: var(--text);
    background: color-mix(in oklab, var(--warning) 14%, transparent);
    border-left: 2px solid var(--warning);
    border-radius: var(--radius-sm);
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
  .seg.on[data-env="prod"] {
    color: var(--danger);
  }
  .no-keys {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    background: var(--surface-1);
    border: 1px dashed var(--border);
    border-radius: var(--radius);
  }
  .no-keys[data-invalid="true"] {
    border-color: var(--danger);
    border-style: solid;
  }
</style>
