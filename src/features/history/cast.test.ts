import { describe, expect, it } from "vitest";
import { findText, outputUntil, parseCast, squeezeIdle } from "./cast";

const REC = [
  '{"version":2,"width":80,"height":24,"title":"prod"}',
  '[0.5,"o","$ ls\\r\\n"]',
  '[1.0,"o","\\u001b[32mapp.log\\u001b[0m  notes.txt\\r\\n"]',
  '[1.2,"r","100x30"]',
  '[31.2,"o","$ tail app.log\\r\\n"]',
  '[31.5,"o","torn',
].join("\n");

describe("asciicast replay", () => {
  it("parses events and skips a torn last line", () => {
    const c = parseCast(REC);
    expect(c.header.title).toBe("prod");
    expect(c.events).toHaveLength(4);
    expect(c.duration).toBe(31.2);
    expect(() => parseCast('{"version":1,"width":1,"height":1}')).toThrow();
  });

  it("caps idle gaps", () => {
    const e = squeezeIdle(parseCast(REC).events, 2);
    const want = [0.5, 1.0, 1.2, 3.2];
    for (const [i, x] of e.entries()) expect(x[0]).toBeCloseTo(want[i] ?? Number.NaN, 9);
  });

  it("finds text without escape codes, wrapping around", () => {
    const { events } = parseCast(REC);
    expect(findText(events, "APP.LOG", 0)).toBe(1.0);
    expect(findText(events, "app.log", 1.0)).toBe(31.2);
    expect(findText(events, "app.log", 31.2)).toBe(1.0);
    expect(findText(events, "32m", 0)).toBeNull();
    expect(findText(events, "", 0)).toBeNull();
  });

  it("collects output up to a time with the size at that point", () => {
    const { events } = parseCast(REC);
    const at = outputUntil(events, 1.5, { cols: 80, rows: 24 });
    expect(at.text).toContain("notes.txt");
    expect(at.text).not.toContain("tail");
    expect([at.cols, at.rows, at.next]).toEqual([100, 30, 3]);
  });
});
