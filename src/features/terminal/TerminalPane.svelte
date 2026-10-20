<script lang="ts">
import { WarningCircle } from "phosphor-svelte";
import { untrack } from "svelte";
import { fade } from "svelte/transition";
import { broadcast } from "$features/broadcast/broadcast.svelte";
import { library } from "$features/hosts/library.svelte";
import { appearance } from "$lib/design/appearance.svelte";
import { resolvePalette } from "$lib/design/palettes";
import { blurFade, rise, shake, sink, springIn } from "$lib/motion";
import { motion, motionLevel } from "$lib/platform";
import AuthPromptCard from "./AuthPromptCard.svelte";
import ConnectCard from "./ConnectCard.svelte";
import ConnectRoute from "./ConnectRoute.svelte";
import { advance, type Progress, START } from "./connect";
import HostKeyCard from "./HostKeyCard.svelte";
import { prefs } from "./prefs.svelte";
import { sessions, type TerminalSession } from "./session.svelte";
import { describeState, isBusy } from "./state";
import Terminal from "./Terminal.svelte";

let { session, active }: { session: TerminalSession; active: boolean } = $props();

const phase = $derived(session.state);
const info = $derived(describeState(phase));
const busy = $derived(isBusy(phase));
const ended = $derived(phase.kind === "closed");
// Where the connection is on its route; a new attempt (hop 1 again) starts over.
let progress = $state<Progress>(START);
// The card stays a moment after connecting to show the finished route, then the terminal
// is revealed. Connections faster than the card's entrance skip both.
let celebrating = $state(false);
let revealing = $state(false);
// When the card (the busy states) has been on screen without a break.
let cardSince = 0;
$effect(() => {
  const s = phase;
  untrack(() => {
    const restart = s.kind === "connecting" && s.hop === 1;
    progress = advance(restart ? START : progress, s);
    const overlaid = isBusy(s);
    if (overlaid && !cardSince) cardSince = performance.now();
    if (s.kind === "ready") {
      // Only a card that was really seen gets its finish; it enters after 120 ms.
      const shown = cardSince > 0 && performance.now() - cardSince > 450;
      cardSince = 0;
      if (shown && motionLevel() === "full") {
        celebrating = true;
        setTimeout(() => {
          celebrating = false;
          revealing = true;
          setTimeout(() => (revealing = false), 450);
        }, 520);
      }
    } else if (!overlaid) cardSince = 0;
  });
});
const names = $derived(session.hops);
// A palette with its own background (Rosé Pine, Kanagawa…) colours the whole pane.
const paneBg = $derived(resolvePalette(appearance.current.theme, prefs.current.palette).background);

// Changing the font size (Ctrl+= / Ctrl+-) shows the new size for a moment.
let sizeBadge = $state<number | null>(null);
let sizeTimer: ReturnType<typeof setTimeout> | undefined;
let lastSize = prefs.current.fontSize;
$effect(() => {
  const size = prefs.current.fontSize;
  untrack(() => {
    if (size === lastSize || !active) {
      lastSize = size;
      return;
    }
    lastSize = size;
    sizeBadge = size;
    clearTimeout(sizeTimer);
    sizeTimer = setTimeout(() => (sizeBadge = null), 900);
  });
});

// Production hosts: a badge in the corner for a few seconds after connecting.
let prodBadge = $state(false);
$effect(() => {
  if (phase.kind === "ready" && session.environment === "prod") {
    untrack(() => {
      prodBadge = true;
      setTimeout(() => (prodBadge = false), 3200);
    });
  }
});

/** Shake the failure card once when it appears. */
function shaken(node: HTMLElement) {
  requestAnimationFrame(() => shake(node, 8));
}

function editConnection() {
  const t = session.target;
  if (t.kind === "host") library.editing = t.hostId;
  else if (t.kind === "quick")
    sessions.showHome({ host: t.host, port: t.port, username: t.username });
}

function onStripKey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.preventDefault();
    void sessions.close(session);
  }
}
</script>

<div
  class="relative h-full w-full bg-bg pt-3 pb-2 pl-3.5"
  style:background={paneBg}
  class:revealing
  class:ended={ended}
  class:beam-border={broadcast.includes(session.key)}
  class:prod={session.environment === "prod"}
  class:broadcasting={broadcast.includes(session.key)}
  data-broadcasting={broadcast.includes(session.key) ? "" : undefined}
