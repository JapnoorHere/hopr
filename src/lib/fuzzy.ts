/**
 * Subsequence fuzzy match for the command palette. Rewards consecutive letters and matches at
 * word starts; returns null when the query doesn't fit.
 */
export interface FuzzyMatch {
  score: number;
  /** Matched character indexes in `text` (for highlighting). */
  hits: number[];
}

const WORD_BREAK = /[\s\-_./@:]/;

export function fuzzy(query: string, text: string): FuzzyMatch | null {
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return { score: 0, hits: [] };
  const t = text.toLowerCase();
  const hits: number[] = [];
  let score = 0;
  let ti = 0;
  let prev = -2;
  for (const ch of q) {
    const i = t.indexOf(ch, ti);
    if (i === -1) return null;
    const wordStart = i === 0 || WORD_BREAK.test(t[i - 1] ?? "");
    score += 1 + (i === prev + 1 ? 3 : 0) + (wordStart ? 2 : 0);
    hits.push(i);
    prev = i;
    ti = i + 1;
  }
  // Prefer shorter texts and earlier first matches.
  score -= (hits[0] ?? 0) * 0.05 + t.length * 0.01;
  return { score, hits };
}
