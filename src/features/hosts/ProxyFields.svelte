<script lang="ts">
/** A SOCKS5 or HTTP CONNECT proxy in front of a host, with optional login. */
import type { ProxyProtocol } from "$lib/ipc";
import { expand } from "$lib/motion";
import FieldError from "$lib/ui/FieldError.svelte";
import type { FieldErrors } from "$lib/ui/form";

type Field = "proxyHost" | "proxyPort" | "proxyUser";

interface Props {
  protocol: ProxyProtocol | "";
  host: string;
  port: string;
  user: string;
  password: string;
  forgetPassword: boolean;
  /** A proxy password is already saved for this host. */
  savedPassword: boolean;
  /** The host is reached through a jump host. */
  viaJump: boolean;
  errors: FieldErrors<Field>;
  fix: (f: Field) => () => void;
}

let {
  protocol = $bindable(),
  host = $bindable(),
  port = $bindable(),
  user = $bindable(),
  password = $bindable(),
  forgetPassword = $bindable(),
  savedPassword,
  viaJump,
  errors,
  fix,
}: Props = $props();
</script>

<div class="field">
  <label class="field">
    <span class="label-caps text-faint">Proxy</span>
    <select class="input" bind:value={protocol} aria-label="Proxy">
      <option value="">None</option>
      <option value="socks5">SOCKS5</option>
      <option value="http">HTTP CONNECT</option>
    </select>
  </label>
  {#if protocol}
    <div class="flex flex-col gap-2" transition:expand>
      <div class="grid grid-cols-[1fr_76px] gap-2">
        <input
          class="input"
          bind:value={host}
          placeholder="proxy.corp"
          spellcheck="false"
          aria-label="Proxy host"
          aria-invalid={!!errors.proxyHost}
          oninput={fix("proxyHost")}
        />
        <input
          class="input tabular"
          bind:value={port}
          inputmode="numeric"
          aria-label="Proxy port"
          aria-invalid={!!errors.proxyPort}
          oninput={fix("proxyPort")}
        />
      </div>
      <FieldError message={errors.proxyHost} />
      <FieldError message={errors.proxyPort} />
      <div class="grid grid-cols-2 gap-2">
        <input
          class="input"
          bind:value={user}
          placeholder="Username (optional)"
          spellcheck="false"
          aria-label="Proxy username"
          aria-invalid={!!errors.proxyUser}
          oninput={fix("proxyUser")}
        />
        <input
          class="input"
          type="password"
          bind:value={password}
          aria-label="Proxy password"
          placeholder={savedPassword && !forgetPassword ? "Saved" : "Password"}
        />
      </div>
      <FieldError message={errors.proxyUser} />
      {#if savedPassword}
        <label class="check text-xs"><input type="checkbox" bind:checked={forgetPassword} /> Forget proxy password</label>
      {/if}
      <span class="text-xs text-faint">
        {viaJump
          ? "With a jump host, the proxy of the first host in the chain is used."
          : "The proxy resolves the hostname, so names only it can see work."}
      </span>
    </div>
  {/if}
</div>
