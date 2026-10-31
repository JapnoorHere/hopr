<script lang="ts">
/**
 * First run, step by step. Per profile: look, name (if unnamed), master password, recovery
 * key, hosts. Once per computer: the welcome before and the shortcuts after.
 */
import {
  ArrowLeft,
  ArrowRight,
  Check,
  Copy,
  DownloadSimple,
  Fingerprint,
  Palette,
  ShieldCheck,
  Sparkle,
  UserCircle,
} from "phosphor-svelte";
import { library } from "$features/hosts/library.svelte";
import ProfileAvatar from "$features/profiles/ProfileAvatar.svelte";
import { profiles } from "$features/profiles/profiles.svelte";
import { appearance } from "$lib/design/appearance.svelte";
import { builtIn, PROFILE_COLORS, type ThemeId, themeStyle } from "$lib/design/themes";
import HoprMark from "$lib/icons/HoprMark.svelte";
import { commands, inTauri, type Profile } from "$lib/ipc";
import { blurFade } from "$lib/motion";
import { readText, writeText } from "$lib/storage";
import FieldError from "$lib/ui/FieldError.svelte";
import { focusFirstInvalid } from "$lib/ui/form";
import Kbd from "$lib/ui/Kbd.svelte";
import { toast } from "$lib/ui/toast.svelte";
import { passwordStrength } from "./strength";
import { vault, wrap } from "./vault.svelte";

type Step = "welcome" | "look" | "profile" | "secure" | "recovery" | "hosts" | "ready";

/** Device-wide (not per profile): the welcome and the tips show only the first time. */
const SEEN_KEY = "hopr.onboarded";
const firstOnDevice = readText(SEEN_KEY) !== "1";
const needsName = !profiles.active?.name;
// The vault already exists (Hopr reloaded while the recovery key was up): carry on from hosts.
const resumed = !!vault.status?.exists;

const steps: Step[] = [
  ...(firstOnDevice ? (["welcome"] as const) : []),
  "look",
  ...(needsName ? (["profile"] as const) : []),
  "secure",
  "recovery",
  "hosts",
  ...(firstOnDevice ? (["ready"] as const) : []),
];
let at = $state(resumed ? steps.indexOf("hosts") : 0);
let dir = $state(1);
const step = $derived(steps[at] as Step);
/** Past the password, there's no going back to change it here (Settings can). */
const locked = $derived(steps.indexOf("recovery") <= at);

const LABEL: Record<Step, string> = {
  welcome: "Welcome",
  look: "Look",
  profile: "Profile",
  secure: "Secure",
  recovery: "Recovery",
  hosts: "Hosts",
  ready: "Ready",
};

function go(by: number) {
  dir = by;
  at = Math.min(steps.length - 1, Math.max(0, at + by));
}

// ── Look ──────────────────────────────────────────────────────────────────────────
const PICKS: ThemeId[] = [
  "ember",
  "graphite",
  "tokyo-night",
  "catppuccin-mocha",
  "nord",
  "rose-pine",
  "paper",
  "catppuccin-latte",
];

// ── Profile ───────────────────────────────────────────────────────────────────────
let name = $state(profiles.active?.name ?? "");
let color = $state<string>(profiles.active?.color ?? PROFILE_COLORS[0]);
let nameError = $state<string | null>(null);
const preview = $derived<Profile>({
  id: "preview",
  name: name.trim() || "?",
  color,
  createdAt: 0,
  look: null,
});

async function saveProfile() {
  nameError = null;
  if (!name.trim()) {
    nameError = "Give this profile a name";
    return;
  }
  if (inTauri) {
    const r = await profiles.update(profiles.current, { name, color });
    if (!r.ok) {
      nameError = r.error.message;
      return;
    }
  }
  go(1);
}

