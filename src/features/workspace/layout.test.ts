import { describe, expect, it } from "vitest";
import {
  dividers,
  insert,
  neighbor,
  pane,
  panes,
  ratioFor,
  rects,
  remove,
  replace,
  sideAt,
  split,
} from "./layout";

describe("layout", () => {
  it("splits, lists and removes panes", () => {
    let l = split(pane("a"), "a", "row", "b");
    l = split(l, "b", "column", "c");
    expect(panes(l)).toEqual(["a", "b", "c"]);
    expect(rects(l).get("c")).toEqual({ x: 0.5, y: 0.5, w: 0.5, h: 0.5 });
    const r = remove(l, "b");
    expect(r && panes(r)).toEqual(["a", "c"]);
    expect(r && rects(r).get("c")).toEqual({ x: 0.5, y: 0, w: 0.5, h: 1 });
    expect(remove(pane("a"), "a")).toBeNull();
    expect(panes(replace(l, "c", "z"))).toEqual(["a", "b", "z"]);
  });

  it("finds neighbors by direction", () => {
    let l = split(pane("a"), "a", "row", "b");
    l = split(l, "b", "column", "c");
    expect(neighbor(l, "a", "right")).toBe("b");
    expect(neighbor(l, "c", "left")).toBe("a");
    expect(neighbor(l, "b", "down")).toBe("c");
    expect(neighbor(l, "c", "up")).toBe("b");
    expect(neighbor(l, "a", "left")).toBeNull();
  });

  it("drags dividers within bounds", () => {
    const l = split(pane("a"), "a", "row", "b");
    const [d] = dividers(l);
    expect(d?.at).toBe(0.5);
    expect(d && ratioFor(d, 0.3)).toBeCloseTo(0.3);
    expect(d && ratioFor(d, 0.99)).toBe(0.9);
  });
});

describe("dropping a tab beside a pane", () => {
  it("puts the dropped layout on the chosen side", () => {
    const base = split(pane("a"), "a", "row", "b");
    const dropped = split(pane("x"), "x", "column", "y");
    const left = insert(base, "b", "left", dropped);
    expect(panes(left)).toEqual(["a", "x", "y", "b"]);
    const below = insert(pane("a"), "a", "bottom", pane("z"));
    expect(below).toMatchObject({ kind: "split", dir: "column", a: { id: "a" }, b: { id: "z" } });
    expect(rects(below).get("z")).toEqual({ x: 0, y: 0.5, w: 1, h: 0.5 });
  });

  it("leaves other panes alone", () => {
    expect(insert(pane("a"), "missing", "right", pane("z"))).toEqual(pane("a"));
  });

  it("finds the nearest side", () => {
    expect(sideAt(0.05, 0.5)).toBe("left");
    expect(sideAt(0.9, 0.4)).toBe("right");
    expect(sideAt(0.5, 0.1)).toBe("top");
    expect(sideAt(0.4, 0.97)).toBe("bottom");
  });
});
