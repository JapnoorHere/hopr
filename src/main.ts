import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@fontsource/commit-mono";
import "@fontsource-variable/jetbrains-mono";
import "./app.css";
import { mount } from "svelte";
import { appearance } from "$lib/design/appearance.svelte";
import App from "./App.svelte";
import QuickTerminal from "./features/shell/QuickTerminal.svelte";
import { initSessionEvents } from "./features/terminal/session.svelte";
import { vault } from "./features/vault/vault.svelte";
import { isQuickWindow } from "./lib/window";

appearance.init();
void initSessionEvents();
void vault.init();
if (import.meta.env.DEV) {
  void import("./features/terminal/debug").then((m) => m.installDebugHooks());
}

const target = document.getElementById("app");
if (!target) throw new Error("#app element missing from index.html");

mount(isQuickWindow ? QuickTerminal : App, { target });
