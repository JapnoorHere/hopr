import { beforeEach, describe, expect, it } from "vitest";
import { panes } from "$features/workspace/layout";
import { sessions, type TabTarget } from "./session.svelte";

const local = (label: string): TabTarget => ({ kind: "local", shellId: "cmd", label });

async function reset() {
  for (const t of [...sessions.tabs]) await sessions.closeTab(t);
}

describe("sessions store (tabs + panes)", () => {
  beforeEach(reset);

  it("opens tabs, cycles and jumps to them", () => {
    sessions.open(local("a"));
    sessions.open(local("b"));
    sessions.open(local("c"));
    expect(sessions.active?.label).toBe("c");
    sessions.cycle(1);
    expect(sessions.active?.label).toBe("a");
    sessions.cycle(-1);
    expect(sessions.active?.label).toBe("c");
    sessions.goto(2);
    expect(sessions.active?.label).toBe("b");
    sessions.goto(9);
    expect(sessions.active?.label).toBe("c");
  });

  it("splits, moves focus, zooms and closes panes", async () => {
    const first = sessions.open(local("a"));
    sessions.split("row");
    const tab = sessions.activeTab;
    if (!tab) throw new Error("no tab");
    expect(panes(tab.layout)).toHaveLength(2);
    const second = sessions.active;
    expect(second?.key).not.toBe(first.key);
    expect(second?.label).toBe("a"); // same target as the split pane

    sessions.move("left");
    expect(sessions.active?.key).toBe(first.key);
    sessions.toggleZoom();
    expect(tab.zoomed).toBe(first.key);
    sessions.toggleZoom();
    expect(tab.zoomed).toBeNull();

    await sessions.close(first);
    expect(panes(tab.layout)).toEqual([second?.key]);
    expect(sessions.active?.key).toBe(second?.key);
    expect(sessions.tabs).toHaveLength(1);
  });

  it("closing the last pane closes the tab; reopen brings it back", async () => {
    const s = sessions.open(local("x"));
    await sessions.close(s);
    expect(sessions.tabs).toHaveLength(0);
    expect(sessions.activeKey).toBeNull();
    expect(sessions.canReopen).toBe(true);
    sessions.reopenClosed();
    expect(sessions.active?.label).toBe("x");
  });

  it("retry replaces the pane in place", () => {
    sessions.open(local("a"));
    sessions.split("column");
    const tab = sessions.activeTab;
    const old = sessions.active;
    if (!tab || !old) throw new Error("setup");
    sessions.retry(old);
    const ids = panes(tab.layout);
    expect(ids).toHaveLength(2);
    expect(ids).not.toContain(old.key);
    expect(sessions.active?.key).toBe(ids[1]);
  });
});
