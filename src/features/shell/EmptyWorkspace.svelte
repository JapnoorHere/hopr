<script lang="ts">
import ConnectForm from "$features/terminal/ConnectForm.svelte";
import { sessions } from "$features/terminal/session.svelte";
import HoprMark from "$lib/icons/HoprMark.svelte";
</script>

<div class="m-auto flex w-full max-w-sm flex-col items-center gap-6 px-6">
  <span class="text-faint"><HoprMark size={40} /></span>
  <!-- Re-create the form when the prefill changes (e.g. "Edit connection"). -->
  {#key sessions.draft}
    <ConnectForm />
  {/key}
</div>
