import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@fontsource/commit-mono";
import "./app.css";
import { mount } from "svelte";
import { appearance } from "$lib/design/appearance.svelte";
import App from "./App.svelte";
import { initSessionEvents } from "./features/terminal/session.svelte";

appearance.init();
void initSessionEvents();
if (import.meta.env.DEV) {
  void import("./features/terminal/debug").then((m) => m.installDebugHooks());
}

const target = document.getElementById("app");
if (!target) throw new Error("#app element missing from index.html");

mount(App, { target });
