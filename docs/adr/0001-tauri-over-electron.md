# ADR 0001: Tauri 2 over Electron

**Status:** Accepted · 2026-10-16

## Context
Hopr's core promise is "lightweight and fast". Electron bundles Chromium, which means 150 MB+ idle RAM and large installers.

## Decision
Use Tauri 2 (stable line, not the 3.x alpha). It uses the OS webview (WebView2, WKWebView, WebKitGTK), has a Rust backend, and supports mobile targets for Phase 8.

## Consequences
- Installers are about 5–10 MB, and idle RAM is a fraction of Electron's.
- Rendering differences between webviews must be tested, especially WebKitGTK (Risk R5).
- All privileged logic lives in Rust behind a capability allow-list.
