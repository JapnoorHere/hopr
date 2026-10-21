# ADR 0006: Binary channel for terminal output

**Status:** Accepted · 2026-10-16

## Decision
Remote output flows Rust → UI through `tauri::ipc::Channel` as raw bytes, never as JSON strings or events. An output batcher flushes every 12 ms or at 256 KiB, whichever comes first, and the UI writes `Uint8Array`s straight into xterm.js. The UI acks rendered bytes (coalesced to about one IPC call per frame), and the batcher pauses above 4 MiB unacknowledged.

Measured on Windows (WebView2): 64 KiB batches + per-batch acks gave 34 MB/s; 256 KiB batches + coalesced acks gave 55 MB/s with UI lag under 35 ms.

## Consequences
- There's no UTF-8 or JSON round-trip on the hot path, and multi-byte sequences split across packets are handled by xterm's own decoder.
- A bounded queue between the SSH reader and the batcher gives backpressure: a slow UI throttles the remote instead of growing RAM.
