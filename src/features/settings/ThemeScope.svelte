<script lang="ts">
/**
 * Whose theme the picker below changes: Hopr's (the profile picker, the quick terminal and
 * every profile that follows it) or this profile's own.
 */
import ProfileAvatar from "$features/profiles/ProfileAvatar.svelte";
import { profileName, profiles } from "$features/profiles/profiles.svelte";
import { appearance } from "$lib/design/appearance.svelte";
import HoprMark from "$lib/icons/HoprMark.svelte";
import Switch from "$lib/ui/Switch.svelte";
import { isQuickWindow } from "$lib/window";

interface Props {
  /** First-run wording; the switch only shows once there is more than one profile. */
  onboarding?: boolean;
}

let { onboarding = false }: Props = $props();

// An unnamed profile reads as "this profile" rather than "Unnamed profile".
const named = $derived(profiles.active?.name ? profileName(profiles.active) : null);
const name = $derived(named ?? "this profile");
const Name = $derived(named ?? "This profile");
const showSwitch = $derived(!isQuickWindow && (!onboarding || profiles.list.length > 1));
</script>

<div class="scope" class:own={!appearance.editsApp}>
  <span class="mark" aria-hidden="true">
    {#if !appearance.editsApp && profiles.active}
      <ProfileAvatar profile={profiles.active} size={28} />
    {:else}
      <span class="text-accent"><HoprMark size={22} /></span>
    {/if}
  </span>
  {#if showSwitch}
    <label class="flex min-w-0 flex-1 items-center gap-3">
      <span class="min-w-0 flex-1">
        <span class="block text-sm text-fg">Own theme for {name}</span>
        <span class="block text-xs text-muted">
          {#if appearance.own}
            Only {name} looks like this. The profile picker, the quick terminal and other profiles keep
            Hopr's theme.
          {:else}
            {Name} uses Hopr's theme, like the profile picker and the quick terminal. Changes below apply to
            all of them.
          {/if}
        </span>
      </span>
      <Switch checked={appearance.own} onchange={(on) => appearance.setOwn(on)} />
    </label>
  {:else}
    <span class="min-w-0 flex-1">
      <span class="block text-sm text-fg">Hopr's theme</span>
      <span class="block text-xs text-muted">
        {#if onboarding}
          Used by every profile, the profile picker and the quick terminal. A profile can have its own later,
          in Settings.
        {:else}
          The quick terminal and the profile picker use it, and so does every profile without its own.
          Changes below apply to all of them.
        {/if}
      </span>
    </span>
  {/if}
</div>

<style>
  .scope {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface-1);
    transition:
      border-color var(--dur-fast) var(--ease-out),
      background var(--dur-fast) var(--ease-out);
  }
  .scope.own {
    border-color: color-mix(in oklab, var(--accent) 45%, var(--border));
    background: color-mix(in oklab, var(--accent) 6%, var(--surface-1));
  }
  .mark {
    display: grid;
    place-items: center;
    width: 32px;
    flex-shrink: 0;
  }
</style>
