/** Parse quick-connect input: `host`, `user@host`, `host:port`, `user@[::1]:22`, `ssh://…`. */

export interface ParsedTarget {
  host: string;
  username?: string;
  port?: number;
}

function parsePort(raw: string): number | null {
  if (!/^\d{1,5}$/.test(raw)) return null;
  const n = Number(raw);
  return n >= 1 && n <= 65535 ? n : null;
}

export function parseTarget(input: string): ParsedTarget | null {
  let rest = input
    .trim()
    .replace(/^ssh:\/\//i, "")
    .replace(/\/$/, "");
  if (!rest || /\s/.test(rest)) return null;

  let username: string | undefined;
  const at = rest.lastIndexOf("@");
  if (at !== -1) {
    username = rest.slice(0, at);
    rest = rest.slice(at + 1);
    if (!username) return null;
  }

  let host = rest;
  let port: number | undefined;
  const v6 = /^\[([^\]]+)\](?::(\d+))?$/.exec(rest);
  if (v6) {
    host = v6[1] ?? "";
    if (v6[2] !== undefined) {
      const p = parsePort(v6[2]);
      if (p === null) return null;
      port = p;
    }
  } else if ((rest.match(/:/g) ?? []).length === 1) {
    const [h = "", p = ""] = rest.split(":");
    const parsed = parsePort(p);
    if (parsed === null) return null;
    host = h;
    port = parsed;
  }
  if (!host) return null;

  const out: ParsedTarget = { host };
  if (username !== undefined) out.username = username;
  if (port !== undefined) out.port = port;
  return out;
}
