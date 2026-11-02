/** How a saved host is described in lists: its address line and its reachability. */
import type { Host, PingResult } from "$lib/ipc";

/** "deploy@10.0.3.4:2222 · via bastion", "telnet · 10.0.0.9:23", "serial · COM3 · 9600 baud". */
export function hostAddress(h: Host, jumpLabel?: string): string {
  const address =
    h.protocol === "serial"
      ? `serial · ${h.hostname} · ${h.serial?.baud ?? 9600} baud`
      : h.protocol === "telnet"
        ? `telnet · ${h.username ? `${h.username}@` : ""}${h.hostname}:${h.port}`
        : `${h.username}@${h.hostname}${h.port === 22 ? "" : `:${h.port}`}`;
  const via = [
    h.proxyCommand ? "via ProxyCommand" : "",
    h.jumpHostId ? `via ${jumpLabel ?? "jump host"}` : "",
    h.proxy && !h.jumpHostId ? `via ${h.proxy.protocol} proxy` : "",
  ].filter(Boolean);
  return [address, ...via].join(" · ");
}

/** The tooltip on a host's reachability dot. */
export function reachTitle(h: Host, ping: PingResult | undefined): string {
  if (!ping) return "Not checked yet";
  if (ping.status === "reachable") return `Reachable · ${ping.ms} ms`;
  if (ping.status === "unreachable") return "Not reachable";
  return h.jumpHostId ? "Reached through a jump host" : "Reached through a proxy";
}
