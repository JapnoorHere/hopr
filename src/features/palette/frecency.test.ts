import { describe, expect, it } from "vitest";
import { record, score } from "./frecency";

describe("frecency", () => {
  it("weighs count by recency", () => {
    const now = 1_000_000_000_000;
    expect(score(undefined, now)).toBe(0);
    expect(score({ count: 3, last: now - 1000 }, now)).toBe(12);
    expect(score({ count: 3, last: now - 40 * 86_400_000 }, now)).toBe(1.5);
  });

  it("records uses", () => {
    const u = record(record({}, "a", 5), "a", 9);
    expect(u.a).toEqual({ count: 2, last: 9 });
  });
});
