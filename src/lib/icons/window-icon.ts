import { getCurrentWindow } from "@tauri-apps/api/window";
import { inTauri } from "$lib/ipc";

/** The three theme colours the logo is drawn in. */
export interface MarkColors {
  bg: string;
  text: string;
  accent: string;
}

/** Drawn at this size; Windows scales it for the taskbar and Alt+Tab. */
const ICON_PX = 64;

/**
 * The small-cut Arc (the same shapes as src-tauri/icons/source/hopr-icon-small.svg) in a theme's
 * colours: tile = background, arc = text, cursor = accent.
 */
export function themedIconSvg({ bg, text, accent }: MarkColors): string {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="t" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" style="stop-color: color-mix(in oklab, ${bg}, ${text} 9%)"/>
      <stop offset="1" style="stop-color: color-mix(in oklab, ${bg}, black 22%)"/>
    </linearGradient>
    <linearGradient id="a" gradientUnits="userSpaceOnUse" x1="200" y1="0" x2="780" y2="0">
      <stop offset="0" style="stop-color: ${text}; stop-opacity: 0.55"/>
      <stop offset="1" style="stop-color: ${text}"/>
    </linearGradient>
  </defs>
  <rect x="16" y="16" width="992" height="992" rx="220" fill="url(#t)"/>
  <rect x="19" y="19" width="986" height="986" rx="217" fill="none" style="stroke: ${text}; stroke-opacity: 0.14" stroke-width="6"/>
  <path d="M200 790 C 270 250, 690 170, 770 610" fill="none" stroke="url(#a)" stroke-width="124" stroke-linecap="round"/>
  <rect x="600" y="700" width="300" height="128" rx="64" style="fill: ${accent}"/>
</svg>`;
}

async function toPng(svg: string, size: number): Promise<Uint8Array> {
  const image = new Image();
  image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  await image.decode();
  const canvas = new OffscreenCanvas(size, size);
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("no 2d canvas");
  ctx.drawImage(image, 0, 0, size, size);
  const blob = await canvas.convertToBlob({ type: "image/png" });
  return new Uint8Array(await blob.arrayBuffer());
}

let shown = "";

/**
 * Gives the running window's taskbar button and Alt+Tab entry the logo in the theme's colours.
 * The exe's own icon (installer, Start menu, pinned shortcut) stays the black one; Windows
 * shows that again once Hopr closes.
 */
export async function setThemedWindowIcon(colors: MarkColors): Promise<void> {
  if (!inTauri) return;
  const key = `${colors.bg}|${colors.text}|${colors.accent}`;
  if (key === shown) return;
  shown = key;
  try {
    await getCurrentWindow().setIcon(await toPng(themedIconSvg(colors), ICON_PX));
  } catch {
    // The icon is a nicety: if drawing or setting it fails, the black one stays.
    shown = "";
  }
}
