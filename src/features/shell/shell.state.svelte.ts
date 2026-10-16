/** Window-level UI state: which drawer is open and whether it is docked. */

export type DrawerId = "settings";

class ShellState {
  drawer = $state<DrawerId | null>(null);
  pinned = $state(false);

  toggle(id: DrawerId) {
    this.drawer = this.drawer === id ? null : id;
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