// ── Secure ────────────────────────────────────────────────────────────────────────
let password = $state("");
let confirm = $state("");
let unlockEasy = $state(true);
let busy = $state(false);
let errors = $state<{ password?: string; confirm?: string; form?: string }>({});
let form = $state<HTMLFormElement>();
let recoveryKey = $state("");
const strength = $derived(passwordStrength(password));
const hello = $derived(!!vault.status?.biometricAvailable);

async function create() {
  errors = {};
  if (!password) errors.password = "Choose a master password";
  else if (password.length < 8) errors.password = "Use at least 8 characters";
  else if (!confirm) errors.confirm = "Type the password again to confirm it";
  else if (password !== confirm) errors.confirm = "The passwords don't match";
  if (errors.password || errors.confirm) return focusFirstInvalid(form);
  busy = true;
  const res = await vault.create(password);
  if (!res.ok) {
    busy = false;
    errors = { form: res.error.message };
    return;
  }
  recoveryKey = res.data;
  // Quick unlock next time: Windows Hello where it's set up, otherwise remember this device.
  if (unlockEasy) {
    const quick = hello ? await vault.setBiometric(true) : await vault.unlock(password, true);
    if (!quick.ok)
      toast.info("Quick unlock isn't available; you can turn it on in Settings › Security");
  }
  password = "";
  confirm = "";
  busy = false;
  go(1);
}

// ── Recovery ──────────────────────────────────────────────────────────────────────
let copied = $state(false);
let saved = $state(false);

async function copyKey() {
  await navigator.clipboard.writeText(recoveryKey);
  copied = true;
  toast.success("Recovery key copied");
  setTimeout(() => (copied = false), 1500);
}

// ── Hosts ─────────────────────────────────────────────────────────────────────────
let found = $state<string[] | null>(null);
let importing = $state(false);
let imported = $state<number | null>(null);

$effect(() => {
  if (step !== "hosts" || found !== null) return;
  if (!inTauri) {
    found = [];
    return;
  }
  void commands.importSshConfigPreview().then((r) => {
    const res = wrap(r);
    found = res.ok ? res.data.hosts.map((h) => h.alias) : [];
  });
});

async function importAll() {
  if (!found?.length) return;
  importing = true;
  const res = wrap(await commands.importSshConfigApply(found));
  importing = false;
  if (!res.ok) {
    toast.error(res.error.message);
    return;
  }
  imported = res.data.hosts;
  await library.refresh();
}

// ── Done ──────────────────────────────────────────────────────────────────────────
function finish() {
  try {
    writeText(SEEN_KEY, "1");
  } catch {
    // Not remembered: the welcome shows again next time, which is harmless.
  }
  recoveryKey = "";
  vault.showRecovery = false;
}

function next() {
  if (step === "profile") void saveProfile();
  else if (step === "secure") void create();
  else if (step === "recovery" && !saved) return;
  else if (at === steps.length - 1) finish();
  else go(1);
}

const nextLabel = $derived(
  step === "welcome"
    ? "Get started"
    : step === "secure"
      ? busy
        ? "Creating your vault…"
        : "Create vault"
      : at === steps.length - 1
        ? "Open Hopr"
        : step === "hosts" && imported === null && found?.length
          ? "Skip for now"
          : "Continue",
);
</script>

