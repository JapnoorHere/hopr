/**
 * Reading the clipboard. In the app it goes through the backend, so the web view never shows
 * its "wants to see text copied to the clipboard" prompt; in a plain browser (vite dev, tests)
 * it falls back to the web API. Empty string when there's no text or it can't be read.
 */
import { commands, inTauri } from "./ipc";

export async function readClipboard(): Promise<string> {
  if (inTauri) {
    const r = await commands.clipboardReadText();
    return r.status === "ok" ? r.data : "";
  }
  return navigator.clipboard.readText().catch(() => "");
}
