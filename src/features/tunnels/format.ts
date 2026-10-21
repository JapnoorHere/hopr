/** Tunnel display helpers (pure, tested). */
import type { Tunnel } from "$lib/ipc";

/** Seconds of traffic kept for the sparkline. */
export const HISTORY = 60;

/** `127.0.0.1:5432 → db:5432` style summary. */
export function describe(t: Tunnel, bound: number | null): string {
  const port = bound ?? t.bindPort;
  const here = `${t.bindAddr}:${port || "auto"}`;
  if (t.kind === "dynamic") return `SOCKS5 on ${here}`;
  if (t.kind === "remote") return `host ${here} → ${t.destHost}:${t.destPort} (here)`;
  return `${here} → ${t.destHost}:${t.destPort}`;
}

/** Listening on every interface exposes the port to the network. */
export function exposed(bindAddr: string): boolean {
  const a = bindAddr.trim();
  return a === "0.0.0.0" || a === "::" || a === "*" || a === "[::]";
}

/** "1.2 KB/s" */
export function rate(bps: number): string {
  if (bps < 1) return "0 B/s";
  if (bps < 1024) return `${Math.round(bps)} B/s`;
  if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(1)} KB/s`;
  return `${(bps / 1024 / 1024).toFixed(1)} MB/s`;
}

/** Points for an SVG polyline of `w` × `h`, newest at the right edge. */
export function sparkline(values: number[], w: number, h: number): string {
  if (values.length < 2) return "";
  const max = Math.max(...values, 1);
  const step = w / (HISTORY - 1);
  const start = w - (values.length - 1) * step;
  return values
    .map((v, i) => `${(start + i * step).toFixed(1)},${(h - (v / max) * (h - 2) - 1).toFixed(1)}`)
    .join(" ");
}
