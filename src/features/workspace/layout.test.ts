import { describe, expect, it } from "vitest";
import { dividers, neighbor, pane, panes, ratioFor, rects, remove, replace, split } from "./layout";

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
