# Hopr task runner. `just --list` shows everything.
set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# Install JS deps and fetch Rust crates.
setup:
    pnpm install
    cargo fetch

# Run the desktop app with hot reload.
dev:
    pnpm tauri dev

# All unit tests (Rust + TS).
test:
    cargo test --workspace
    pnpm test

# Lint, typecheck, format checks. CI runs exactly this.
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    pnpm lint
    pnpm check

# Regenerate src/lib/ipc/bindings.ts from the Rust command definitions.
bindings:
    cargo run -q -p hopr -- --export-bindings

# Production bundle for the current OS.
build:
    pnpm tauri build

# Supply-chain checks (needs `cargo install cargo-deny`).
deny:
    cargo deny check
