import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@fontsource/commit-mono";
import "@fontsource-variable/jetbrains-mono";
import "./app.css";
import { mount } from "svelte";
import { appearance } from "$lib/design/appearance.svelte";
import App from "./App.svelte";
import { profiles } from "./features/profiles/profiles.svelte";
import QuickTerminal from "./features/shell/QuickTerminal.svelte";
import { initSessionEvents } from "./features/terminal/session.svelte";
import { vault } from "./features/vault/vault.svelte";
import { isQuickWindow } from "./lib/window";

// Which profile is open decides whose theme and saved layout load, so it comes first.
await profiles.load().catch(() => undefined);
appearance.init();
void initSessionEvents();
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
