/** Pure helpers for the files drawer and transfer queue. */
import type { RemoteEntry } from "$lib/ipc";

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

/** 0o754 → "rwxr-xr--". */
export function modeString(mode: number): string {
  const bits = "rwxrwxrwx";
  let out = "";
  for (let i = 0; i < 9; i++) out += mode & (1 << (8 - i)) ? bits[i] : "-";
  return out;
}

/** "755" or "0755" → 0o755; null if not an octal permission. */
export function parseOctal(text: string): number | null {
  const t = text.trim();
  if (!/^0?[0-7]{3,4}$/.test(t)) return null;
  return Number.parseInt(t, 8);
}

export function octal(mode: number): string {
  return (mode & 0o7777).toString(8).padStart(3, "0");
}

/** "/home/me/src" → [{name: "/", path: "/"}, {name: "home", path: "/home"}, …]. */
export function crumbs(path: string): { name: string; path: string }[] {
  const parts = path.split("/").filter(Boolean);
  const out = [{ name: "/", path: "/" }];
  let acc = "";
  for (const p of parts) {
    acc += `/${p}`;
    out.push({ name: p, path: acc });
  }
  return out;
}

export function parentPath(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  const i = trimmed.lastIndexOf("/");
  return i <= 0 ? "/" : trimmed.slice(0, i);
}

export type SortKey = "name" | "size" | "modified";

/** Folders always first; then by the chosen key. */
export function sortEntries(entries: RemoteEntry[], key: SortKey, asc: boolean): RemoteEntry[] {
  const dir = asc ? 1 : -1;
  return [...entries].sort((a, b) => {
    const ad = a.kind === "dir" ? 0 : 1;
    const bd = b.kind === "dir" ? 0 : 1;
    if (ad !== bd) return ad - bd;
    let c = 0;
    if (key === "size") c = a.size - b.size;
    else if (key === "modified") c = (a.modified ?? 0) - (b.modified ?? 0);
    if (c === 0) {
      c = a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
    }
    return c * dir;
  });
}

export function filterEntries(
  entries: RemoteEntry[],
  showHidden: boolean,
  query: string,
): RemoteEntry[] {
  const q = query.trim().toLowerCase();
  return entries.filter(
    (e) => (showHidden || !e.name.startsWith(".")) && (!q || e.name.toLowerCase().includes(q)),
  );
}

export function formatSpeed(bytesPerSec: number): string {
  return `${formatSize(Math.round(bytesPerSec))}/s`;
}

/** Shell-quoted for pasting into a terminal. */
export function shellQuote(path: string): string {
  return /^[\w./~-]+$/.test(path) ? path : `'${path.replaceAll("'", `'\\''`)}'`;
}
