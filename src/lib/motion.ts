/**
 * Shared motion (see the motion plan): Svelte transitions and one-shot effects that follow
 * Settings > Motion. Full moves; reduced keeps short fades and drops movement; off is instant.
 * Animate only transform, opacity and filter, so nothing triggers layout.
 */
import { cubicIn, cubicOut } from "svelte/easing";
import type { TransitionConfig } from "svelte/transition";
import { motionLevel } from "./platform";

const W = Math.sqrt(260);
const Z = 26 / (2 * Math.sqrt(260));
const WD = W * Math.sqrt(1 - Z * Z);
/** Seconds the spring takes to settle; the easing below maps 0..1 onto it. */
const SETTLE = 0.52;

/** The same spring as --ease-spring (stiffness 260, damping 26): a 1.4% overshoot. */
export function springEase(t: number): number {
  if (t >= 1) return 1;
  const x = t * SETTLE;
  return 1 - Math.exp(-Z * W * x) * (Math.cos(WD * x) + ((Z * W) / WD) * Math.sin(WD * x));
}

/** Overshooting ease for success moments (checks, pops), like --ease-bounce. */
export function bounceEase(t: number): number {
  if (t >= 1) return 1;
  const w = Math.sqrt(380);
  const z = 20 / (2 * Math.sqrt(380));
  const wd = w * Math.sqrt(1 - z * z);
  const x = t * 0.64;
  return 1 - Math.exp(-z * w * x) * (Math.cos(wd * x) + ((z * w) / wd) * Math.sin(wd * x));
}

interface MoveOpts {
  delay?: number;
  duration?: number;
  /** Starting offset in px (positive x = from the right, positive y = from below). */
  x?: number;
  y?: number;
  /** Starting scale. */
  scale?: number;
  /** Starting blur in px. */
  blur?: number;
  /** A transform the element already has (e.g. translateX(-50%)), kept in front. */
  base?: string;
}

/** Reduced and off motion: a short fade, or nothing. */
function calm(duration: number): TransitionConfig | null {
  const level = motionLevel();
  if (level === "off") return { duration: 0 };
  if (level === "reduced")
    return { duration: Math.min(duration, 100), css: (t) => `opacity: ${t}` };
  return null;
}

function moved(t: number, u: number, o: MoveOpts): string {
  const parts = [o.base ?? ""];
  if (o.x || o.y) parts.push(`translate(${u * (o.x ?? 0)}px, ${u * (o.y ?? 0)}px)`);
  if (o.scale !== undefined && o.scale !== 1) parts.push(`scale(${o.scale + (1 - o.scale) * t})`);
  return parts.join(" ").trim() || "none";
}

/** Fade in with a small rise and a blur that clears: the default entrance. */
export function blurFade(_node: Element, o: MoveOpts = {}): TransitionConfig {
  const duration = o.duration ?? 240;
  const reduced = calm(duration);
  if (reduced) return reduced;
  const opts = { y: 6, blur: 4, ...o };
  return {
    delay: o.delay ?? 0,
    duration,
    easing: cubicOut,
    css: (t, u) =>
      `opacity: ${t}; transform: ${moved(t, u, opts)}; filter: blur(${u * (opts.blur ?? 0)}px)`,
  };
}

/** Settle into place on the spring curve: drawers, cards, popovers, toasts. */
export function springIn(_node: Element, o: MoveOpts = {}): TransitionConfig {
  const duration = o.duration ?? 380;
  const reduced = calm(duration);
  if (reduced) return reduced;
  return {
    delay: o.delay ?? 0,
    duration,
    easing: springEase,
    // Opacity follows a plain curve: the spring may overshoot past 1.
    css: (t, u) => `opacity: ${Math.min(1, t * 1.8)}; transform: ${moved(t, u, o)}`,
  };
}

/** Leave quickly (exits run shorter than entrances), moving the way the entrance came from. */
export function sink(_node: Element, o: MoveOpts = {}): TransitionConfig {
  const duration = o.duration ?? 140;
  const reduced = calm(duration);
  if (reduced) return reduced;
  return {
    delay: o.delay ?? 0,
    duration,
    easing: cubicIn,
    css: (t, u) => `opacity: ${t}; transform: ${moved(t, u, o)}`,
  };
}

/** Rise from below: strips, banners, list rows. */
export function rise(node: Element, o: MoveOpts = {}): TransitionConfig {
  return blurFade(node, { y: 8, blur: 0, duration: 180, ...o });
}

