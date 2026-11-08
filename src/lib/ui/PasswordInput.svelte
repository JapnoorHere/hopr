<script lang="ts">
/**
 * A password field: lock icon, show/hide, a Caps Lock warning, and for new passwords a strength
 * bar or (on a confirm field) a tick once it matches. The text masks again when the field loses
 * focus. Other attributes (aria-*, autocomplete, autofocus, oninput…) go to the input.
 */
import { Check, Eye, EyeSlash, Key, LockSimple, Warning } from "phosphor-svelte";
import type { HTMLInputAttributes } from "svelte/elements";

interface Props extends Omit<HTMLInputAttributes, "type" | "value" | "class"> {
  value?: string;
  /** The leading icon: a lock for passwords, a key for key passphrases. */
  icon?: "lock" | "key" | "none";
  /** A strength reading for a new password (e.g. from passwordStrength). */
  meter?: { score: number; max: number; label: string } | null;
  /** A confirm field: whether it matches the first one. */
  matches?: boolean | null;
  /** Classes for the wrapper (layout: `min-w-0 flex-1`…). */
  class?: string;
}

let {
  value = $bindable(""),
  icon = "lock",
  meter = null,
  matches = null,
  class: cls = "",
  onkeydown,
  onkeyup,
  onblur,
  ...rest
}: Props = $props();

let shown = $state(false);
let caps = $state(false);

function readCaps(e: KeyboardEvent) {
  caps = e.getModifierState?.("CapsLock") ?? false;
}
</script>

<div class="pw {cls}">
  <div class="box" class:shown>
    {#if icon !== "none"}
      <span class="lead" aria-hidden="true">
        {#if icon === "key"}<Key size={14} />{:else}<LockSimple size={14} />{/if}
      </span>
    {/if}
    <input
      {...rest}
      type={shown ? "text" : "password"}
      bind:value
      spellcheck="false"
      autocapitalize="off"
      class:masked={!shown && value.length > 0}
      onkeydown={(e) => {
        readCaps(e);
        onkeydown?.(e);
      }}
      onkeyup={(e) => {
        readCaps(e);
        onkeyup?.(e);
      }}
      onblur={(e) => {
        shown = false;
        caps = false;
        onblur?.(e);
      }}
    />
    {#if matches !== null && value}
      <span class="match" class:ok={matches} title={matches ? "Matches" : "Doesn't match yet"} aria-hidden="true">
        {#if matches}<Check size={13} weight="bold" />{:else}<span class="dot"></span>{/if}
      </span>
    {/if}
    <button
      type="button"
      class="eye"
      aria-label={shown ? "Hide password" : "Show password"}
      aria-pressed={shown}
      title={shown ? "Hide" : "Show"}
      onmousedown={(e) => e.preventDefault()}
      onclick={() => (shown = !shown)}
    >
      {#if shown}<EyeSlash size={15} />{:else}<Eye size={15} />{/if}
    </button>
  </div>
  {#if caps}
    <p class="caps" role="status"><Warning size={12} weight="fill" /> Caps Lock is on</p>
  {/if}
  {#if meter && value}
    <div class="meter" aria-live="polite">
      <div class="bars">
        {#each Array.from({ length: meter.max }, (_, i) => i + 1) as n (n)}
          <span class:on={meter.score >= n} data-score={meter.score} data-max={meter.max}></span>
        {/each}
      </div>
      <span class="label">{meter.label}</span>
    </div>
  {/if}
</div>

<style>
  .pw {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .box {
    display: flex;
    align-items: center;
    min-width: 0;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    transition:
      border-color var(--dur-fast) var(--ease-out),
      box-shadow var(--dur-fast) var(--ease-out),
      background var(--dur-fast) var(--ease-out);
  }
  .box:hover {
    border-color: var(--border-strong, var(--text-faint));
  }
  .box:focus-within {
    border-color: var(--accent);
    background: var(--bg);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 18%, transparent);
  }
  .box:has(input[aria-invalid="true"]) {
    border-color: var(--danger);
  }
  .box:has(input[aria-invalid="true"]):focus-within {
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--danger) 22%, transparent);
  }
  .box:has(input:disabled) {
    opacity: 0.6;
  }
  .lead {
    display: grid;
    place-items: center;
    padding-left: 10px;
    color: var(--text-faint);
    transition: color var(--dur-fast) var(--ease-out);
  }
  .box:focus-within .lead {
    color: var(--accent);
  }
  input {
    flex: 1;
    min-width: 0;
    padding: 7px 10px;
    font-size: 13px;
    color: var(--text);
    background: transparent;
    border: 0;
    outline: none;
    user-select: text;
  }
  input::placeholder {
    color: var(--text-faint);
    letter-spacing: normal;
  }
  /* Masked: even, roomier dots. */
  input.masked {
    font-family: var(--font-mono);
    letter-spacing: 0.22em;
  }
  .shown input {
    font-family: var(--font-mono);
  }
  /* Edge/WebView2 draws its own eye on password fields: ours replaces it. */
  input::-ms-reveal,
  input::-ms-clear {
    display: none;
  }
  .match {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    margin-right: 2px;
    border-radius: 50%;
    color: var(--text-faint);
  }
  .match.ok {
    color: var(--accent-fg);
    background: var(--success);
  }
  .match .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-faint);
  }
  .eye {
    display: grid;
    place-items: center;
    align-self: stretch;
    width: 32px;
    flex-shrink: 0;
    color: var(--text-faint);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    transition: color var(--dur-fast) var(--ease-out);
  }
  .eye:hover,
  .shown .eye {
    color: var(--text);
  }
  .eye:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .caps {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--warning);
  }
  .meter {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .bars {
    display: flex;
    flex: 1;
    gap: 4px;
  }
  .bars span {
    height: 4px;
    flex: 1;
    border-radius: 2px;
    background: var(--surface-3);
    transition: background var(--dur-slow) var(--ease-out);
  }
  .bars span.on {
    background: var(--warning);
  }
  .bars span.on[data-score="1"] {
    background: var(--danger);
  }
  .bars span.on:is([data-score="3"][data-max="3"], [data-score="4"][data-max="4"]) {
    background: var(--success);
  }
  .label {
    font-size: 11px;
    color: var(--text-muted);
    min-width: 7ch;
    text-align: right;
  }
</style>
