/** asciicast v2 reading for replay: parse, squeeze idle gaps, search. */

export interface CastHeader {
  version: number;
  width: number;
  height: number;
  timestamp?: number;
  title?: string;
}

/** `[seconds, "o" | "r" | …, data]`. */
export type CastEvent = [number, string, string];

export interface Cast {
  header: CastHeader;
  events: CastEvent[];
  duration: number;
}

/** Parse a recording; bad lines are skipped, a bad header is an error. */
export function parseCast(text: string): Cast {
  const lines = text.split("\n");
  const header = JSON.parse(lines[0] ?? "") as CastHeader;
  if (header.version !== 2 || !(header.width > 0) || !(header.height > 0)) {
    throw new Error("Not an asciicast v2 recording");
  }
  const events: CastEvent[] = [];
  for (const line of lines.slice(1)) {
    if (!line.trim()) continue;
    try {
      const e = JSON.parse(line) as unknown;
      if (
        Array.isArray(e) &&
        typeof e[0] === "number" &&
        typeof e[1] === "string" &&
        typeof e[2] === "string"
      ) {
        events.push([e[0], e[1], e[2]]);
      }
    } catch {
      // A torn last line (recording still being written): skip it.
    }
  }
  return { header, events, duration: events.at(-1)?.[0] ?? 0 };
}

/** Cap every pause at `maxIdle` seconds ("skip idle"). */
export function squeezeIdle(events: CastEvent[], maxIdle: number): CastEvent[] {
  let shift = 0;
  let prev = 0;
  return events.map(([t, code, data]) => {
    const gap = t - prev;
    if (gap > maxIdle) shift += gap - maxIdle;
    prev = t;
    return [t - shift, code, data];
  });
}

/** Time of the first output event after `from` whose text contains `query` (case-insensitive,
 *  escape sequences ignored); wraps around to the start. */
export function findText(events: CastEvent[], query: string, from: number): number | null {
  const q = query.toLowerCase();
  if (!q) return null;
  const plain = (s: string) =>
    // biome-ignore lint/suspicious/noControlCharactersInRegex: stripping terminal escapes
    s.replace(/\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)/g, "").toLowerCase();
  const hit = (e: CastEvent) => e[1] === "o" && plain(e[2]).includes(q);
  const after = events.find((e) => e[0] > from && hit(e));
  return (after ?? events.find(hit))?.[0] ?? null;
}

/** Output from the start up to (and including) `t`, as one string, plus the last size. */
export function outputUntil(
  events: CastEvent[],
  t: number,
  size: { cols: number; rows: number },
): { text: string; cols: number; rows: number; next: number } {
  let text = "";
  let { cols, rows } = size;
  let i = 0;
  for (; i < events.length; i++) {
    const e = events[i];
    if (!e || e[0] > t) break;
    if (e[1] === "o") text += e[2];
    else if (e[1] === "r") {
      const m = /^(\d+)x(\d+)$/.exec(e[2]);
      if (m) {
        cols = Number(m[1]);
        rows = Number(m[2]);
      }
    }
  }
  return { text, cols, rows, next: i };
}
