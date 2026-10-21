/**
 * Keyboard shortcut registry (HOPR_PLAN T13). Commands register an id, title and default
 * keys; the palette lists them; one window handler dispatches them. App shortcuts use
 * Ctrl+Shift (or Ctrl+Tab / Ctrl+digit / Ctrl+Alt+arrows) so the shell keeps plain Ctrl
 * combos like Ctrl+W, Ctrl+K and Ctrl+L.
 */
import { safeStorage } from "./design/theme";
import { isMac } from "./platform";

export interface Command {
  id: string;
  title: string;
  /** Palette section, e.g. "Tabs". */
  group: string;
  /** Canonical "Ctrl+Shift+P" form; Ctrl means Cmd on macOS. */
  keys?: string[];
  run: () => void;
  /** Hidden from the palette and inactive when false. */
  when?: () => boolean;
}

interface Chord {
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  key: string;
}

const KEY_ALIASES: Record<string, string> = {
  esc: "escape",
  return: "enter",
  space: " ",
  "`": "backquote",
};

export function parseChord(spec: string): Chord {
  const parts = spec.split("+");
  // "Ctrl++" specs: an empty last part means the key is "+".
  const last = parts.pop() || "+";
  const mods = new Set(parts.map((p) => p.toLowerCase()));
  const key = last.toLowerCase();
  return {
    ctrl: mods.has("ctrl") || mods.has("cmd"),
    shift: mods.has("shift"),
    alt: mods.has("alt"),
    key: KEY_ALIASES[key] ?? key,
  };
}

function eventKey(e: KeyboardEvent): string {
  if (e.code === "Backquote") return "backquote";
  if (e.code === "Equal") return "=";
  if (e.code === "Minus") return "-";
  // With Shift held, e.key is the shifted symbol; use the physical digit/letter instead.
  if (/^Digit\d$/.test(e.code)) return e.code.slice(5);
  if (/^Key[A-Z]$/.test(e.code)) return e.code.slice(3).toLowerCase();
  return e.key.toLowerCase();
}

export function matches(e: KeyboardEvent, spec: string, mac = isMac): boolean {
  const c = parseChord(spec);
  const ctrl = mac ? e.metaKey : e.ctrlKey;
  return ctrl === c.ctrl && e.shiftKey === c.shift && e.altKey === c.alt && eventKey(e) === c.key;
}

export class Registry {
  #commands = new Map<string, Command>();
  /** User rebinding: command id → keys (empty = unbound). */
  #overrides: Record<string, string[]>;
  #storage: Storage | null;

  constructor(storage: Storage | null = null) {
    this.#storage = storage;
    let saved: unknown = null;
    try {
      saved = JSON.parse(storage?.getItem(BINDINGS_KEY) ?? "null");
    } catch {
      saved = null;
    }
    this.#overrides = parseOverrides(saved);
  }

  /** Effective keys: the user's binding if set, else the default. */
  keysOf(c: Command): string[] {
    return this.#overrides[c.id] ?? c.keys ?? [];
  }

  isCustom(id: string): boolean {
    return id in this.#overrides;
  }

  /** Rebind (`null` restores the default). */
  setKeys(id: string, keys: string[] | null) {
    if (keys === null) delete this.#overrides[id];
    else this.#overrides[id] = keys;
    this.#save();
  }

  overrides(): Record<string, string[]> {
    return { ...this.#overrides };
  }

  replaceOverrides(raw: unknown) {
    this.#overrides = parseOverrides(raw);
    this.#save();
  }

  #save() {
    try {
      this.#storage?.setItem(BINDINGS_KEY, JSON.stringify(this.#overrides));
    } catch {
      // Storage unavailable: bindings apply for this run only.
    }
  }

  register(cmds: Command[]): () => void {
    for (const c of cmds) this.#commands.set(c.id, c);
    return () => {
      for (const c of cmds) this.#commands.delete(c.id);
    };
  }

  all(): Command[] {
    return [...this.#commands.values()].filter((c) => c.when?.() ?? true);
  }

  get(id: string): Command | undefined {
    return this.#commands.get(id);
  }

  /** The active command bound to this key event, if any. */
  find(e: KeyboardEvent): Command | undefined {
    return this.all().find((c) => this.keysOf(c).some((k) => matches(e, k)));
  }

  /** Bindings used by more than one command (shown as conflicts in Settings). */
  conflicts(): [string, string[]][] {
    const by = new Map<string, string[]>();
    for (const c of this.#commands.values()) {
      for (const k of this.keysOf(c)) {
        const norm = JSON.stringify(parseChord(k));
        by.set(norm, [...(by.get(norm) ?? []), c.id]);
      }
    }
    return [...by].filter(([, ids]) => ids.length > 1);
  }
}

const BINDINGS_KEY = "hopr.keybindings";

function parseOverrides(raw: unknown): Record<string, string[]> {
  if (!raw || typeof raw !== "object") return {};
  const out: Record<string, string[]> = {};
  for (const [id, keys] of Object.entries(raw)) {
    if (Array.isArray(keys) && keys.every((k) => typeof k === "string" && k.length < 40))
      out[id] = keys;
  }
  return out;
}

/** Canonical "Ctrl+Shift+K" text for a key event (for recording a new binding). */
export function chordOf(e: KeyboardEvent, mac = isMac): string | null {
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return null;
  const key = eventKey(e);
  const name = key.length === 1 ? key.toUpperCase() : key === "backquote" ? "`" : e.key;
  const ctrl = mac ? e.metaKey : e.ctrlKey;
  return [ctrl && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", name].filter(Boolean).join("+");
}

export const shortcuts = new Registry(safeStorage());
