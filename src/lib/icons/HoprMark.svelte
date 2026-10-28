<script lang="ts">
/**
 * Hopr logo: a > prompt that hops (dots growing as they travel) onto an underscore cursor.
 * In the theme's accent colour (the app icon is white). At 24px and below it switches
 * to a heavier three-dot cut that stays crisp. `animated` plays it once: the prompt draws,
 * the dots hop in, the cursor blinks.
 * The app icon is drawn from the same shapes (src-tauri/icons/source/).
 */
let { size = 20, animated = false }: { size?: number; animated?: boolean } = $props();
const small = $derived(size <= 24);
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" class="mark" class:animated>
  {#if small}
    <path
      class="prompt"
      pathLength="1"
      d="M3.2 7.2L8.8 12L3.2 16.8"
      fill="none"
      stroke="currentColor"
      stroke-width="3.1"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <circle class="dot d1" cx="11.6" cy="7.9" r="1.45" opacity="0.7" />
    <circle class="dot d2" cx="15" cy="5.7" r="1.75" opacity="0.88" />
    <circle class="dot d3" cx="18.6" cy="8.3" r="2.05" />
    <rect class="cursor" x="12.2" y="15.8" width="9.4" height="3.6" rx="1.8" />
  {:else}
    <path
      class="prompt"
      pathLength="1"
      d="M3.4 7.6L8.6 12L3.4 16.4"
      fill="none"
      stroke="currentColor"
      stroke-width="2.8"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <circle class="dot d1" cx="10.9" cy="8.4" r="0.95" opacity="0.55" />
    <circle class="dot d2" cx="13.2" cy="6.1" r="1.15" opacity="0.7" />
    <circle class="dot d3" cx="16.1" cy="6" r="1.35" opacity="0.85" />
    <circle class="dot d4" cx="18.3" cy="8.7" r="1.55" />
    <rect class="cursor" x="12.6" y="16.2" width="8.4" height="3.1" rx="1.55" />
  {/if}
</svg>

<style>
  .mark {
    color: var(--accent);
  }
  .animated .prompt {
    stroke-dasharray: 1;
    stroke-dashoffset: 1;
    animation: mark-draw 380ms var(--ease-out) 120ms forwards;
  }
  .animated .dot {
    transform-box: fill-box;
    transform-origin: center;
    animation: mark-pop 300ms var(--ease-bounce) backwards;
  }
  .animated .d1 {
    animation-delay: 420ms;
  }
  .animated .d2 {
    animation-delay: 510ms;
  }
  .animated .d3 {
    animation-delay: 600ms;
  }
  .animated .d4 {
    animation-delay: 690ms;
  }
  .animated .cursor {
    transform-box: fill-box;
    transform-origin: left center;
    animation:
      mark-land 320ms var(--ease-spring) 780ms backwards,
      mark-blink 1s steps(1) 1.2s 2;
  }
  @keyframes mark-draw {
    to {
      stroke-dashoffset: 0;
    }
  }
  @keyframes mark-pop {
    from {
      transform: scale(0);
      opacity: 0;
    }
  }
  @keyframes mark-land {
    from {
      transform: scaleX(0);
    }
  }
  @keyframes mark-blink {
    50% {
      opacity: 0.2;
    }
  }
  :global(:root[data-motion="reduced"]) .animated .prompt,
  :global(:root[data-motion="off"]) .animated .prompt {
    animation: none;
    stroke-dashoffset: 0;
  }
  :global(:root[data-motion="reduced"]) .animated .dot,
  :global(:root[data-motion="reduced"]) .animated .cursor {
    animation: none;
  }
</style>
