/**
 * Dev-only hooks for the native E2E smoke test (scripts/e2e-smoke.mjs), which drives the
 * real WebView2 over the Chrome DevTools Protocol. Never bundled in production builds:
 * main.ts imports this only when import.meta.env.DEV is true.
 */
import { sessions } from "./session.svelte";

export function installDebugHooks() {
  Object.assign(window, {
    __hopr: {
      connect(host: string, port: number, username: string, password: string) {
        sessions.open({ host, port, username }, password);
      },
      state: () => sessions.active?.state ?? null,
      screen: () => sessions.active?.snapshot() ?? "",
      type(text: string) {
        sessions.active?.write(new TextEncoder().encode(text));
      },
      count: () => sessions.list.length,
    },
  });
}