/** Grow or collapse a block's height (sections that appear, rows that are removed). */
export function expand(
  node: Element,
  o: { delay?: number; duration?: number } = {},
): TransitionConfig {
  const duration = o.duration ?? 200;
  const reduced = calm(duration);
  if (reduced) return reduced;
  const style = getComputedStyle(node);
  const height = Number.parseFloat(style.height);
  const pt = Number.parseFloat(style.paddingTop);
  const pb = Number.parseFloat(style.paddingBottom);
  const mt = Number.parseFloat(style.marginTop);
  const mb = Number.parseFloat(style.marginBottom);
  return {
    delay: o.delay ?? 0,
    duration,
    easing: cubicOut,
    css: (t) =>
      `overflow: hidden; opacity: ${Math.min(1, t * 1.5)}; height: ${t * height}px; ` +
      `padding-top: ${t * pt}px; padding-bottom: ${t * pb}px; margin-top: ${t * mt}px; margin-bottom: ${t * mb}px`,
  };
}

/** Pop in with a bounce: check marks, badges, counters. */
export function pop(
  _node: Element,
  o: { delay?: number; duration?: number } = {},
): TransitionConfig {
  const duration = o.duration ?? 420;
  const reduced = calm(duration);
  if (reduced) return reduced;
  return {
    delay: o.delay ?? 0,
    duration,
    easing: bounceEase,
    css: (t) => `opacity: ${Math.min(1, t * 2)}; transform: scale(${0.5 + 0.5 * t})`,
  };
}

/** Delay for item `index` in a list that staggers in (only the first few; none when calm). */
export function stagger(index: number, step = 22, max = 10): number {
  return motionLevel() === "full" ? Math.min(index, max) * step : 0;
}

/** Shake once (a refused password, a failed connection). Full motion only. */
export function shake(el: Element | null | undefined, distance = 6) {
  if (!el || motionLevel() !== "full" || typeof el.animate !== "function") return;
  const d = distance;
  el.animate(
    [
      { transform: "translateX(0)" },
      { transform: `translateX(${-d}px)` },
      { transform: `translateX(${d * 0.8}px)` },
      { transform: `translateX(${-d * 0.5}px)` },
      { transform: `translateX(${d * 0.25}px)` },
      { transform: "translateX(0)" },
    ],
    { duration: 320, easing: "ease-out" },
  );
}

/**
 * For a row of segmented buttons or tabs: one highlight that slides to the selected one
 * (`.on`, `aria-checked` or `aria-selected`) instead of each button lighting up on its own.
 */
export function slidingPill(node: HTMLElement) {
  const pill = document.createElement("span");
  pill.className = "seg-pill";
  pill.setAttribute("aria-hidden", "true");
  node.dataset.pill = "";
  node.prepend(pill);
  let first = true;
  const place = () => {
    const on = node.querySelector<HTMLElement>(
      ':scope > .on, :scope > [aria-checked="true"], :scope > [aria-selected="true"]',
    );
    if (!on) {
      pill.style.opacity = "0";
      return;
    }
    // The first placement is where the pill starts, not a move.
    pill.style.transition = first ? "none" : "";
    pill.style.opacity = "1";
    pill.style.transform = `translate(${on.offsetLeft}px, ${on.offsetTop}px)`;
    pill.style.width = `${on.offsetWidth}px`;
    pill.style.height = `${on.offsetHeight}px`;
    first = false;
  };
  const changes = new MutationObserver(place);
  changes.observe(node, {
    subtree: true,
    attributes: true,
    attributeFilter: ["class", "aria-checked", "aria-selected"],
  });
  const sizes = new ResizeObserver(place);
  sizes.observe(node);
  requestAnimationFrame(place);
  return {
    destroy() {
      changes.disconnect();
      sizes.disconnect();
      pill.remove();
    },
  };
}

/** A small wiggle that asks for attention (a background tab whose bell rang). */
export function wiggle(el: Element | null | undefined) {
  if (!el || motionLevel() !== "full" || typeof el.animate !== "function") return;
  el.animate(
    [
      { transform: "rotate(0)" },
      { transform: "rotate(-3deg)" },
      { transform: "rotate(3deg)" },
      { transform: "rotate(-2deg)" },
      { transform: "rotate(1deg)" },
      { transform: "rotate(0)" },
    ],
    { duration: 520, easing: "ease-out" },
  );
}

/** Briefly glow an element with the accent colour (a row that was just added or reached). */
export function flash(el: Element | null | undefined) {
  if (!el || motionLevel() === "off" || typeof el.animate !== "function") return;
  el.animate(
    [
      { boxShadow: "inset 0 0 0 100vmax color-mix(in oklab, var(--accent) 16%, transparent)" },
      { boxShadow: "inset 0 0 0 100vmax transparent" },
    ],
    { duration: 900, easing: "ease-out" },
  );
}
