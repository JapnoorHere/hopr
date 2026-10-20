<script lang="ts">
import { untrack } from "svelte";
import { sessions, type Tab } from "$features/terminal/session.svelte";
import TerminalPane from "$features/terminal/TerminalPane.svelte";
import { blurFade } from "$lib/motion";
import { motionLevel } from "$lib/platform";
import { type Divider, dividers, ratioFor, rects } from "./layout";

let { tab, visible }: { tab: Tab; visible: boolean } = $props();

let host: HTMLDivElement | undefined = $state();
const boxes = $derived(rects(tab.layout));
const lines = $derived(dividers(tab.layout));
const multi = $derived(boxes.size > 1);

// Zooming a pane in or out grows it from where it was (FLIP: the layout jumps once, a
// transform covers the difference), so the terminal resizes once instead of every frame.
const paneEls = new Map<string, HTMLElement>();
let before = new Map<string, DOMRect>();
function track(node: HTMLElement, key: string) {
  paneEls.set(key, node);
  return { destroy: () => paneEls.delete(key) };
}
$effect.pre(() => {
  void tab.zoomed;
  untrack(() => {
    before = new Map([...paneEls].map(([k, el]) => [k, el.getBoundingClientRect()]));
  });
});
$effect(() => {
  void tab.zoomed;
  untrack(() => {
    if (motionLevel() !== "full" || !visible) return;
    for (const [k, el] of paneEls) {
      const a = before.get(k);
      const b = el.getBoundingClientRect();
      if (!a || !b.width || !b.height || !a.width) continue;
      if (
        Math.abs(a.width - b.width) < 1 &&
        Math.abs(a.left - b.left) < 1 &&
        Math.abs(a.top - b.top) < 1
      )
        continue;
      el.animate(
        [
          {
            transformOrigin: "0 0",
            transform: `translate(${a.left - b.left}px, ${a.top - b.top}px) scale(${a.width / b.width}, ${a.height / b.height})`,
          },
          { transformOrigin: "0 0", transform: "none" },
        ],
        { duration: 260, easing: "cubic-bezier(0.2, 0.8, 0.2, 1)" },
      );
    }
  });
});

function style(id: string): string {
  const r = tab.zoomed === id ? { x: 0, y: 0, w: 1, h: 1 } : boxes.get(id);
  if (!r) return "display:none";
  return `left:${r.x * 100}%;top:${r.y * 100}%;width:${r.w * 100}%;height:${r.h * 100}%`;
}

function lineStyle(d: Divider): string {
  const a = d.area;
  return d.node.dir === "row"
    ? `left:calc(${d.at * 100}% - 3px);top:${a.y * 100}%;height:${a.h * 100}%`
    : `top:calc(${d.at * 100}% - 3px);left:${a.x * 100}%;width:${a.w * 100}%`;
}

function drag(e: PointerEvent, d: Divider) {
  if (!host) return;
  e.preventDefault();
  const box = host.getBoundingClientRect();
  const el = e.currentTarget as HTMLElement;
  el.setPointerCapture(e.pointerId);
  const onMove = (m: PointerEvent) => {
    const pos =
      d.node.dir === "row"
        ? (m.clientX - box.left) / box.width
        : (m.clientY - box.top) / box.height;
    d.node.ratio = ratioFor(d, pos);
  };
  const onUp = () => {
    el.removeEventListener("pointermove", onMove);
    el.removeEventListener("pointerup", onUp);
  };
  el.addEventListener("pointermove", onMove);
  el.addEventListener("pointerup", onUp);
}
</script>

<!-- The desk: panes sit on it as cards with a 6 px gutter (terminal revamp T2). -->
<div class="desk frame absolute inset-0" class:invisible={!visible}>
<div class="absolute inset-[3px]" bind:this={host}>
  {#each sessions.panesOf(tab) as s (s.key)}
    {@const hidden = tab.zoomed !== null && tab.zoomed !== s.key}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="pane"
      class:focused={multi && tab.focused === s.key && !tab.zoomed}
      class:resting={multi && tab.focused !== s.key && !tab.zoomed}
      class:invisible={hidden}
      style={style(s.key)}
      onfocusin={() => sessions.focus(s)}
      onpointerdown={() => sessions.focus(s)}
      in:blurFade={{ scale: 0.98, y: 0, blur: 3, duration: 220 }}
      use:track={s.key}
    >
      <div class="card">
        <TerminalPane session={s} active={visible && tab.focused === s.key} />
      </div>
    </div>
  {/each}
  {#if !tab.zoomed}
    {#each lines as d, i (i)}
      <div
        class="divider"
        class:row={d.node.dir === "row"}
        style={lineStyle(d)}
        role="separator"
        aria-orientation={d.node.dir === "row" ? "vertical" : "horizontal"}
        onpointerdown={(e) => drag(e, d)}
      ></div>
    {/each}
  {/if}
</div>
</div>

<style>
  .desk {
    background: var(--surface-1);
  }
  .pane {
    position: absolute;
  }
  /* Each pane is a card: rounded, a hairline edge, its own background. */
  .card {
    position: absolute;
    inset: 3px;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg);
    transition:
      border-color var(--dur) var(--ease-out),
      box-shadow var(--dur-slow) var(--ease-out),
      filter var(--dur-slow) var(--ease-out);
  }
  /* In a split, the focused card lifts with a faint accent ring; the others rest. */
  .pane.focused .card {
    border-color: color-mix(in oklab, var(--accent) 45%, var(--border));
    box-shadow:
      0 0 0 1px color-mix(in oklab, var(--accent) 22%, transparent),
      var(--shadow-pop);
  }
  .pane.resting .card {
    filter: brightness(0.9) saturate(0.85);
  }
  .divider {
    position: absolute;
    z-index: 15;
    height: 6px;
    cursor: row-resize;
  }
  .divider.row {
    width: 6px;
    height: auto;
    cursor: col-resize;
  }
  /* The gutter is the divider: a pill handle shows on hover. */
  .divider::before {
    content: "";
    position: absolute;
    inset: 0;
    margin: auto;
    width: 36px;
    height: 4px;
    border-radius: 999px;
    background: var(--accent);
    opacity: 0;
    transform: scale(0.6);
    transition:
      opacity var(--dur) var(--ease-out),
      transform var(--dur) var(--ease-spring);
  }
  .divider.row::before {
    width: 4px;
    height: 36px;
  }
  .divider:hover::before,
  .divider:active::before {
    opacity: 1;
    transform: scale(1);
  }
</style>
