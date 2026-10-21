import { describe, expect, it } from "vitest";
import type { TabTarget } from "$features/terminal/session.svelte";
import { pane, panes, relabel, split } from "./layout";
import { loadTab, parseWorkspace, saveTab } from "./snapshot";

const host = (id: string): TabTarget => ({
  kind: "host",
  hostId: id,
  label: id,
  environment: "none",
  trail: [id],
});

describe("workspace snapshots", () => {
  it("saves panes by index and loads them back under new keys", () => {
    let layout = pane("a");
    layout = split(layout, "a", "row", "b");
    layout = split(layout, "b", "column", "c");
    const targets: Record<string, TabTarget> = {
      a: host("web"),
      b: host("db"),
      c: host("cache"),
    };
    const saved = saveTab(layout, "c", null, (k) => targets[k]);
    expect(saved?.targets.map((t) => (t.kind === "host" ? t.hostId : ""))).toEqual([
      "web",
      "db",
      "cache",
    ]);
    expect(saved?.focused).toBe(2);

    const loaded = loadTab(saved as NonNullable<typeof saved>, (_t, i) => `new-${i}`);
    expect(panes(loaded?.layout ?? pane(""))).toEqual(["new-0", "new-1", "new-2"]);
    expect(loaded?.focused).toBe("new-2");
  });

  it("drops panes whose host is gone and keeps the rest", () => {
    const layout = split(pane("a"), "a", "row", "b");
    const saved = saveTab(layout, "b", "b", (k) => (k === "a" ? host("web") : host("db")));
    const loaded = loadTab(saved as NonNullable<typeof saved>, (t, i) =>
      t.kind === "host" && t.hostId === "db" ? null : `k${i}`,
    );
    expect(loaded?.layout).toEqual(pane("k0"));
    expect(loaded?.focused).toBe("k0");
    expect(loaded?.zoomed).toBeNull();
  });

  it("parses stored JSON defensively", () => {
    const good = {
      version: 1,
      active: 7,
      tabs: [
        {
          layout: { kind: "pane", id: 0 },
          targets: [{ kind: "local", shellId: "pwsh", label: "PowerShell" }],
          focused: 0,
          zoomed: null,
        },
        { layout: { kind: "pane", id: 3 }, targets: [host("x")], focused: 0, zoomed: null },
        {
          layout: { kind: "pane", id: 0 },
          targets: [{ kind: "quick", host: "h", port: "22" }],
          focused: 0,
          zoomed: null,
        },
      ],
    };
    const ws = parseWorkspace(JSON.stringify(good));
    expect(ws?.tabs).toHaveLength(1);
    expect(ws?.active).toBe(0);
    expect(parseWorkspace("not json")).toBeNull();
    expect(parseWorkspace(JSON.stringify({ version: 2, tabs: [] }))).toBeNull();
  });

  it("relabel removes panes mapped to null", () => {
    const l = split(split(pane("a"), "a", "row", "b"), "b", "column", "c");
    const out = relabel(l, (id) => (id === "b" ? null : String(id)));
    expect(panes(out ?? pane(""))).toEqual(["a", "c"]);
  });
});
