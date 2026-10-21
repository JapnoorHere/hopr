/** Which Hopr window this web view is: every window loads the same app. */
import { getCurrentWindow } from "@tauri-apps/api/window";
import { inTauri } from "./ipc";

export const windowLabel: string = inTauri ? getCurrentWindow().label : "main";

/** The main window (owns workspaces, tray hooks, startup unlock). */
export const isMainWindow = windowLabel === "main";
/** The global-hotkey drop-down terminal. */
export const isQuickWindow = windowLabel === "quick";
/** A window holding tabs torn off another window. */
export const isTornOffWindow = windowLabel.startsWith("tab-");
