# ADR 0004: SQLite via sqlx

**Status:** Accepted · 2026-10-16

## Context
Hosts, groups, keys, snippets and history need local, durable, queryable storage with migrations.

## Decision
SQLite in WAL mode, accessed through `sqlx`, with embedded migrations run at boot inside a transaction and an automatic DB backup before each migration.

## Consequences
- Async-friendly, with migrations built in.
- Secrets are never stored in plaintext columns: `_enc` columns hold XChaCha20-Poly1305 ciphertext (ADR 0005).
