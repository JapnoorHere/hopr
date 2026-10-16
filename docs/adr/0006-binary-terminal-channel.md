# ADR 0006: Binary channel for terminal output

**Status:** Accepted · 2026-10-16

## Decision
Remote output flows Rust → UI through `tauri::ipc::Channel` as raw bytes, never as JSON strings or events. An output batcher flushes every 8–16 ms or at 64 KB, whichever comes first, and the UI writes `Uint8Array`s straight into xterm.js.

## Consequences
- There's no UTF-8 or JSON round-trip on the hot path, and multi-byte sequences split across packets are handled by xterm's own decoder.
- A bounded queue between the SSH reader and the batcher gives backpressure: a slow UI throttles the remote instead of growing RAM.
