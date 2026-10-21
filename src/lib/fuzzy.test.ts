import { describe, expect, it } from "vitest";
import { fuzzy } from "./fuzzy";

describe("fuzzy", () => {
  it("matches subsequences and rejects misses", () => {
    expect(fuzzy("spr", "Split pane right")?.hits).toEqual([0, 1, 11]);
    expect(fuzzy("xyz", "Split pane right")).toBeNull();
    expect(fuzzy("", "anything")?.score).toBe(0);
  });

  it("ranks word starts and runs above scattered letters", () => {
    const a = fuzzy("prod", "prod-db") as { score: number };
    const b = fuzzy("prod", "p-r-o-d staging") as { score: number };
    expect(a.score).toBeGreaterThan(b.score);
  });
});
