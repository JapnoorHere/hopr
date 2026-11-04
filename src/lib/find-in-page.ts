/**
 * Find in page: every match of a search inside an element is highlighted (CSS Custom
 * Highlight API), and Enter / Shift+Enter step through them like a text search.
 * Highlights are named `<name>-hit` (all) and `<name>-now` (the current one) for styling.
 */
export class PageFinder {
  #hits: Range[] = [];
  #at = -1;

  constructor(readonly name: string) {}

  get count(): number {
    return this.#hits.length;
  }

  /** The current match, 0-based; -1 before the first step. */
  get at(): number {
    return this.#at;
  }

  /** Every match of `query` (lower-case) in `root`'s text, skipping elements matching `skip`. */
  find(root: HTMLElement | undefined, query: string, skip: string) {
    this.#hits = [];
    this.#at = -1;
    if (query && root) {
      const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
        acceptNode: (n) =>
          n.parentElement?.closest(skip) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT,
      });
      for (let n = walk.nextNode(); n; n = walk.nextNode()) {
        const text = (n.textContent ?? "").toLowerCase();
        for (let i = text.indexOf(query); i >= 0; i = text.indexOf(query, i + query.length)) {
          const r = document.createRange();
          r.setStart(n, i);
          r.setEnd(n, i + query.length);
          this.#hits.push(r);
        }
      }
    }
    this.#paint();
  }

  /** Move to the next (1) or previous (-1) match and return it. */
  step(by: number): Range | undefined {
    if (!this.#hits.length) return undefined;
    this.#at = (this.#at + by + this.#hits.length) % this.#hits.length;
    this.#paint();
    return this.#hits[this.#at];
  }

  clear() {
    if (!supported()) return;
    CSS.highlights.delete(`${this.name}-hit`);
    CSS.highlights.delete(`${this.name}-now`);
  }

  #paint() {
    if (!supported()) return;
    CSS.highlights.set(`${this.name}-hit`, new Highlight(...this.#hits));
    const now = this.#hits.at(this.#at);
    if (now && this.#at >= 0) CSS.highlights.set(`${this.name}-now`, new Highlight(now));
    else CSS.highlights.delete(`${this.name}-now`);
  }
}

const supported = () => typeof CSS !== "undefined" && "highlights" in CSS;
