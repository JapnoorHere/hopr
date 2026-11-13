/** Full screen (F11): the window covers the whole screen, no title bar or taskbar. */
import { getCurrentWindow } from "@tauri-apps/api/window";
import { inTauri } from "./ipc";

class FullScreen {
  on = $state(false);

  constructor() {
    if (!inTauri) return;
    const win = getCurrentWindow();
    const sync = async () => {
      this.on = await win.isFullscreen();
    };
    void sync();
    // Leaving full screen another way (the OS, a window snap) is followed here too.
    void win.onResized(() => void sync());
  }

  async toggle() {
    if (!inTauri) return;
    const win = getCurrentWindow();
    const next = !(await win.isFullscreen());
    await win.setFullscreen(next);
    this.on = next;
  }
}

export const fullscreen = new FullScreen();
