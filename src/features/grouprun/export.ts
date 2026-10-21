/** Run-on-hosts results as CSV or JSON (pure, tested). */
import type { GroupRunEvent } from "$lib/ipc";

/** One row per host. Cells with commas, quotes or newlines are quoted. */
export function toCsv(rows: GroupRunEvent[]): string {
  const cell = (v: string | number | null) => {
    const s = v === null ? "" : String(v);
    return /[",\n\r]/.test(s) ? `"${s.replaceAll('"', '""')}"` : s;
  };
  const head = "host,state,exit,duration_ms,stdout,stderr,error";
  const lines = rows.map((r) =>
    [r.label, r.state, r.exitStatus, r.durationMs, r.stdout, r.stderr, r.error].map(cell).join(","),
  );
  return [head, ...lines].join("\n");
}

export function toJson(command: string, rows: GroupRunEvent[]): string {
  return JSON.stringify(
    {
      command,
      results: rows.map((r) => ({
        host: r.label,
        state: r.state,
        exit: r.exitStatus,
        durationMs: r.durationMs,
        stdout: r.stdout,
        stderr: r.stderr,
        error: r.error,
      })),
    },
    null,
    2,
  );
}
