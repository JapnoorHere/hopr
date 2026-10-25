<script lang="ts">
import { inkOn } from "$lib/design/themes";
import type { Profile } from "$lib/ipc";
import { initial } from "./profiles.svelte";

/** A profile's round avatar: its initial on its colour. */
let { profile, size = 28 }: { profile: Profile; size?: number } = $props();
</script>

<span
  class="avatar"
  style:width="{size}px"
  style:height="{size}px"
  style:font-size="{Math.round(size * 0.44)}px"
  style:background={profile.color}
  style:color={inkOn(profile.color)}
  aria-hidden="true"
>
  {initial(profile)}
</span>

<style>
  .avatar {
    display: inline-grid;
    flex-shrink: 0;
    place-items: center;
    font-weight: 600;
    line-height: 1;
    border-radius: 999px;
    box-shadow: inset 0 0 0 1px var(--border);
    user-select: none;
  }
</style>