<div class="stage">
  <div class="card">
    <!-- Progress: one segment per step, labels underneath. -->
    <ol class="progress" aria-label="Setup steps">
      {#each steps as s, i (s)}
        <li class:done={i < at} class:now={i === at} aria-current={i === at ? "step" : undefined}>
          <span class="bar"></span>
          <span class="lbl">{LABEL[s]}</span>
        </li>
      {/each}
    </ol>

    <form
      class="body"
      novalidate
      bind:this={form}
      onsubmit={(e) => {
        e.preventDefault();
        next();
      }}
    >
      {#key step}
        <div class="pane" in:blurFade={{ x: 18 * dir, y: 0, blur: 3, duration: 260 }}>
          {#if step === "welcome"}
            <div class="hero">
              <HoprMark size={64} animated />
              <h1>Welcome to Hopr</h1>
              <p>
                A fast, private SSH client. Hop through jump hosts, keep your keys encrypted, and land in a terminal
                that feels at home.
              </p>
              <ul class="perks">
                <li><ShieldCheck size={16} /> Everything stays encrypted on this computer</li>
                <li><Sparkle size={16} /> Themes, split panes and smart copy and paste</li>
                <li><DownloadSimple size={16} /> Bring your ~/.ssh/config hosts in one click</li>
              </ul>
            </div>
          {:else if step === "look"}
            <header><Palette size={20} /><div><h2>Pick a look</h2><p>It changes as you click. More themes, and your own, in Settings.</p></div></header>
            <div class="themes" role="radiogroup" aria-label="Theme">
              {#each PICKS as id (id)}
                {@const th = builtIn(id)}
                <button
                  type="button"
                  role="radio"
                  aria-checked={appearance.current.theme === id}
                  class="theme"
                  class:on={appearance.current.theme === id}
                  style={themeStyle(th)}
                  onclick={() => appearance.update({ theme: id })}
                >
                  <span class="swatch">
                    {#each [th.ansi.red, th.ansi.yellow, th.ansi.green, th.ansi.blue] as c, i (i)}<i style:background={c}></i>{/each}
                    <b></b>
                  </span>
                  <span class="tname">{th.name}</span>
                </button>
              {/each}
            </div>
          {:else if step === "profile"}
            <header><UserCircle size={20} /><div><h2>Name this profile</h2><p>Profiles keep separate hosts, keys and passwords. You can add more later.</p></div></header>
            <div class="profile">
              <ProfileAvatar profile={preview} size={56} />
              <div class="flex-1">
                <label class="field">
                  <span class="label-caps text-faint">Name</span>
                  <!-- svelte-ignore a11y_autofocus -->
                  <input class="input" bind:value={name} maxlength="40" placeholder="Name this profile" autofocus aria-invalid={!!nameError} oninput={() => (nameError = null)} />
                </label>
                {#if nameError}<div class="mt-1.5"><FieldError message={nameError} /></div>{/if}
              </div>
            </div>
            <div class="colours" role="radiogroup" aria-label="Colour">
              {#each PROFILE_COLORS as c (c)}
                <button type="button" role="radio" aria-checked={color === c} aria-label="Colour {c}" class="dot" class:on={color === c} style:background={c} onclick={() => (color = c)}>
                  {#if color === c}<Check size={12} weight="bold" />{/if}
                </button>
              {/each}
            </div>
          {:else if step === "secure"}
            <header><ShieldCheck size={20} /><div><h2>Lock it with a master password</h2><p>It encrypts your hosts, passwords and keys. Hopr never sees it or sends it anywhere.</p></div></header>
            <label class="field">
              <span class="label-caps text-faint">Master password</span>
              <!-- svelte-ignore a11y_autofocus -->
              <input class="input" type="password" bind:value={password} autocomplete="new-password" autofocus aria-invalid={!!errors.password} oninput={() => (errors.password = undefined)} />
              <FieldError message={errors.password} />
            </label>
            {#if password}
              <div class="flex items-center gap-2" aria-live="polite">
                <div class="flex flex-1 gap-1">
                  {#each [1, 2, 3] as n (n)}<span class="meter" class:on={strength.score >= n} data-score={strength.score}></span>{/each}
                </div>
                <span class="text-xs text-muted">{strength.label}</span>
              </div>
            {/if}
            <label class="field">
              <span class="label-caps text-faint">Confirm</span>
              <input class="input" type="password" bind:value={confirm} autocomplete="new-password" aria-invalid={!!errors.confirm} oninput={() => (errors.confirm = undefined)} />
              <FieldError message={errors.confirm} />
            </label>
            <label class="check">
              <input type="checkbox" bind:checked={unlockEasy} />
              {#if hello}<Fingerprint size={15} /> Unlock with Windows Hello{:else}Remember this device (no password on this computer){/if}
            </label>
            <FieldError message={errors.form} />
          {:else if step === "recovery"}
            <header><ShieldCheck size={20} class="text-success" /><div><h2>Save your recovery key</h2><p>If you forget your password, this key is the only way back in. Hopr can't recover it for you.</p></div></header>
            <div class="key">
              <code class="reveal select-text" data-testid="recovery-key">{recoveryKey || "Shown once, when the vault was created."}</code>
              {#if recoveryKey}
                <button type="button" class="btn-ghost" onclick={copyKey}>
                  {#if copied}<Check size={14} /> Copied{:else}<Copy size={14} /> Copy{/if}
                </button>
              {/if}
            </div>
            <label class="check"><input type="checkbox" bind:checked={saved} /> I've saved my recovery key somewhere safe</label>
          {:else if step === "hosts"}
            <header><DownloadSimple size={20} /><div><h2>Bring your hosts</h2><p>Hopr can import the hosts in your ~/.ssh/config, with their users, ports and jump hosts.</p></div></header>
            {#if found === null}
              <p class="text-sm text-muted">Looking for ~/.ssh/config…</p>
            {:else if imported !== null}
              <div class="ok"><Check size={18} /> Imported {imported} host{imported === 1 ? "" : "s"}.</div>
            {:else if found.length}
              <div class="found">
                {#each found.slice(0, 8) as a (a)}<span class="chip">{a}</span>{/each}
                {#if found.length > 8}<span class="chip more">+{found.length - 8} more</span>{/if}
              </div>
              <button type="button" class="btn-primary self-start" disabled={importing} onclick={importAll}>
                <DownloadSimple size={14} /> {importing ? "Importing…" : `Import ${found.length} host${found.length === 1 ? "" : "s"}`}
              </button>
            {:else}
              <p class="text-sm text-muted">No ~/.ssh/config found. Add hosts any time from the Hosts screen, or import from other apps there.</p>
            {/if}
          {:else if step === "ready"}
            <div class="hero">
              <HoprMark size={48} animated />
              <h1>You're all set</h1>
              <p>A few keys worth knowing:</p>
              <ul class="keys">
                <li><Kbd keys="Ctrl+Shift+P" /> Command palette: every action, searchable</li>
                <li><Kbd keys="Ctrl+Shift+T" /> New tab</li>
                <li><Kbd keys="Ctrl+Shift+L" /> Lock Hopr</li>
              </ul>
            </div>
          {/if}
        </div>
      {/key}

      <footer>
        {#if at > 0 && !locked && !resumed}
          <button type="button" class="btn-ghost" onclick={() => go(-1)}><ArrowLeft size={14} /> Back</button>
        {/if}
        <span class="flex-1"></span>
        <span class="count">{at + 1} / {steps.length}</span>
        <button type="submit" class="btn-primary go" disabled={busy || (step === "recovery" && !saved)}>
          {nextLabel}
          {#if step !== "secure"}<ArrowRight size={14} />{/if}
        </button>
      </footer>
    </form>
  </div>
</div>

<style>
  .stage {
    display: grid;
    place-items: center;
    width: 100%;
    padding: 32px 20px;
    overflow: auto;
  }
  .card {
    display: flex;
    flex-direction: column;
    width: min(560px, 100%);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-pop);
  }
  .progress {
    display: flex;
    gap: 6px;
    padding: 18px 22px 0;
  }
  .progress li {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .bar {
    height: 3px;
    border-radius: 99px;
    background: var(--surface-3);
    transition: background var(--dur-slow) var(--ease-out);
  }
  .done .bar,
  .now .bar {
    background: var(--accent);
  }
  .lbl {
    overflow: hidden;
    font-size: 10.5px;
    letter-spacing: 0.04em;
    color: var(--text-faint);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .now .lbl {
    color: var(--text);
  }
  .body {
    display: flex;
    flex-direction: column;
  }
  .pane {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 300px;
    padding: 24px 24px 8px;
  }
  header {
    display: flex;
    gap: 12px;
    color: var(--accent);
  }
  header h2 {
    font-size: 17px;
    font-weight: 600;
    color: var(--text);
  }
  header p {
    margin-top: 2px;
    font-size: 13px;
    color: var(--text-muted);
  }
  .hero {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    text-align: center;
  }
  .hero h1 {
    margin-top: 8px;
    font-size: 24px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  .hero p {
    max-width: 400px;
    font-size: 13.5px;
    color: var(--text-muted);
  }
  .perks,
  .keys {
    display: grid;
    gap: 8px;
    margin-top: 10px;
    text-align: left;
  }
  .perks li,
  .keys li {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    color: var(--text-muted);
  }
  .perks :global(svg) {
    color: var(--accent);
  }
  .themes {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 10px;
  }
  .theme {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px;
    text-align: left;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    transition:
      border-color var(--dur-fast) var(--ease-out),
      transform var(--dur) var(--ease-spring);
  }
  .theme:hover {
    transform: translateY(-2px);
  }
  .theme.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .swatch {
    display: flex;
    align-items: center;
    gap: 3px;
    height: 34px;
    padding: 0 8px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  .swatch i {
    width: 7px;
    height: 7px;
    border-radius: 99px;
  }
  .swatch b {
    width: 6px;
    height: 18px;
    margin-left: auto;
    border-radius: 3px;
    background: var(--accent);
  }
  .tname {
    overflow: hidden;
    font-size: 12px;
    font-weight: 500;
    color: var(--text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .profile {
    display: flex;
    align-items: center;
    gap: 16px;
  }
  .colours {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .dot {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    color: var(--bg);
    border-radius: 99px;
    box-shadow: inset 0 0 0 1px var(--border);
    transition: transform var(--dur-fast) var(--ease-spring);
  }
  .dot:hover {
    transform: scale(1.1);
  }
  .dot.on {
    box-shadow:
      0 0 0 2px var(--surface-1),
      0 0 0 4px var(--text);
  }
  .meter {
    height: 4px;
    flex: 1;
    border-radius: 2px;
    background: var(--surface-3);
    transition: background var(--dur-slow) var(--ease-out);
  }
  .meter.on[data-score="1"] {
    background: var(--danger);
  }
  .meter.on[data-score="2"] {
    background: var(--warning);
  }
  .meter.on[data-score="3"] {
    background: var(--success);
  }
  .key {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px;
    background: var(--bg);
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius);
  }
  .key code {
    flex: 1;
    font-family: var(--font-mono);
    font-size: 15px;
    letter-spacing: 0.04em;
    word-break: break-all;
    color: var(--text);
  }
  /* The recovery key is uncovered left to right once, so it reads as something new. */
  .reveal {
    animation: key-reveal 700ms var(--ease-out) 1 both;
  }
  @keyframes key-reveal {
    from {
      clip-path: inset(0 100% 0 0);
      filter: blur(2px);
    }
  }
  :global(:root[data-motion="reduced"]) .reveal {
    animation: none;
  }
  .found {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    padding: 3px 9px;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-muted);
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: 99px;
  }
  .chip.more {
    font-family: var(--font-ui);
  }
  .ok {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 14px;
    font-size: 14px;
    color: var(--success);
    background: color-mix(in oklab, var(--success) 10%, transparent);
    border-radius: var(--radius);
  }
  footer {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 24px 20px;
  }
  .count {
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    color: var(--text-faint);
  }
  .go :global(svg) {
    transition: translate var(--dur) var(--ease-spring);
  }
  .go:hover :global(svg) {
    translate: 3px 0;
  }
</style>
