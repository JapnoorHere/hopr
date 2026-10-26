import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// The Settings list on the left and the sections on the page must be in the same order,
// or clicking one jumps somewhere unexpected and the scroll highlight lights the wrong item.
describe("Settings sections", () => {
  it("are listed in the same order as they appear on the page", () => {
    const src = readFileSync("src/features/shell/SettingsDrawer.svelte", "utf8");
    const nav = [...src.matchAll(/^\s{4}id: "([a-z]+)",$/gm)].map((m) => m[1]);
    const page = [...src.matchAll(/<section data-section="([a-z]+)"/g)].map((m) => m[1]);
    expect(nav.length).toBeGreaterThan(5);
    expect(page).toEqual(nav);
  });
});
