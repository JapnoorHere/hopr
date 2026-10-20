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

/**
 * Smart copy and paste, as in Windows Terminal: Ctrl+C copies when text is selected (and
 * clears the selection, so the next Ctrl+C interrupts), otherwise it reaches the shell as
 * an interrupt; Ctrl+V pastes. Returns what the key should do, or null to let it through.
 */
export function smartKey(
  e: {
    type: string;
    key: string;
    ctrlKey: boolean;
    shiftKey: boolean;
    altKey: boolean;
    metaKey: boolean;
  },
  smart: boolean,
  hasSelection: boolean,
): "copy" | "paste" | null {
  if (!smart || e.type !== "keydown" || !e.ctrlKey || e.shiftKey || e.altKey || e.metaKey)
    return null;
  const k = e.key.toLowerCase();
  if (k === "c") return hasSelection ? "copy" : null;
  if (k === "v") return "paste";
  return null;
}