>
  <Terminal {session} {active} />
  {#if sizeBadge !== null}
    <span class="size-badge" in:blurFade={{ y: 0, scale: 0.9, blur: 2, duration: 160 }} out:sink={{ duration: 160 }}>
      {sizeBadge} px
    </span>
  {/if}
  {#if prodBadge}
    <span class="prod-badge" in:blurFade={{ x: 12, y: 0, blur: 2 }} out:sink={{ x: 8, duration: 200 }}>Production</span>
  {/if}

  {#if busy || celebrating}
    <!-- The route lights up stage by stage (motion plan, M2). A short delay keeps very
         fast connections from flashing the card. -->
    <div class="overlay" in:fade={{ duration: motion(160), delay: motion(120) }} out:fade={{ duration: motion(200) }}>
      <div
        class="flex w-full justify-center"
        in:springIn={{ scale: 0.96, y: 8, delay: 120 }}
        out:sink={{ scale: 0.92, y: -10, duration: 200 }}
      >
        <ConnectCard
          {progress}
          {names}
          address={session.address}
          oncancel={() => sessions.close(session)}
        />
      </div>
    </div>
  {:else if phase.kind === "reconnecting"}
    <!-- Scrollback stays visible while reconnecting; only a strip sits on top. -->
    <div class="strip warn beam-border" role="status" in:rise={{ y: -8 }} out:sink={{ y: -8 }}>
      <span class="flex-1 text-sm text-fg">
        Connection lost. Reconnecting (attempt {phase.attempt} of {phase.maxAttempts})…
        <span class="text-faint">next try in {phase.retryInSecs} s</span>
      </span>
      <button type="button" class="btn-ghost" onclick={() => sessions.close(session)}>Close</button>
    </div>
  {:else if phase.kind === "verifying_host_key"}
    <div class="overlay" transition:fade={{ duration: motion(140) }}>
      <div class="flex flex-col items-center gap-4" in:springIn={{ scale: 0.97, y: 6 }}>
        <div class="w-72"><ConnectRoute {progress} {names} compact /></div>
        <HostKeyCard prompt={phase.prompt} onanswer={(d) => session.answerHostKey(d)} />
      </div>
    </div>
  {:else if phase.kind === "prompting"}
    <div class="overlay" transition:fade={{ duration: motion(140) }}>
      <div class="flex flex-col items-center gap-4">
        <div class="w-72"><ConnectRoute {progress} {names} compact /></div>
        {#key phase.prompt}
          <div in:springIn={{ scale: 0.97, y: 10 }}>
            <AuthPromptCard prompt={phase.prompt} onanswer={(r, remember) => session.answerPrompt(r, remember)} />
          </div>
        {/key}
      </div>
    </div>
  {:else if phase.kind === "failed"}
    {@const failed = phase.error}
    <div class="overlay" transition:fade={{ duration: motion(120) }}>
      <div class="panel w-96" role="alert" use:shaken>
        {#if session.target.kind !== "local"}
          <div class="mb-1"><ConnectRoute {progress} {names} /></div>
        {/if}
        <div class="flex items-start gap-3">
          <span class="mt-0.5 text-danger"><WarningCircle size={20} /></span>
          <div class="min-w-0 flex-1">
            <p class="text-sm font-medium text-fg">{failed.message}</p>
            <p class="tabular mt-0.5 text-xs text-faint">{session.address ?? session.label}</p>
          </div>
        </div>
        {#if failed.detail}
          <details class="text-xs text-muted">
            <summary class="cursor-default select-none">Technical details</summary>
            <pre class="mt-2 font-mono break-all whitespace-pre-wrap select-text text-faint">{failed.detail}</pre>
          </details>
        {/if}
        <div class="flex justify-end gap-2">
          <button type="button" class="btn-ghost" onclick={() => sessions.close(session)}>Close</button>
          {#if session.target.kind !== "local"}
            <button type="button" class="btn-ghost" onclick={editConnection}>Edit connection</button>
          {/if}
          <button type="button" class="btn-primary" onclick={() => sessions.retry(session)}>Retry</button>
        </div>
      </div>
    </div>
  {:else if ended}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="strip" class:down={info.tone === "down"} onkeydown={onStripKey} in:rise>
      <span class="flex-1 text-sm text-fg">{info.label}</span>
      <button type="button" class="btn-ghost" onclick={() => sessions.close(session)}>
        Close <span class="text-faint">Esc</span>
      </button>
      <!-- svelte-ignore a11y_autofocus -->
      <button type="button" class="btn-primary" autofocus onclick={() => sessions.retry(session)}>
        Reconnect <span class="opacity-70">↵</span>
      </button>
    </div>
  {/if}
</div>

<style>
  .broadcasting {
    box-shadow: inset 3px 0 0 var(--accent);
  }
  /* Production hosts get a red left edge (§9.5). */
  .prod {
    box-shadow: inset 2px 0 0 var(--danger);
  }
  .overlay {
    position: absolute;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    background: color-mix(in oklab, var(--bg) 70%, transparent);
  }
  /* A finished session fades back so it doesn't read as live; scrollback stays readable. */
  .relative :global(.xterm) {
    transition: filter 300ms var(--ease-out);
  }
  .ended :global(.xterm) {
    filter: saturate(0.35) brightness(0.8);
  }
  .size-badge {
    position: absolute;
    top: 50%;
    left: 50%;
    z-index: 6;
    padding: 6px 14px;
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
    background: color-mix(in oklab, var(--surface-3) 92%, transparent);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-pop);
    translate: -50% -50%;
    pointer-events: none;
  }
  .prod-badge {
    position: absolute;
    top: 10px;
    right: 14px;
    z-index: 6;
    padding: 2px 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--danger);
    background: color-mix(in oklab, var(--danger) 14%, var(--surface-2));
    border: 1px solid color-mix(in oklab, var(--danger) 40%, transparent);
    border-radius: 999px;
    pointer-events: none;
  }
  /* After connecting, the terminal comes into focus from a light blur, once. */
  .revealing :global(.xterm) {
    animation: hopr-reveal 420ms var(--ease-out) 1;
  }
  @keyframes hopr-reveal {
    from {
      opacity: 0.35;
      filter: blur(3px);
    }
  }
  .strip {
    position: absolute;
    left: 12px;
    right: 12px;
    bottom: 12px;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 8px 14px;
    background: var(--surface-3);
    border: 1px solid var(--border);
    border-left: 2px solid var(--text-faint);
    border-radius: var(--radius);
  }
  .strip.down {
    border-left-color: var(--danger);
  }
  .strip.warn {
    border-left-color: var(--warning);
  }
</style>
