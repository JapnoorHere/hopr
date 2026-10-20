import type { Group, Host } from "$lib/ipc";

/**
 * Match hosts against a search query. Words are ANDed; each word matches label, hostname,
 * user or tag (case-insensitive). `tag:x`, `env:prod` and `user:x` narrow to one field.
 */
export function matchesQuery(host: Host, query: string): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  return words.every((w) => {
    const [key, value] = w.includes(":") ? (w.split(":", 2) as [string, string]) : ["", w];
    switch (key) {
      case "tag":
        return host.tags.some((t) => t.includes(value));
      case "env":
        return host.environment.startsWith(value);
      case "user":
        return host.username.toLowerCase().includes(value);
      default:
        return [host.label, host.hostname, host.username, ...host.tags].some((f) =>
          f.toLowerCase().includes(w),
        );
    }
  });
}

/**
 * Split `text` into pieces that do and don't match the plain words of a search query
 * (`tag:`, `env:` and `user:` words aren't about this text), for highlighting.
 */
export function highlight(text: string, query: string): { text: string; hit: boolean }[] {
  const words = query
    .toLowerCase()
    .split(/\s+/)
    .filter((w) => w && !w.includes(":"));
  const lower = text.toLowerCase();
  const marks: [number, number][] = [];
  for (const w of words) {
    for (let at = lower.indexOf(w); at >= 0; at = lower.indexOf(w, at + w.length)) {
      marks.push([at, at + w.length]);
    }
  }
  if (marks.length === 0) return [{ text, hit: false }];
  marks.sort((x, y) => x[0] - y[0]);
  const merged: [number, number][] = [];
  for (const m of marks) {
    const last = merged.at(-1);
    if (last && m[0] <= last[1]) last[1] = Math.max(last[1], m[1]);
    else merged.push([m[0], m[1]]);
  }
  const out: { text: string; hit: boolean }[] = [];
  let pos = 0;
  for (const [from, to] of merged) {
    if (from > pos) out.push({ text: text.slice(pos, from), hit: false });
    out.push({ text: text.slice(from, to), hit: true });
    pos = to;
  }
  if (pos < text.length) out.push({ text: text.slice(pos), hit: false });
  return out;
}

export interface HostSection {
  id: string;
  title: string;
  hosts: Host[];
}

const RECENT_COUNT = 5;

/** Favorites, Recent, then one section per group, then ungrouped. Empty sections are dropped. */
export function sectionHosts(hosts: Host[], groups: Group[], query: string): HostSection[] {
  const visible = hosts.filter((h) => matchesQuery(h, query));
  if (query.trim()) return [{ id: "results", title: "Results", hosts: visible }];

  const recent = visible
    .filter((h) => h.lastConnectedAt)
    .sort((a, b) => (b.lastConnectedAt ?? "").localeCompare(a.lastConnectedAt ?? ""))
    .slice(0, RECENT_COUNT);
  const sections: HostSection[] = [
    { id: "favorites", title: "Favorites", hosts: visible.filter((h) => h.favorite) },
    { id: "recent", title: "Recent", hosts: recent },
    ...groups.map((g) => ({
      id: g.id,
      title: g.name,
      hosts: visible.filter((h) => h.groupId === g.id),
    })),
    {
      id: "ungrouped",
      title: groups.length ? "Other hosts" : "All hosts",
      hosts: visible.filter((h) => !h.groupId || !groups.some((g) => g.id === h.groupId)),
    },
  ];
  return sections.filter((s) => s.hosts.length > 0);
}
