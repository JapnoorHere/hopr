import { readJson, scoped, writeJson } from "$lib/storage";
/** Frecency for palette results: how often and how recently each item was chosen. */

export interface Use {
  count: number;
  last: number;
}

export type Uses = Record<string, Use>;

const KEY = "hopr.palette.frecency";
const DAY = 86_400_000;

export function score(use: Use | undefined, now = Date.now()): number {
  if (!use) return 0;
  const age = now - use.last;
  const recency = age < DAY ? 4 : age < 7 * DAY ? 2 : age < 30 * DAY ? 1 : 0.5;
  return use.count * recency;
}

export function record(uses: Uses, id: string, now = Date.now()): Uses {
  const prev = uses[id];
  const next = { ...uses, [id]: { count: (prev?.count ?? 0) + 1, last: now } };
  // Keep the store small: drop the weakest entries past 200.
  const ids = Object.keys(next);
  if (ids.length > 200) {
    ids.sort((a, b) => score(next[a], now) - score(next[b], now));
    for (const drop of ids.slice(0, ids.length - 200)) delete next[drop];
  }
  return next;
}

export function load(): Uses {
  return readJson(scoped(KEY), (raw) => (raw && typeof raw === "object" ? (raw as Uses) : {}), {});
}

export function save(uses: Uses) {
  writeJson(scoped(KEY), uses);
}
