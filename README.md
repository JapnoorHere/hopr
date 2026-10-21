# Hopr

A fast, lightweight SSH client built with Tauri 2, Rust and Svelte 5. The full product and engineering plan is in [HOPR_PLAN.md](HOPR_PLAN.md).

## Status

| Phase | Scope | State |
|---|---|---|
| 0 | Foundations: workspace, tokens, themes, app shell, CI | Done |
| 1 | SSH → xterm data path (55 MB/s, no UI freeze) | Done |
| 2 | Vault, auth, saved hosts, keys, local terminal | Built (throughput check open) |
| 3 | Workspace UX: splits, command palette, jump hosts | Next |

## Develop

Prerequisites: Rust (stable), Node 24, pnpm, and on Windows the MSVC C++ Build Tools plus WebView2.

```sh
pnpm install
pnpm tauri dev        # run the app
cargo test --workspace && pnpm test
pnpm lint && pnpm check
cargo run -q -p hopr -- --export-bindings   # regenerate src/lib/ipc/bindings.ts
```

With [just](https://github.com/casey/just) installed: `just dev`, `just test`, `just lint`, `just bindings`.

## Layout

- `crates/`: Rust domain crates (`hopr-core`, later `hopr-ssh`, `hopr-vault`, …)
- `src-tauri/`: thin Tauri glue (commands only)
- `src/`: Svelte app, organized by feature (`src/features/*`), with shared code in `src/lib`
- `docs/adr/`: architecture decision records
