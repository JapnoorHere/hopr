/** Window-level UI state: which drawer is open. Drawers dock beside the content. */

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
  /** The open drawer was opened over the host editor, so it sits on top and Esc closes it first. */
  overEditor = $state(false);
  /** Which Keys panel to show the next time the Keys drawer opens. */
  keysIntent = $state<"list" | "generate" | "import">("list");

  /** Settings opens scrolled to this section (then it is cleared). */
  settingsSection = $state<string | null>(null);

  openSettings(section: string | null = null) {
    this.settingsSection = section;
    this.drawer = "settings";
  }

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

  /** Esc handler: closes the open drawer. */
  closeTop(): boolean {
    if (this.drawer) {
      this.drawer = null;
      return true;
    }
    return false;
  }
}

export const shell = new ShellState();
