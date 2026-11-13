/**
 * Find in the terminal's scrollback, fast enough to type into while logs stream.
 *
 * xterm's search addon re-reads a wrapped line once for every row it spans (long JSON log
 * lines made one search take over ten seconds) and runs synchronously. Here each logical line
 * is read once as plain text and matched with a RegExp, in slices of a few milliseconds so
 * typing never waits, and new output re-runs the search at most about once a second.
 *
 * Positions are kept relative to a marker at the bottom of the buffer, so they stay right
 * while old lines scroll out of a full scrollback.
 */
import type { IBufferCell, IDecoration, IDisposable, IMarker, Terminal } from "@xterm/xterm";
import type { SearchHighlight } from "$lib/design/highlight";

export interface FindOptions {
  caseSensitive?: boolean;
  wholeWord?: boolean;
  regex?: boolean;
  decorations?: SearchHighlight;
}

export interface FindResults {
  /** The selected match, -1 for none. */
  resultIndex: number;
  /** Every match in the scrollback (only the first `limit` are highlighted). */
  resultCount: number;
}

/** A match: its first row (relative to the anchor), column and length, in cells. */
interface Match {
  rel: number;
  col: number;
  size: number;
}

/** Time a scan may hold the main thread before it yields to the next frame. */
const SLICE_MS = 6;
/** New output re-runs the search no more often than this. */
const RESCAN_MS = 900;
/** Matches whose positions are kept for stepping through; the count goes on beyond it. */
const MAX_KEPT = 20_000;

const escapeRegExp = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** The pattern for a query; throws on an unfinished regular expression. */
export function patternFor(query: string, o: FindOptions): RegExp {
  let source = o.regex ? query : escapeRegExp(query);
  if (o.wholeWord) source = `\\b(?:${source})\\b`;
  return new RegExp(source, o.caseSensitive ? "g" : "gi");
}

export class FastSearch {
  #term: Terminal;
  #limit: number;
  #listeners = new Set<(r: FindResults) => void>();
  #query = "";
  #opts: FindOptions = {};
  #pattern: RegExp | null = null;
  #matches: Match[] = [];
  #total = 0;
  #index = -1;
  /** Bumped by every new scan: a slice from an older one stops. */
  #generation = 0;
  #scanning = false;
  #anchor: IMarker | undefined;
  #decorations: IDecoration[] = [];
  #active: IDecoration[] = [];
  #rescan: ReturnType<typeof setTimeout> | undefined;
  #lastScan = 0;
  #cell: IBufferCell | undefined;
  #subs: IDisposable[];
  /** After the scan: select the match before the viewport's bottom, or step from the current one. */
  #pending: "nearest" | "next" | "previous" | null = null;

  constructor(term: Terminal, limit: number) {
    this.#term = term;
    this.#limit = limit;
    this.#subs = [term.onWriteParsed(() => this.#outputArrived())];
  }

