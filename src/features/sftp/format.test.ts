import { describe, expect, it } from "vitest";
import type { RemoteEntry } from "$lib/ipc";
import {
  crumbs,
  filterEntries,
  formatSize,
  modeString,
  octal,
  parentPath,
  parseOctal,
  shellQuote,
  sortEntries,
} from "./format";

const e = (name: string, kind: RemoteEntry["kind"], size = 0, modified = 0): RemoteEntry => ({
  name,
  path: `/x/${name}`,
  kind,
  size,
  modified,
  mode: 0o644,
  owner: null,
  group: null,
});

describe("sftp formatting", () => {
  it("formats sizes and modes", () => {
    expect(formatSize(512)).toBe("512 B");
    expect(formatSize(1536)).toBe("1.5 KB");
    expect(formatSize(5 * 1024 * 1024)).toBe("5.0 MB");
    expect(modeString(0o754)).toBe("rwxr-xr--");
    expect(octal(0o7)).toBe("007");
    expect(parseOctal("755")).toBe(0o755);
    expect(parseOctal("0644")).toBe(0o644);
    expect(parseOctal("999")).toBeNull();
  });

  it("splits paths", () => {
    expect(crumbs("/home/me").map((c) => c.path)).toEqual(["/", "/home", "/home/me"]);
    expect(parentPath("/home/me/")).toBe("/home");
    expect(parentPath("/etc")).toBe("/");
  });

  it("sorts folders first and filters hidden files", () => {
    const list = [
      e("b.txt", "file", 10),
      e("src", "dir"),
      e("a10.txt", "file", 5),
      e("a2.txt", "file", 7),
      e(".env", "file"),
    ];
    expect(sortEntries(list, "name", true).map((x) => x.name)).toEqual([
      "src",
      ".env",
      "a2.txt",
      "a10.txt",
      "b.txt",
    ]);
    expect(sortEntries(list, "size", false)[1]?.name).toBe("b.txt");
    expect(filterEntries(list, false, "").some((x) => x.name === ".env")).toBe(false);
    expect(filterEntries(list, true, "A1").map((x) => x.name)).toEqual(["a10.txt"]);
  });

  it("quotes paths for a shell", () => {
    expect(shellQuote("/home/me/a.txt")).toBe("/home/me/a.txt");
    expect(shellQuote("/tmp/it's here")).toBe(`'/tmp/it'\\''s here'`);
  });
});
