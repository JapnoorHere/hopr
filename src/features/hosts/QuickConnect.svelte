<script lang="ts">
/** Connect to a host that isn't saved: user@host:port, checked as you submit. */
import { ArrowRight } from "phosphor-svelte";
import { sessions } from "$features/terminal/session.svelte";
import { parseTarget } from "$features/terminal/target";
import FieldError from "$lib/ui/FieldError.svelte";

let quick = $state(fromDraft());
let error = $state<string | null>(null);

/** A connection started elsewhere (the palette) and not finished is offered again here. */
function fromDraft(): string {
  const d = sessions.draft;
  if (!d?.host) return "";
  const user = d.username ? `${d.username}@` : "";
  const port = d.port && d.port !== 22 ? `:${d.port}` : "";
  return `${user}${d.host}${port}`;
}

function problem(text: string): string | null {
  if (!text.trim()) return "Type a host to connect to, like deploy@10.0.3.4";
  const t = parseTarget(text);
  if (!t) return "Use host, user@host or user@host:port";
  if (!t.username) return `Add a user, like deploy@${t.host}`;
  return null;
}

function onsubmit(e: SubmitEvent) {
  e.preventDefault();
  error = problem(quick);
  const t = parseTarget(quick);
  if (error || !t?.username) return;
  sessions.open({ kind: "quick", host: t.host, port: t.port ?? 22, username: t.username });
  quick = "";
}
</script>

<form class="flex items-center gap-2 px-6 pt-4" {onsubmit} novalidate>
  <input
    class="input font-mono"
    bind:value={quick}
    placeholder="Quick connect: user@host:port"
    spellcheck="false"
    aria-label="Quick connect"
    aria-invalid={!!error}
    oninput={() => (error = null)}
    onblur={() => {
      if (!quick.trim()) error = null;
    }}
  />
  <button type="submit" class="go btn-primary shrink-0">
    Connect <ArrowRight size={14} />
  </button>
</form>
{#if error}<div class="px-6 pt-1.5"><FieldError message={error} /></div>{/if}

<style>
  /* The arrow leans toward where you're going. */
  .go :global(svg) {
    transition: translate var(--dur) var(--ease-spring);
  }
  .go:hover :global(svg) {
    translate: 3px 0;
  }
</style>
