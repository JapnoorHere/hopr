/** Paste safety (HOPR_PLAN §9.4): multi-line or very long pastes are confirmed first,
 *  because every newline runs a command. */

export const LONG_PASTE_CHARS = 2000;

export interface PasteCheck {
  confirm: boolean;
  lines: number;
}

export function checkPaste(text: string): PasteCheck {
  // A single trailing newline is how most copies end; it still executes, so it counts.
  const lines = text.split(/\r\n|\r|\n/).length - (/(\r\n|\r|\n)$/.test(text) ? 1 : 0);
  const multiline = /[\r\n]/.test(text);
  return { confirm: multiline || text.length > LONG_PASTE_CHARS, lines: Math.max(1, lines) };
}
