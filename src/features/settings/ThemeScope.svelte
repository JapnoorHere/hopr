<script lang="ts">
/** Whose theme the picker below changes: Hopr's, or the open profile's. */
import { UserCircle } from "phosphor-svelte";
import { profileName, profiles } from "$features/profiles/profiles.svelte";
import { appearance, type LookTarget } from "$lib/design/appearance.svelte";
import HoprMark from "$lib/icons/HoprMark.svelte";
import Segmented, { type SegmentOption } from "$lib/ui/Segmented.svelte";
import { isQuickWindow } from "$lib/window";

interface Props {
  /** First run: the switcher only shows once there is more than one profile. */
  onboarding?: boolean;
}

let { onboarding = false }: Props = $props();

// An unnamed profile reads as "this profile" rather than its default name.
const named = $derived(profiles.active?.name ? profileName(profiles.active) : null);
const Name = $derived(named ?? "This profile");
const options = $derived<SegmentOption<LookTarget>[]>([
  { id: "app", label: "Hopr", icon: HoprMark },
  { id: "profile", label: named ?? "This profile", icon: UserCircle },
]);
const switcher = $derived(!isQuickWindow && (!onboarding || profiles.list.length > 1));
</script>

<div class="flex flex-col gap-2">
  {#if switcher}
    <div class="max-w-[420px]">
      <Segmented label="Whose theme" {options} value={appearance.editing} onchange={(v) => appearance.setEditing(v)} />
    </div>
  {/if}
  <p class="text-xs text-muted">
    {#if appearance.editsApp}
      Hopr's theme: the profile picker, the quick terminal and every profile that follows it.
    {:else if appearance.own}
      Only {named ?? "this profile"} uses this theme.
    {:else}
      {Name} follows Hopr's theme. Pick one below to give it its own.
    {/if}
  </p>
</div>
