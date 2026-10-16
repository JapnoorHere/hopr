<script lang="ts">
/** Hopr logo: two hops between three points. Uses currentColor. */
let { size = 20 }: { size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  aria-hidden="true"
>
  <path d="M4 16.5C5.8 9 10.2 9 12 16.5" />
  <path d="M12 16.5C13.8 9 18.2 9 20 16.5" />
  <circle cx="4" cy="16.5" r="1.7" fill="currentColor" stroke="none" />
  <circle cx="12" cy="16.5" r="1.7" fill="currentColor" stroke="none" />
  <circle cx="20" cy="16.5" r="1.7" fill="currentColor" stroke="none" />
</svg>
