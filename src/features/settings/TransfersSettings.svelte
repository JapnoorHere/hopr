<script lang="ts">
import { wrap } from "$features/vault/vault.svelte";
import { commands, inTauri, type TransferSettings } from "$lib/ipc";
import { toast } from "$lib/ui/toast.svelte";

const LIMITS = [
  { value: 0, label: "No limit" },
  { value: 256, label: "256 KB/s" },
  { value: 1024, label: "1 MB/s" },
  { value: 5 * 1024, label: "5 MB/s" },
  { value: 10 * 1024, label: "10 MB/s" },
  { value: 50 * 1024, label: "50 MB/s" },
];

let s = $state<TransferSettings>({ concurrency: 3, limitKbps: 0 });

$effect(() => {
  if (inTauri) {
    void commands.transfersSettings().then((v) => {
      s = v;
    });
  }
});

async function save(next: TransferSettings) {
  const res = wrap(await commands.transfersSetSettings(next));
  if (res.ok) s = next;
  else toast.error(res.error.message);
}

function num(e: Event): number {
  return Number((e.currentTarget as HTMLSelectElement).value);
}
</script>

{#if inTauri}
  <section class="mt-5 flex flex-col gap-2">
    <h3 class="label-caps text-faint">Transfers</h3>
    <label class="field">
      <span class="text-xs text-muted">At the same time</span>
      <select
        class="input"
        value={s.concurrency}
        aria-label="Transfers at the same time"
        onchange={(e) => save({ ...s, concurrency: num(e) })}
      >
        {#each [1, 2, 3, 4, 5, 6, 7, 8] as n (n)}<option value={n}>{n}</option>{/each}
      </select>
    </label>
    <label class="field">
      <span class="text-xs text-muted">Speed limit (all transfers together)</span>
      <select
        class="input"
        value={s.limitKbps}
        aria-label="Speed limit"
        onchange={(e) => save({ ...s, limitKbps: num(e) })}
      >
        {#each LIMITS as l (l.value)}<option value={l.value}>{l.label}</option>{/each}
      </select>
    </label>
  </section>
{/if}
