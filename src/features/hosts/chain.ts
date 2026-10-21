import type { Host } from "$lib/ipc";

/** Most hops the backend allows (hopr-store MAX_HOPS). */
export const MAX_HOPS = 5;

/** Hosts dialed to reach `host`, outermost jump first, ending with `host`. Stops at loops,
 *  missing jump hosts and the hop limit, mirroring the backend. */
export function jumpTrail(host: Host, all: Host[]): Host[] {
  const trail: Host[] = [host];
  let next = host.jumpHostId;
  while (next && trail.length < MAX_HOPS) {
    const jump = all.find((h) => h.id === next);
    if (!jump || trail.includes(jump)) break;
    trail.unshift(jump);
    next = jump.jumpHostId;
  }
  return trail;
}

/** Hosts that may be chosen as `host`'s jump: not itself, and not anything that already
 *  jumps through it (which would make a loop). */
export function jumpCandidates(hostId: string | null, all: Host[]): Host[] {
  if (!hostId) return all;
  return all.filter((h) => h.id !== hostId && !jumpTrail(h, all).some((x) => x.id === hostId));
}

/** Breadcrumb labels for a tab, collapsing long chains to `first › … › last`. */
export function crumbs(labels: string[], max = 3): string[] {
  if (labels.length <= max) return labels;
  return [labels[0] ?? "", "…", labels[labels.length - 1] ?? ""];
}

function target(h: Host): string {
  return `${h.username}@${h.hostname}`;
}

/** The OpenSSH command that reaches `host` the way Hopr does (jumps become `-J`). */
export function sshCommand(host: Host, all: Host[]): string {
  if (host.protocol === "telnet") return `telnet ${host.hostname} ${host.port}`;
  if (host.protocol === "serial") return host.hostname;
  const trail = jumpTrail(host, all);
  const jumps = trail
    .slice(0, -1)
    .map((j) => (j.port === 22 ? target(j) : `${target(j)}:${j.port}`));
  const parts = ["ssh"];
  if (jumps.length) parts.push("-J", jumps.join(","));
  if (host.proxyCommand) parts.push("-o", `ProxyCommand="${host.proxyCommand}"`);
  if (host.port !== 22) parts.push("-p", String(host.port));
  parts.push(target(host));
  return parts.join(" ");
}
