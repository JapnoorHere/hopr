import { Terminal } from "@xterm/xterm";
import { describe, expect, it } from "vitest";
import { FastSearch, type FindResults, patternFor } from "./fast-search";

describe("patternFor", () => {
  it("matches literally unless regex is on", () => {
    expect(patternFor("a.b", {}).test("axb")).toBe(false);
    expect(patternFor("a.b", { regex: true }).test("axb")).toBe(true);
  });
  it("ignores case unless asked", () => {
    expect(patternFor("error", {}).test("ERROR")).toBe(true);
    expect(patternFor("error", { caseSensitive: true }).test("ERROR")).toBe(false);
  });
  it("whole word", () => {
    expect(patternFor("err", { wholeWord: true }).test("error")).toBe(false);
    expect(patternFor("err", { wholeWord: true }).test("an err here")).toBe(true);
  });
  it("throws on an unfinished regex", () => {
    expect(() => patternFor("(abc", { regex: true })).toThrow();
  });
});

/** A terminal with some text written, and a search that resolves when its scan is done. */
async function setUp(text: string, cols = 40) {
  const term = new Terminal({ cols, rows: 10, scrollback: 1000, allowProposedApi: true });
  await new Promise<void>((r) => term.write(text, r));
  const search = new FastSearch(term, 100);
  const results = (fn: () => void) =>
    new Promise<FindResults>((resolve) => {
      const sub = search.onDidChangeResults((r) => {
        sub.dispose();
        resolve(r);
      });
      fn();
    });
  return { term, search, results };
}

describe("FastSearch", () => {
  it("counts matches across wrapped rows and selects the latest one", async () => {
    // A 100-character line in a 40-column terminal wraps over three rows; ERROR sits in the
    // middle row and across the break between rows two and three.
    const long = `${"a".repeat(45)}ERROR${"b".repeat(28)}ERROR${"c".repeat(17)}`;
    const { search, results } = await setUp(`first line\r\n${long}\r\nlast ERROR\r\n`);
    const r = await results(() => search.search("error"));
    expect(r.resultCount).toBe(3);
    expect(r.resultIndex).toBe(2); // the one on the last line, nearest the bottom
    expect(search.current).toEqual({ row: 4, col: 5, size: 5 });
    // Next wraps round to the first ERROR: character 45 of the long line is row 2, column 5.
    search.findNext("error");
    expect(search.current).toEqual({ row: 2, col: 5, size: 5 });
    // The second (character 78) starts at row 2, column 38 and runs into row 3.
    search.findNext("error");
    expect(search.current).toEqual({ row: 2, col: 38, size: 5 });
  });

  it("selects a match that wraps over two rows in full", async () => {
    const long = `${"a".repeat(38)}ERROR${"b".repeat(10)}`;
    const { search, results } = await setUp(`${long}\r\n`);
    await results(() => search.search("error"));
    // Starts at column 38 of the first row and runs 2 cells into the next: 5 cells in all.
    expect(search.current).toEqual({ row: 0, col: 38, size: 5 });
  });

  it("steps forward and back, wrapping around", async () => {
    const { search, results } = await setUp("one x\r\ntwo x\r\nthree x\r\n");
    const first = await results(() => search.search("x"));
    expect(first).toEqual({ resultIndex: 2, resultCount: 3 });
    expect((await results(() => search.findNext("x"))).resultIndex).toBe(0);
    expect((await results(() => search.findPrevious("x"))).resultIndex).toBe(2);
  });

  it("reports nothing found", async () => {
    const { search, results } = await setUp("nothing to see\r\n");
    expect(await results(() => search.search("zzz"))).toEqual({ resultIndex: -1, resultCount: 0 });
  });
});
