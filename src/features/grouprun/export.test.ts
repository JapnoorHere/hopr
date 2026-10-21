import { describe, expect, it } from "vitest";
import type { GroupRunEvent } from "$lib/ipc";
import { toCsv, toJson } from "./export";

const row = (over: Partial<GroupRunEvent>): GroupRunEvent => ({
  runId: "r",
  hostId: "h",
  label: "web-1",
  state: "ok",
  exitStatus: 0,
  durationMs: 120,
  stdout: "up 3 days\n",
  stderr: "",
  error: null,
  ...over,
});

describe("run-on-hosts export", () => {
  it("writes CSV with quoting", () => {
    const csv = toCsv([
      row({}),
      row({ label: 'db "main"', state: "failed", exitStatus: 1, stderr: "a,b" }),
    ]);
    expect(csv.split("\n")[0]).toBe("host,state,exit,duration_ms,stdout,stderr,error");
    expect(csv).toContain('web-1,ok,0,120,"up 3 days\n",,');
    expect(csv).toContain('"db ""main""",failed,1,120,"up 3 days\n","a,b",');
  });

  it("writes JSON with the command", () => {
    const j = JSON.parse(
      toJson("uptime", [row({ error: "Not an SSH host", state: "failed", exitStatus: null })]),
    );
    expect(j.command).toBe("uptime");
    expect(j.results[0]).toMatchObject({
      host: "web-1",
      state: "failed",
      exit: null,
      error: "Not an SSH host",
    });
  });
});
