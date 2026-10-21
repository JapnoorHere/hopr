/**
 * App-wide toasts: short confirmations ("Saved", "Copied") and failures of actions that
 * aren't tied to one form field. Field problems are shown next to the field instead
 * (FieldError.svelte).
 */

export type ToastKind = "success" | "error" | "info";

export interface Toast {
  id: number;
  kind: ToastKind;
  text: string;
  action?: { label: string; run: () => void };
  /** When its timer last started and how long it runs (drives the countdown line). */
  armed?: number;
  life?: number;
}

/** At most this many on screen; the oldest goes first. */
const MAX = 4;
const LIFETIME: Record<ToastKind, number> = { success: 3500, info: 4000, error: 7000 };

class Toasts {
  list = $state<Toast[]>([]);
  #next = 1;
  #timers = new Map<number, ReturnType<typeof setTimeout>>();

  show(text: string, kind: ToastKind = "info", action?: Toast["action"]): number {
    // The same message twice in a row just restarts its timer.
    const same = this.list.find((t) => t.text === text && t.kind === kind);
    if (same) {
      this.#arm(same.id, kind);
      return same.id;
    }
    const id = this.#next++;
    this.list = [...this.list, { id, kind, text, action }].slice(-MAX);
    this.#arm(id, kind);
    return id;
  }

  /** A toast with an action (Undo) stays up longer, so there's time to use it. */
  #life(id: number, kind: ToastKind): number {
    return this.list.find((t) => t.id === id)?.action ? 8000 : LIFETIME[kind];
  }

  success = (text: string, action?: Toast["action"]) => this.show(text, "success", action);
  error = (text: string, action?: Toast["action"]) => this.show(text, "error", action);
  info = (text: string, action?: Toast["action"]) => this.show(text, "info", action);

  dismiss(id: number) {
    clearTimeout(this.#timers.get(id));
    this.#timers.delete(id);
    this.list = this.list.filter((t) => t.id !== id);
  }

  /** Keep a toast up while the pointer is on it. */
  hold(id: number) {
    clearTimeout(this.#timers.get(id));
  }

  release(id: number) {
    const t = this.list.find((x) => x.id === id);
    if (t) this.#arm(id, t.kind);
  }

  #arm(id: number, kind: ToastKind) {
    clearTimeout(this.#timers.get(id));
    const life = this.#life(id, kind);
    this.#timers.set(
      id,
      setTimeout(() => this.dismiss(id), life),
    );
    this.list = this.list.map((t) => (t.id === id ? { ...t, armed: Date.now(), life } : t));
  }
}

export const toast = new Toasts();
