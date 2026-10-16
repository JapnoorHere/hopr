# ADR 0003: Svelte 5 (runes) as a plain Vite SPA

**Status:** Accepted · 2026-10-16

## Context
The UI updates constantly (terminal tabs, transfer progress, host lists). The create-tauri-app template defaults to SvelteKit.

## Decision
Svelte 5 with runes and TypeScript strict, built by Vite as a single-page app. **No SvelteKit**: Hopr is one window with no URLs, SSR or routing, so the router and adapter would only add weight and config.

## Consequences
- A tiny runtime with fine-grained reactivity (the initial JS bundle is about 34 KB gzipped in Phase 0).
- Navigation is app state (drawers, tabs), not routes.
- Path aliases `$lib` and `$features` are configured in `vite.config.ts` and `tsconfig.json`.
