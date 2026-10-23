/** Terminal preferences (HOPR_PLAN T10, T14, T16). Per device, stored in localStorage. */

export const FONTS = ["Commit Mono", "Geist Mono Variable", "JetBrains Mono Variable"] as const;
export type Font = (typeof FONTS)[number];

export interface TerminalPrefs {
  fontFamily: Font;
  fontSize: number;
  lineHeight: number;
  cursorStyle: "bar" | "block" | "underline";
  cursorBlink: boolean;
  /** The cursor slides to each new position (full motion only). */
  cursorGlide: boolean;
  scrollback: number;
  copyOnSelect: boolean;
  /** Ctrl+C copies a selection (else interrupts) and Ctrl+V pastes, like Windows Terminal. */
  smartCopyPaste: boolean;
  /** Right-click copies the selection, or pastes when nothing is selected. */
  rightClickCopyPaste: boolean;
  bell: "none" | "visual" | "sound";
  /** Background brightness over the theme's, 0..100 (0 = the theme's own). */
  bgLift: number;
  /** Background opacity, 30..100 (below 100 shows the window backdrop through). */
  bgOpacity: number;
  /** What a remote program writing the clipboard (OSC 52) may do. */
  osc52: "deny" | "ask" | "allow";
  /** Command blocks (needs shell integration): separators, exit chips, pinned command. */
  blocks: "off" | "separators" | "full";
}

export const DEFAULT_PREFS: TerminalPrefs = {
  fontFamily: "Commit Mono",
  fontSize: 14,
  lineHeight: 1.2,
  cursorStyle: "bar",
  cursorBlink: true,
  cursorGlide: true,
  scrollback: 10_000,
  copyOnSelect: false,
  smartCopyPaste: true,
  rightClickCopyPaste: true,
  bell: "visual",
  osc52: "ask",
  blocks: "full",
  bgLift: 0,
  bgOpacity: 100,
};

export const FONT_SIZE = { min: 8, max: 32 };
export const SCROLLBACK = { min: 1_000, max: 100_000 };
export const BG_OPACITY = { min: 30, max: 100 };

const clamp = (n: unknown, lo: number, hi: number, d: number) =>
  typeof n === "number" && Number.isFinite(n) ? Math.min(hi, Math.max(lo, n)) : d;
const oneOf = <T extends string>(v: unknown, opts: readonly T[], d: T): T =>
  opts.includes(v as T) ? (v as T) : d;
const bool = (v: unknown, d: boolean) => (typeof v === "boolean" ? v : d);

/** Validate anything (stored JSON, an imported file) into complete prefs. */
export function parsePrefs(raw: unknown): TerminalPrefs {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const d = DEFAULT_PREFS;
  return {
    fontFamily: oneOf(r.fontFamily, FONTS, d.fontFamily),
    fontSize: Math.round(clamp(r.fontSize, FONT_SIZE.min, FONT_SIZE.max, d.fontSize)),
    lineHeight: Math.round(clamp(r.lineHeight, 1, 2, d.lineHeight) * 100) / 100,
    cursorStyle: oneOf(r.cursorStyle, ["bar", "block", "underline"] as const, d.cursorStyle),
    cursorBlink: bool(r.cursorBlink, d.cursorBlink),
    cursorGlide: bool(r.cursorGlide, d.cursorGlide),
    scrollback: Math.round(clamp(r.scrollback, SCROLLBACK.min, SCROLLBACK.max, d.scrollback)),
    copyOnSelect: bool(r.copyOnSelect, d.copyOnSelect),
    smartCopyPaste: bool(r.smartCopyPaste, d.smartCopyPaste),
    rightClickCopyPaste: bool(r.rightClickCopyPaste, d.rightClickCopyPaste),
    bell: oneOf(r.bell, ["none", "visual", "sound"] as const, d.bell),
    osc52: oneOf(r.osc52, ["deny", "ask", "allow"] as const, d.osc52),
    blocks: oneOf(r.blocks, ["off", "separators", "full"] as const, d.blocks),
    bgLift: Math.round(clamp(r.bgLift, 0, 100, d.bgLift)),
    bgOpacity: Math.round(clamp(r.bgOpacity, BG_OPACITY.min, BG_OPACITY.max, d.bgOpacity)),
  };
}

/** OSC 52 payload `c;<base64>` → text to copy, or null for queries/invalid data. */
export function decodeOsc52(data: string): string | null {
  const [, payload] = data.split(";", 2);
  if (!payload || payload === "?") return null; // reading the clipboard is never allowed
  try {
    const bytes = Uint8Array.from(atob(payload), (c) => c.charCodeAt(0));
    return new TextDecoder().decode(bytes);
  } catch {
    return null;
  }
}
