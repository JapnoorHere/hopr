<script lang="ts">
import { historyPrefs } from "$features/history/prefs.svelte";
import { sessions } from "$features/terminal/session.svelte";
import { SHELL_NAMES, type ShellKind, SNIPPETS, STARTUP_FILES } from "$features/terminal/shellint";
import { type HistorySettings, inTauri } from "$lib/ipc";
import FieldError from "$lib/ui/FieldError.svelte";
import { toast } from "$lib/ui/toast.svelte";

const NOTIFY = [
  { value: 0, label: "Never" },
  { value: 10, label: "After 10 s" },
  { value: 30, label: "After 30 s" },
  { value: 60, label: "After 1 min" },
  { value: 300, label: "After 5 min" },
];

let error = $state<string | null>(null);
let shellKind = $state<ShellKind>("bash");
let copied = $state(false);
let patterns = $state(historyPrefs.current.exclude.join("\n"));

$effect(() => {
  patterns = historyPrefs.current.exclude.join("\n");
});

/** Saves a change. A bad exclude pattern is shown under that box; anything else in a toast. */
async function save(patch: Partial<HistorySettings>) {
  const failed = await historyPrefs.save({ ...historyPrefs.current, ...patch });
  error = failed && patch.exclude ? failed : null;
  if (failed && !patch.exclude) toast.error(failed);
}

function savePatterns() {
  const exclude = patterns
    .split("\n")
    .map((p) => p.trim())
    .filter(Boolean);
  if (exclude.join("\n") !== historyPrefs.current.exclude.join("\n")) void save({ exclude });
}

async function copySnippet() {
  await navigator.clipboard.writeText(SNIPPETS[shellKind]).catch(() => undefined);
  copied = true;
  setTimeout(() => (copied = false), 1500);
}

function enableHere() {
  sessions.active?.write(new TextEncoder().encode(` ${SNIPPETS[shellKind]}\r`));
}

const canType = $derived(sessions.active?.state.kind === "ready");
</script>

{#if inTauri}
  <section class="mt-5 flex flex-col gap-2">
    <h3 class="label-caps text-faint">History &amp; recording</h3>
    <label class="flex items-center gap-2 text-sm">
      <input
        type="checkbox"
        checked={historyPrefs.current.enabled}
        onchange={(e) => save({ enabled: e.currentTarget.checked })}
      />
      Save command history
    </label>
    <label class="flex items-center gap-2 text-sm">
      <input
        type="checkbox"
        checked={historyPrefs.current.ignoreSpace}
        onchange={(e) => save({ ignoreSpace: e.currentTarget.checked })}
      />
      Skip commands that start with a space
    </label>
    <label class="field">
      <span class="text-xs text-muted">Never save commands matching (one per line, * = anything)</span>
      <textarea
        class="input font-mono text-xs"
        rows="3"
        bind:value={patterns}
        onblur={savePatterns}
        aria-label="Exclude patterns"
        spellcheck="false"
        aria-invalid={!!error}
        oninput={() => (error = null)}
      ></textarea>
      <FieldError message={error} />
      <span class="text-xs text-faint">
        Commands with passwords, tokens, API keys or keys in them are never saved, whatever this list says.
      </span>
    </label>
    <label class="flex items-center gap-2 text-sm">
      <input
        type="checkbox"
        checked={historyPrefs.current.recordAll}
        onchange={(e) => save({ recordAll: e.currentTarget.checked })}
      />
      Record every new session
    </label>
    <p class="-mt-1 pl-6 text-xs text-faint">
      Each recording stops at 63 MB. Past 1 GB in total, the oldest are deleted.
    </p>
    <label class="field">
      <span class="text-xs text-muted">Notify when a long command finishes in the background</span>
      <select
        class="input"
        value={historyPrefs.current.notifyAfterSecs}
        aria-label="Notify after"
        onchange={(e) => save({ notifyAfterSecs: Number(e.currentTarget.value) })}
      >
        {#each NOTIFY as n (n.value)}<option value={n.value}>{n.label}</option>{/each}
      </select>
    </label>

    <h3 class="label-caps mt-3 text-faint">Shell integration</h3>
    <p class="text-xs text-muted">
      Lets Hopr see each command and where you are, for exact history, exit codes and jumping between commands.
    </p>
    <div class="flex gap-2">
      <select class="input flex-1" bind:value={shellKind} aria-label="Shell">
        {#each Object.keys(SNIPPETS) as ShellKind[] as k (k)}<option value={k}>{SHELL_NAMES[k]}</option>{/each}
      </select>
      <button type="button" class="btn-ghost" onclick={copySnippet}>{copied ? "Copied" : "Copy"}</button>
      <button
        type="button"
        class="btn-ghost"
        disabled={!canType}
        onclick={enableHere}
        title="Type it into the focused terminal (this session only)">Enable here</button
      >
    </div>
    <textarea
      class="input font-mono text-xs"
      rows="3"
      readonly
      value={SNIPPETS[shellKind]}
      aria-label="Setup command"
    ></textarea>
    <p class="text-xs text-faint">To keep it on, add this line to {STARTUP_FILES[shellKind]} on the host.</p>
  </section>
{/if}
