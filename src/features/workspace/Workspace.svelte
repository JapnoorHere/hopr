<script lang="ts">
import { sessions, type Tab } from "$features/terminal/session.svelte";
import TerminalPane from "$features/terminal/TerminalPane.svelte";
import { blurFade } from "$lib/motion";
import { type Divider, dividers, ratioFor, rects } from "./layout";

let { tab, visible }: { tab: Tab; visible: boolean } = $props();

let host: HTMLDivElement | undefined = $state();
const boxes = $derived(rects(tab.layout));
const lines = $derived(dividers(tab.layout));
const multi = $derived(boxes.size > 1);

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

<div class="absolute inset-0" class:invisible={!visible} bind:this={host}>
  {#each sessions.panesOf(tab) as s (s.key)}
    {@const hidden = tab.zoomed !== null && tab.zoomed !== s.key}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="pane"
      class:focused={multi && tab.focused === s.key && !tab.zoomed}
      class:invisible={hidden}
      style={style(s.key)}
      onfocusin={() => sessions.focus(s)}
      onpointerdown={() => sessions.focus(s)}
      in:blurFade={{ scale: 0.98, y: 0, blur: 3, duration: 220 }}
    >
      <TerminalPane session={s} active={visible && tab.focused === s.key} />
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

<style>
  .pane {
    position: absolute;
    overflow: hidden;
  }
  /* 1 px accent on the active pane (§9.6). */
  .pane.focused::after {
    content: "";
    position: absolute;
    inset: 0;
    z-index: 20;
    pointer-events: none;
    box-shadow: inset 0 0 0 1px var(--accent);
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
  .divider::before {
    content: "";
    position: absolute;
    inset: 0;
    margin: auto;
    width: 100%;
    height: 1px;
    background: var(--border);
  }
  .divider.row::before {
    width: 1px;
    height: 100%;
  }
  .divider:hover::before {
    background: var(--accent);
  }
</style>
