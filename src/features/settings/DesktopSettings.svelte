<script lang="ts">
import { layout, type RailSide } from "$features/shell/layout.svelte";
import { wrap } from "$features/vault/vault.svelte";
import { commands, inTauri } from "$lib/ipc";
import { chordOf } from "$lib/shortcuts";
import Kbd from "$lib/ui/Kbd.svelte";
import Segmented from "$lib/ui/Segmented.svelte";
import { toast } from "$lib/ui/toast.svelte";
import { updates } from "./updates.svelte";

let closeToTray = $state(true);
let hotkey = $state("");
let recording = $state(false);
/** Results show as toasts. */
function say(m: { ok: boolean; text: string }) {
  if (m.ok) toast.success(m.text);
  else toast.error(m.text);
}
let version = $state("");

$effect(() => {
  if (!inTauri) return;
  void commands.desktopPrefs().then((r) => {
    closeToTray = r.closeToTray;
    hotkey = r.quickHotkey;
  });
  void commands.appInfo().then((r) => {
    version = r.version;
  });
});

async function setTray(e: Event) {
  const enabled = (e.currentTarget as HTMLInputElement).checked;
  const res = wrap(await commands.desktopSetCloseToTray(enabled));
  if (res.ok) closeToTray = enabled;
  else say({ ok: false, text: res.error.message });
}

async function setHotkey(chord: string) {
  recording = false;
  const res = wrap(await commands.desktopSetQuickHotkey(chord));
  if (res.ok) {
    hotkey = chord;
    say({ ok: true, text: chord ? `Quick terminal: ${chord}` : "Quick terminal hotkey off" });
  } else {
    say({ ok: false, text: res.error.message });
  }
}

function record(e: KeyboardEvent) {
  e.preventDefault();
  e.stopPropagation();
  if (e.key === "Escape") {
    recording = false;
    return;
  }
  const chord = chordOf(e);
  if (chord) void setHotkey(chord);
}
</script>

<section class="mt-5 flex flex-col gap-3">
  <h3 class="label-caps text-faint">Sidebar</h3>
  <div class="field">
    <span class="text-xs text-muted">Side</span>
    <Segmented
      label="Sidebar side"
      options={[
        { id: "left", label: "Left" },
        { id: "right", label: "Right" },
      ] as const}
      value={layout.current.railSide}
      onchange={(v: RailSide) => layout.update({ railSide: v })}
    />
  </div>
  <label class="check">
    <input
      type="checkbox"
      checked={layout.current.railAutoHide}
      onchange={(e) => layout.update({ railAutoHide: e.currentTarget.checked })}
    />
    Hide the sidebar until the pointer reaches the edge
  </label>
  <p class="-mt-1 pl-6 text-xs text-faint">Also from the pin at the bottom of the sidebar. F11 toggles full screen.</p>
</section>

{#if inTauri}
  <section class="mt-5 flex flex-col gap-3">
    <h3 class="label-caps text-faint">Desktop</h3>
    <p class="text-xs text-faint">For this computer: the same in every profile.</p>
    <label class="check">
      <input type="checkbox" checked={closeToTray} onchange={setTray} />
      Keep Hopr running in the tray when the window is closed
    </label>

    <div class="field">
      <span class="text-xs text-muted">Quick terminal (system-wide hotkey)</span>
      <div class="flex items-center gap-2">
        {#if recording}
          <!-- svelte-ignore a11y_autofocus -->
          <button
            type="button"
            class="btn-ghost flex-1"
            autofocus
            onkeydown={record}
            onblur={() => (recording = false)}
          >
            Press a shortcut… (Esc cancels)
          </button>
        {:else}
          <span class="flex-1">
            {#if hotkey}<Kbd keys={hotkey} />{:else}<span class="text-xs text-faint">Off</span>{/if}
          </span>
          <button type="button" class="btn-ghost" onclick={() => (recording = true)}>Change</button>
          {#if hotkey}
            <button type="button" class="btn-ghost" onclick={() => setHotkey("")}>Turn off</button>
          {/if}
          <button type="button" class="btn-ghost" onclick={() => commands.quickToggle()}>Open</button>
        {/if}
      </div>
      <span class="text-xs text-faint">
        Slides a local shell down from the top of the screen, from any app.
      </span>
    </div>

    <div class="field">
      <span class="text-xs text-muted">Updates</span>
      <div class="flex items-center gap-2">
        <span class="flex-1 text-sm text-fg">Hopr {version}</span>
        <button
          type="button"
          class="btn-ghost"
          disabled={updates.phase !== "idle"}
          onclick={() => updates.check()}
        >
          {updates.phase === "checking" ? "Checking…" : "Check for updates"}
        </button>
        {#if updates.available}
          <button
            type="button"
            class="btn-primary"
            disabled={updates.phase !== "idle"}
            onclick={() => updates.install()}
          >
            Install {updates.available.version}
          </button>
        {/if}
      </div>
      {#if updates.message}
        <span class="text-xs text-muted" role="status">{updates.message}</span>
      {/if}
      <span class="text-xs text-faint">
        Checked at start and every 6 hours. Updates are signed and never installed without asking.
      </span>
    </div>
  </section>
{/if}