  onDidChangeResults(cb: (r: FindResults) => void): IDisposable {
    this.#listeners.add(cb);
    return { dispose: () => this.#listeners.delete(cb) };
  }

  /** Search afresh (the query or options changed); lands on the match nearest the latest
   *  output. Throws on an unfinished regular expression. */
  search(query: string, opts: FindOptions = {}): void {
    if (!query) {
      this.clearDecorations();
      this.#emit();
      return;
    }
    this.#pattern = patternFor(query, opts);
    this.#query = query;
    this.#opts = opts;
    this.#pending = "nearest";
    this.#startScan();
  }

  /** Step to the next match (or search, if the query changed). Throws on a bad regex. */
  findNext(query: string, opts: FindOptions = {}): boolean {
    return this.#find(query, opts, "next");
  }

  findPrevious(query: string, opts: FindOptions = {}): boolean {
    return this.#find(query, opts, "previous");
  }

  /** The selected match: absolute row, column and length in cells. */
  get current(): { row: number; col: number; size: number } | null {
    const m = this.#matches[this.#index];
    const a = this.#anchor?.line;
    return m && a !== undefined ? { row: a + m.rel, col: m.col, size: m.size } : null;
  }

  clearDecorations(): void {
    this.#generation++;
    this.#scanning = false;
    clearTimeout(this.#rescan);
    this.#query = "";
    this.#pattern = null;
    this.#matches = [];
    this.#total = 0;
    this.#index = -1;
    this.#pending = null;
    this.#clearHighlights();
    this.#anchor?.dispose();
    this.#anchor = undefined;
  }

  dispose(): void {
    this.clearDecorations();
    for (const s of this.#subs) s.dispose();
    this.#listeners.clear();
  }

  #find(query: string, opts: FindOptions, dir: "next" | "previous"): boolean {
    if (!query) {
      this.clearDecorations();
      this.#emit();
      return false;
    }
    const same = query === this.#query && sameOptions(opts, this.#opts);
    if (same && !this.#scanning) {
      this.#step(dir);
      return this.#total > 0;
    }
    const pattern = patternFor(query, opts); // may throw: the bar shows "Invalid pattern"
    this.#query = query;
    this.#opts = opts;
    this.#pattern = pattern;
    // A new query lands on the match nearest the latest output; a repeated one steps.
    this.#pending = same ? dir : "nearest";
    this.#startScan();
    return true;
  }

  /** New output while a search is open: run it again soon, not on every write. */
  #outputArrived() {
    if (!this.#pattern || this.#rescan !== undefined) return;
    const wait = Math.max(0, RESCAN_MS - (performance.now() - this.#lastScan));
    this.#rescan = setTimeout(() => {
      this.#rescan = undefined;
      if (this.#pattern && !this.#scanning) this.#startScan(true);
    }, wait);
  }

  #startScan(keepSelection = false) {
    const gen = ++this.#generation;
    this.#scanning = true;
    this.#lastScan = performance.now();
    const term = this.#term;
    const buf = term.buffer.active;
    // The anchor sits on the last line; everything is stored relative to it.
    this.#anchor?.dispose();
    const anchor = term.registerMarker(buf.length - 1 - (buf.baseY + buf.cursorY));
    this.#anchor = anchor;
    const anchorAt = anchor?.line ?? buf.length - 1;
    const previous = keepSelection ? this.#selected() : null;
    const found: Match[] = [];
    let total = 0;
    let y = 0;
    const pattern = this.#pattern as RegExp;

    const slice = () => {
      if (gen !== this.#generation) return;
      const end = performance.now() + SLICE_MS;
      // Rows that left the top of a full scrollback since the scan began.
      const shift = anchorAt - (anchor?.line ?? anchorAt);
      while (performance.now() < end) {
        const row = y - shift;
        if (row >= buf.length) break;
        y =
          this.#scanLine(
            Math.max(0, row),
            pattern,
            found,
            (n) => (total += n),
            anchor?.line ?? anchorAt,
          ) + shift;
      }
      if (y - shift < buf.length) {
        requestAnimationFrame(slice);
        return;
      }
      this.#scanning = false;
      this.#matches = found;
      this.#total = total;
      this.#settle(previous);
    };
    slice();
  }

  /** Read one logical line (a row and the rows wrapped after it), record its matches,
   *  and return the row after it. */
  #scanLine(
    start: number,
    pattern: RegExp,
    out: Match[],
    count: (n: number) => void,
    anchorLine: number,
  ): number {
    const buf = this.#term.buffer.active;
    let first = start;
    // Start at the head of a wrapped line.
    while (first > 0 && buf.getLine(first)?.isWrapped) first--;
    const rows: string[] = [];
    let r = first;
    for (; r < buf.length; r++) {
      const line = buf.getLine(r);
      if (!line || (r > first && !line.isWrapped)) break;
      const next = buf.getLine(r + 1);
      rows.push(line.translateToString(!next?.isWrapped));
    }
    const text = rows.join("");
    if (!text) return r;
    pattern.lastIndex = 0;
    let n = 0;
    for (let m = pattern.exec(text); m; m = pattern.exec(text)) {
      if (m[0].length === 0) {
        pattern.lastIndex++;
        continue;
      }
      n++;
      if (out.length < MAX_KEPT) {
        const at = this.#cellOf(first, rows, m.index);
        const to = this.#cellOf(first, rows, m.index + m[0].length);
        if (at && to) {
          const cols = this.#term.cols;
          out.push({
            rel: at.row - anchorLine,
            col: at.col,
            size: (to.row - at.row) * cols + to.col - at.col,
          });
        }
      }
    }
    count(n);
    return r;
  }

  /** The cell at a string offset into a logical line (wide characters take two cells). */
  #cellOf(first: number, rows: string[], offset: number): { row: number; col: number } | null {
    let k = 0;
    let left = offset;
    while (k < rows.length - 1 && left >= (rows[k]?.length ?? 0)) {
      left -= rows[k]?.length ?? 0;
      k++;
    }
    const line = this.#term.buffer.active.getLine(first + k);
    if (!line) return null;
    // Plain text (one char per cell) maps straight across; otherwise walk the cells.
    if ((rows[k]?.length ?? 0) === line.length || left === 0) {
      if (left <= line.length) return { row: first + k, col: left };
    }
    let col = 0;
    let seen = 0;
    for (; col < line.length && seen < left; col++) {
      this.#cell = line.getCell(col, this.#cell);
      if (!this.#cell || this.#cell.getWidth() === 0) continue;
      seen += this.#cell.getChars().length || 1;
    }
    return { row: first + k, col };
  }

  /** The selected match's place (absolute row), to find it again after a rescan. */
  #selected(): { row: number; col: number } | null {
    const m = this.#matches[this.#index];
    const a = this.#anchor?.line;
    return m && a !== undefined ? { row: a + m.rel, col: m.col } : null;
  }

