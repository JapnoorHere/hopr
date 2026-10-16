# ADR 0007: One actor per session

**Status:** Accepted · 2026-10-16

## Decision
Each connection is a tokio task that owns its state (russh handle, channels, forwards). Callers talk to it through a bounded `mpsc` command channel (write, resize, close), and it publishes state on a `watch` channel. The session manager only stores actor handles.

## Consequences
- No shared `Mutex` around sessions, and no lock held across `.await` (clippy `await_holding_lock` is set to deny).
- Each session gets its own `tracing` span (`session_id`, `host`) for debugging.
