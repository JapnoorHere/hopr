import { isTauri } from "@tauri-apps/api/core";

export * from "./bindings";

/** False when the UI runs in a plain browser (vite dev without Tauri, tests). */
export const inTauri: boolean = isTauri();
