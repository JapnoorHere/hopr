/** Reading text back out of an xterm buffer. */
import type { Terminal } from "@xterm/xterm";

/** Lines `from`..`to` (inclusive) as text, trailing blanks trimmed. */
export function readLines(t: Terminal, from: number, to: number): string {
  const buf = t.buffer.active;
  const lines: string[] = [];
  for (let i = Math.max(0, from); i <= to; i++)
    lines.push(buf.getLine(i)?.translateToString(true) ?? "");
  return lines.join("\n").trimEnd();
}

/**
 * The last `maxLines` lines of output, counted back from the last line with text (or the
 * cursor), not from the blank rows below it.
 */
export function lastLines(t: Terminal, maxLines = Number.POSITIVE_INFINITY): string {
  const buf = t.buffer.active;
  let end = Math.max(buf.baseY + buf.cursorY, 0);
  for (let i = buf.length - 1; i > end; i--) {
    if (buf.getLine(i)?.translateToString(true)) {
      end = i;
      break;
    }
  }
  return readLines(t, end + 1 - maxLines, end);
}
