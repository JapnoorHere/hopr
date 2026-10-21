import { describe, expect, it } from "vitest";
import { CommandTracker, parse133, parse633, parseOsc7, SNIPPETS, unescape633 } from "./shellint";

describe("shell integration parsing", () => {
  it("reads OSC 133 marks", () => {
    expect(parse133("A")).toEqual({ kind: "prompt" });
    expect(parse133("A;cl=m;aid=3")).toEqual({ kind: "prompt" });
    expect(parse133("B")).toEqual({ kind: "input" });
    expect(parse133("C")).toEqual({ kind: "exec" });
    expect(parse133("D;0")).toEqual({ kind: "done", exit: 0 });
    expect(parse133("D;127")).toEqual({ kind: "done", exit: 127 });
    expect(parse133("D")).toEqual({ kind: "done", exit: null });
    expect(parse133("D;aid=3")).toEqual({ kind: "done", exit: null });
    expect(parse133("Z")).toBeNull();
  });

  it("reads OSC 633 command lines with VS Code escaping", () => {
    expect(unescape633(String.raw`echo a\x3bb \\ c`)).toBe("echo a;b \\ c");
    expect(parse633(String.raw`E;ls -la\x3b pwd;nonce123`)).toEqual({
      kind: "cmdline",
      command: "ls -la; pwd",
    });
    expect(parse633("P;Cwd=/srv/app")).toEqual({ kind: "cwd", path: "/srv/app" });
    expect(parse633("D;1")).toEqual({ kind: "done", exit: 1 });
  });

  it("reads OSC 7 working directories", () => {
    expect(parseOsc7("file://web-1/srv/my%20app")).toBe("/srv/my app");
    expect(parseOsc7("file:///home/me")).toBe("/home/me");
    expect(parseOsc7("file://localhost/C:/Users/me")).toBe("C:/Users/me");
    expect(parseOsc7("http://x/y")).toBeNull();
  });
});

describe("CommandTracker", () => {
  function tracker(typed = "ls -la") {
    let now = 1000;
    const t = new CommandTracker(
      () => typed,
      () => now,
    );
    return { t, tick: (ms: number) => (now += ms) };
  }

  it("turns A B C D into a finished command with exit code, cwd and duration", () => {
    const { t, tick } = tracker();
    t.mark({ kind: "cwd", path: "/srv" });
    expect(t.mark({ kind: "prompt" })).toBeNull();
    t.mark({ kind: "input" });
    t.mark({ kind: "exec" });
    expect(t.running).toBe(true);
    tick(1500);
    expect(t.mark({ kind: "done", exit: 2 })).toEqual({
      command: "ls -la",
      exit: 2,
      cwd: "/srv",
      durationMs: 1500,
    });
    expect(t.integrated).toBe(true);
  });

  it("prefers the shell's own command line and ignores repeated or early marks", () => {
    const { t } = tracker("garbled");
    // D before any command (the prompt drawn right after integration was installed).
    expect(t.mark({ kind: "done", exit: 0 })).toBeNull();
    t.mark({ kind: "prompt" });
    t.mark({ kind: "input" });
    t.mark({ kind: "cmdline", command: "make test" });
    t.mark({ kind: "exec" });
    t.mark({ kind: "exec" }); // bash's DEBUG trap fires per simple command
    t.mark({ kind: "prompt" }); // a mark printed by the command's own output
    const done = t.mark({ kind: "done", exit: 0 });
    expect(done?.command).toBe("make test");
    expect(t.mark({ kind: "done", exit: 0 })).toBeNull();
  });
});

describe("snippets", () => {
  it("are single lines with every mark", () => {
    for (const [shell, s] of Object.entries(SNIPPETS)) {
      expect(s, shell).not.toContain("\n");
      expect(s, shell).toContain("133;D");
      expect(s, shell).toContain("133;C");
      expect(s, shell).toContain("]7;file://");
    }
    expect(SNIPPETS.bash).toContain("$" + "{PROMPT_COMMAND:+;$PROMPT_COMMAND}");
  });
});