  #settle(previous: { row: number; col: number } | null) {
    const anchor = this.#anchor?.line ?? 0;
    const pending = this.#pending;
    this.#pending = null;
    if (previous) {
      // Same search, new output: keep the same match selected.
      const i = this.#matches.findIndex(
        (m) => anchor + m.rel === previous.row && m.col === previous.col,
      );
      this.#index = i >= 0 ? i : Math.min(this.#index, this.#matches.length - 1);
    } else if (pending === "nearest") {
      // The last match above the viewport's bottom: the most recent one you can see.
      const buf = this.#term.buffer.active;
      const bottom = buf.viewportY + this.#term.rows - 1;
      let i = this.#matches.length - 1;
      while (i > 0 && anchor + (this.#matches[i] as Match).rel > bottom) i--;
      this.#index = this.#matches.length ? i : -1;
    } else if (pending) {
      this.#step(pending, false);
    }
    this.#paint(!previous);
    this.#emit();
  }

  #step(dir: "next" | "previous", paint = true) {
    const n = this.#matches.length;
    if (!n) return;
    this.#index =
      this.#index < 0
        ? dir === "next"
          ? 0
          : n - 1
        : (this.#index + (dir === "next" ? 1 : -1) + n) % n;
    if (paint) {
      this.#paintActive(true);
      this.#emit();
    }
  }

  #paint(reveal: boolean) {
    this.#clearHighlights();
    const deco = this.#opts.decorations;
    const anchor = this.#anchor;
    if (deco && anchor) {
      const seenRows = new Set<number>();
      for (const m of this.#matches.slice(0, this.#limit)) {
        this.#decorations.push(
          ...this.#decorate(
            m,
            deco.matchBackground,
            deco.matchBorder,
            false,
            seenRows.has(m.rel) ? undefined : deco.matchOverviewRuler,
          ),
        );
        seenRows.add(m.rel);
      }
    }
    this.#paintActive(reveal);
  }

  #paintActive(reveal: boolean) {
    for (const d of this.#active) d.dispose();
    this.#active = [];
    const m = this.#matches[this.#index];
    const anchor = this.#anchor;
    if (!m || !anchor) return;
    const row = anchor.line + m.rel;
    const deco = this.#opts.decorations;
    if (deco) {
      this.#active = this.#decorate(
        m,
        deco.activeMatchBackground,
        deco.activeMatchBorder,
        true,
        deco.activeMatchColorOverviewRuler,
      );
    }
    if (reveal) {
      const term = this.#term;
      const top = term.buffer.active.viewportY;
      if (row < top || row >= top + term.rows)
        term.scrollToLine(Math.max(0, row - Math.floor(term.rows / 2)));
      // Not on screen (never opened): there is no selection to set.
      if (term.element) term.select(m.col, row, m.size);
    }
  }

  /** Decorations covering a match, one per row it spans. */
  #decorate(
    m: Match,
    background: string,
    border: string,
    active: boolean,
    ruler: string | undefined,
  ): IDecoration[] {
    const term = this.#term;
    const buf = term.buffer.active;
    const anchor = this.#anchor;
    if (!anchor) return [];
    const out: IDecoration[] = [];
    let row = anchor.line + m.rel;
    let col = m.col;
    let left = m.size;
    while (left > 0) {
      const width = Math.min(term.cols - col, left);
      const marker = term.registerMarker(row - (buf.baseY + buf.cursorY));
      if (!marker) break;
      const d = term.registerDecoration({
        marker,
        x: col,
        width,
        backgroundColor: background,
        overviewRulerOptions:
          ruler && out.length === 0 ? { color: ruler, position: "center" } : undefined,
      });
      if (d) {
        d.onRender((el) => {
          el.classList.add("xterm-find-result-decoration");
          if (active) el.classList.add("xterm-find-active-result-decoration");
          el.style.outline = `1px solid ${border}`;
        });
        d.onDispose(() => marker.dispose());
        out.push(d);
      } else {
        marker.dispose();
      }
      left -= width;
      col = 0;
      row++;
    }
    return out;
  }

  #clearHighlights() {
    for (const d of this.#decorations) d.dispose();
    for (const d of this.#active) d.dispose();
    this.#decorations = [];
    this.#active = [];
  }

  #emit() {
    const r = { resultIndex: this.#index, resultCount: this.#total };
    for (const cb of this.#listeners) cb(r);
  }
}

function sameOptions(a: FindOptions, b: FindOptions): boolean {
  return (
    !!a.caseSensitive === !!b.caseSensitive &&
    !!a.wholeWord === !!b.wholeWord &&
    !!a.regex === !!b.regex
  );
}
