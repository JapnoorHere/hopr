import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@fontsource/commit-mono";
import "@fontsource-variable/jetbrains-mono";
import "./app.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { mount } from "svelte";
import { appearance } from "$lib/design/appearance.svelte";
import App from "./App.svelte";
import { profiles } from "./features/profiles/profiles.svelte";
import QuickTerminal from "./features/shell/QuickTerminal.svelte";
import { prefs } from "./features/terminal/prefs.svelte";
import { initSessionEvents } from "./features/terminal/terminal-session.svelte";
import { vault } from "./features/vault/vault.svelte";
import { i18n } from "./lib/i18n/i18n.svelte";
import { inTauri } from "./lib/ipc";
import { shortcuts } from "./lib/shortcuts";
import { readText, removeKey, SWITCHED_KEY, safeSessionStorage, writeText } from "./lib/storage";
import { isQuickWindow } from "./lib/window";

/** Set once this window has been shown at start-up (survives reloads, not restarts). */
const SHOWN_KEY = "hopr.shown";

// Just switched profile: appear at once, without the start-up animation.
if (readText(SWITCHED_KEY, safeSessionStorage())) {
  document.documentElement.dataset.switched = "";
  removeKey(SWITCHED_KEY, safeSessionStorage());
}

// Which profile is open decides whose theme and saved layout load, so it comes first.
await profiles.load().catch(() => undefined);
// The quick terminal and the start-up profile picker show Hopr's own look.
appearance.init(isQuickWindow || profiles.picking);
void initSessionEvents();
// Another Hopr window (quick terminal, main, torn-off tabs) changed a setting: follow it here.
window.addEventListener("storage", (e) => {
  if (e.key !== null && !e.key.startsWith("hopr.")) return;
  appearance.reload();
  prefs.reload();
  i18n.reload();
  shortcuts.reload();
});
void vault.init();
if (import.meta.env.DEV) {
  void import("./features/terminal/debug").then((m) => m.installDebugHooks());
}

const target = document.getElementById("app");
if (!target) throw new Error("#app element missing from index.html");

// Sliders fill with the accent up to the thumb: keep each range's --fill in step with it.
const fillRange = (el: Element) => {
  if (!(el instanceof HTMLInputElement) || el.type !== "range") return;
  const min = Number(el.min || 0);
  const max = Number(el.max || 100);
  const pct = max > min ? ((Number(el.value) - min) / (max - min)) * 100 : 0;
  el.style.setProperty("--fill", `${pct}%`);
};
// A touchpad pinch (Ctrl + wheel) never zooms the whole app; the terminal handles its own.
window.addEventListener(
  "wheel",
  (e) => {
    if (e.ctrlKey) e.preventDefault();
  },
  { passive: false },
);

document.addEventListener("input", (e) => e.target && fillRange(e.target as Element), true);
new MutationObserver(() => {
  for (const el of document.querySelectorAll('input[type="range"]')) fillRange(el);
}).observe(document.body, {
  childList: true,
  subtree: true,
  attributes: true,
  attributeFilter: ["value"],
});

mount(isQuickWindow ? QuickTerminal : App, { target });

// App windows open hidden: show this one once its first frame is on screen, so it never
// flashes black while starting. Once per window: a reload while it sits in the tray must
// not bring it back. The quick terminal shows itself.
if (inTauri && !isQuickWindow && !readText(SHOWN_KEY, safeSessionStorage())) {
  writeText(SHOWN_KEY, "1", safeSessionStorage());
  requestAnimationFrame(() =>
    requestAnimationFrame(() => {
      const w = getCurrentWindow();
      void w.show().then(() => w.setFocus());
    }),
  );
}
