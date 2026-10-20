<script lang="ts">
/** Hopr logo: two hops between three points. Uses currentColor. `animated` draws it once:
 *  the points appear, then each hop arcs from one to the next. */
let { size = 20, animated = false }: { size?: number; animated?: boolean } = $props();
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
  class:animated
>
  <path d="M4 16.5C5.8 9 10.2 9 12 16.5" pathLength="1" class="hop one" />
  <path d="M12 16.5C13.8 9 18.2 9 20 16.5" pathLength="1" class="hop two" />
  <circle cx="4" cy="16.5" r="1.7" fill="currentColor" stroke="none" class="pt a" />
  <circle cx="12" cy="16.5" r="1.7" fill="currentColor" stroke="none" class="pt b" />
  <circle cx="20" cy="16.5" r="1.7" fill="currentColor" stroke="none" class="pt c" />
</svg>

<style>
  .animated .hop {
    stroke-dasharray: 1;
    stroke-dashoffset: 1;
    animation: mark-draw 420ms var(--ease-out) forwards;
  }
  .animated .one {
    animation-delay: 180ms;
  }
  .animated .two {
    animation-delay: 480ms;
  }
  .animated .pt {
    transform-box: fill-box;
    transform-origin: center;
    animation: mark-pop 360ms var(--ease-bounce) backwards;
  }
  .animated .b {
    animation-delay: 420ms;
  }
  .animated .c {
    animation-delay: 760ms;
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
  :global(:root[data-motion="reduced"]) .animated .hop,
  :global(:root[data-motion="off"]) .animated .hop {
    animation: none;
    stroke-dashoffset: 0;
  }
  :global(:root[data-motion="reduced"]) .animated .pt {
    animation: none;
  }
</style>
