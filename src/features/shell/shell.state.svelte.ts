/** Window-level UI state: which drawer is open and whether it is docked. */

export type DrawerId =
  | "settings"
  | "keys"
  | "import"
  | "audit"
  | "files"
  | "history"
  | "tunnels"
  | "snippets";

class ShellState {
  drawer = $state<DrawerId | null>(null);
  pinned = $state(false);
  /** The open drawer was opened over the host editor, so it sits on top and Esc closes it first. */
  overEditor = $state(false);
  /** Which Keys panel to show the next time the Keys drawer opens. */
  keysIntent = $state<"list" | "generate" | "import">("list");

  toggle(id: DrawerId) {
    this.drawer = this.drawer === id ? null : id;
  }

  /** Open the Keys drawer straight on its list, Generate or Import panel. */
  openKeys(mode: "list" | "generate" | "import" = "list") {
    this.keysIntent = mode;
    this.drawer = "keys";
  }

  close() {
    this.drawer = null;
  }

  togglePin() {
    this.pinned = !this.pinned;
  }

  /** Esc handler: closes the top-most floating overlay. Pinned drawers stay. */
  closeTop(): boolean {
    if (this.drawer && !this.pinned) {
      this.drawer = null;
      return true;
    }
    return false;
  }
}

export const shell = new ShellState();
