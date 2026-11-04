<script lang="ts">
/** A serial host's settings: the port (with the ones found on this computer) and the line format. */
import type { Parity, SerialFlow } from "$lib/ipc";
import FieldError from "$lib/ui/FieldError.svelte";
import type { FieldErrors } from "$lib/ui/form";

type Field = "hostname" | "serial";

interface Props {
  port: string;
  baud: string;
  dataBits: number;
  parity: Parity;
  stopBits: number;
  flow: SerialFlow;
  /** Serial ports found on this computer. */
  ports: string[];
  errors: FieldErrors<Field>;
  fix: (f: Field) => () => void;
}

let {
  port = $bindable(),
  baud = $bindable(),
  dataBits = $bindable(),
  parity = $bindable(),
  stopBits = $bindable(),
  flow = $bindable(),
  ports,
  errors,
  fix,
}: Props = $props();

const BAUDS = [1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];
</script>

<label class="field">
  <span class="label-caps text-faint">Serial port</span>
  <!-- svelte-ignore a11y_autofocus -->
  <input
    class="input font-mono"
    bind:value={port}
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
    {#each ports as sp (sp)}<option value={sp}></option>{/each}
  </datalist>
  <span class="text-xs text-faint">
    {ports.length ? `Found: ${ports.join(", ")}` : "No serial ports found right now."}
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
  <select class="input" bind:value={flow} aria-label="Flow control">
    <option value="none">None</option>
    <option value="software">XON/XOFF</option>
    <option value="hardware">RTS/CTS</option>
  </select>
</label>
