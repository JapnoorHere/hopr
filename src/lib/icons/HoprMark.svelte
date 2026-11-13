<script lang="ts" module>
let next = 0;
</script>

<script lang="ts">
/**
 * Hopr logo: an arc that hops and lands on a cursor, in the colours of the theme in use.
 * With a tile (the default from 28px): the tile is the theme's background, the arc its text
 * colour, the cursor and glow its accent, so the logo always belongs to the screen it's on.
 * Without a tile it's the bare mark in `currentColor` with an accent cursor, for tight spots.
 * At 24px and below it uses the heavier small cut. `animated` plays it once: the arc draws,
 * the cursor lands and blinks. The app icon is drawn from the same shapes (src-tauri/icons/source/).
 */
let {
  size = 20,
  animated = false,
  tile = size >= 28,
}: { size?: number; animated?: boolean; tile?: boolean } = $props();

const id = `hopr-mark-${next++}`;
const small = $derived(size <= 24);
const arc = $derived(small ? "M200 790 C 270 250, 690 170, 770 610" : "M226 760 C 300 300, 650 210, 748 642");
const cursor = $derived(small ? { x: 600, y: 700, w: 300, h: 128 } : { x: 648, y: 708, w: 226, h: 88 });
</script>

<svg width={size} height={size} viewBox="0 0 1024 1024" aria-hidden="true" class="mark" class:animated class:tile>
  {#if tile}
    <defs>
      <linearGradient id="{id}-tile" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" class="tile-top" />
        <stop offset="1" class="tile-bottom" />
      </linearGradient>
      <radialGradient id="{id}-glow" cx="0.74" cy="0.2" r="0.64">
        <stop offset="0" class="glow-in" />
        <stop offset="1" class="glow-out" />
      </radialGradient>
      <linearGradient id="{id}-arc" gradientUnits="userSpaceOnUse" x1="220" y1="0" x2="780" y2="0">
        <stop offset="0" class="arc-from" />
        <stop offset="0.45" class="arc-mid" />
        <stop offset="1" class="arc-to" />
      </linearGradient>
    </defs>
    <rect width="1024" height="1024" rx="232" fill="url(#{id}-tile)" />
    <rect width="1024" height="1024" rx="232" fill="url(#{id}-glow)" />
    <rect x="3" y="3" width="1018" height="1018" rx="229" class="edge" />
  {/if}
  <path
    class="arc"
    pathLength="1"
    d={arc}
    fill="none"
    stroke={tile ? `url(#${id}-arc)` : "currentColor"}
    stroke-width={small ? 124 : 84}
    stroke-linecap="round"
  />
  <rect class="cursor" x={cursor.x} y={cursor.y} width={cursor.w} height={cursor.h} rx={cursor.h / 2} />
</svg>

<style>
  .mark {
    color: var(--accent);
    flex: none;
  }
  .cursor {
    fill: var(--accent);
  }
  .tile .tile-top {
    stop-color: color-mix(in oklab, var(--bg), var(--text) 9%);
  }
  .tile .tile-bottom {
    stop-color: color-mix(in oklab, var(--bg), black 22%);
  }
  .tile .glow-in {
    stop-color: var(--accent);
    stop-opacity: 0.4;
  }
  .tile .glow-out {
    stop-color: var(--accent);
    stop-opacity: 0;
  }
  .tile .arc-from {
    stop-color: var(--text);
    stop-opacity: 0.1;
  }
  .tile .arc-mid {
    stop-color: var(--text);
    stop-opacity: 0.72;
  }
  .tile .arc-to {
    stop-color: var(--text);
  }
  .edge {
    fill: none;
    stroke: var(--text);
    stroke-opacity: 0.12;
    stroke-width: 6;
  }
  .animated .arc {
    stroke-dasharray: 1;
    stroke-dashoffset: 1;
    animation: mark-draw 520ms var(--ease-out) 120ms forwards;
  }
  .animated .cursor {
    transform-box: fill-box;
    transform-origin: center;
    animation:
      mark-land 340ms var(--ease-spring) 560ms backwards,
      mark-blink 1s steps(1) 1s 2;
  }
  @keyframes mark-draw {
    to {
      stroke-dashoffset: 0;
    }
  }
  @keyframes mark-land {
    from {
      transform: scaleX(0.4);
      opacity: 0;
    }
  }
  @keyframes mark-blink {
    50% {
      opacity: 0.2;
    }
  }
  :global(:root[data-motion="reduced"]) .animated .arc,
  :global(:root[data-motion="off"]) .animated .arc {
    animation: none;
    stroke-dashoffset: 0;
  }
  :global(:root[data-motion="reduced"]) .animated .cursor,
  :global(:root[data-motion="off"]) .animated .cursor {
    animation: none;
  }
</style>
