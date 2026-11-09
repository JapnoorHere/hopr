# Hopr coding standards

How we write code in Hopr: Rust (backend and crates), TypeScript and Svelte 5 (UI), CSS (design tokens). The goal is code that is **small, reusable, module-wise and easy to read**: fewer lines, one job per piece, shared helpers instead of copies.

This is the reference for new code. The refactor log at the end shows what was changed to bring the existing code in line and what is next; [CODE-AUDIT.md](./CODE-AUDIT.md) has the status of every source file.

---

## 1. Principles (all languages)

1. **One job per module, function and component.** If a name needs "and", split it.
2. **Reuse before you write.** Look in `src/lib/` (UI) and `crates/hopr-core` (Rust) first. A helper copied into a second file is a bug waiting to diverge: move it to a shared place instead.
3. **Pure logic separate from side effects.** Calculations (themes, filters, parsing, routing) live in plain functions that are tested without a UI or a network. Components and commands only wire them up.
4. **Validate at the boundary, trust inside.** Anything from outside (IPC, storage, files, the network, user input) is parsed once into a typed value (`parsePrefs`, `parseAppearance`, `parseCustom`). Code past that point does not re-check.
5. **Short over clever.** Prefer a clear early return to nested conditions; prefer a small named function to a long expression. No dead code, no commented-out code.
6. **Comments say why, not what.** One line above a block that explains the reason or the trap. Keep comment density similar to the surrounding code.
7. **Size limits (soft):** functions under ~40 lines, files under ~400 lines (Svelte components under ~300). Past that, split by responsibility (see the backlog).
8. **Names are the documentation.** Full words, no abbreviations except well-known ones (`id`, `url`, `ssh`).

---

## 2. Repository layout and module boundaries

| Area | Owns | Must not |
|---|---|---|
| `crates/hopr-core` | Shared models, `AppError`/`ErrorCode`, cross-crate helpers | Depend on Tauri or storage |
| `crates/hopr-ssh`, `hopr-sftp`, `hopr-term` | Protocols (SSH, SFTP, Telnet, serial) | Know about the UI or the vault |
| `crates/hopr-store` | SQLite access, migrations, backups, sync | Hold UI state |
| `crates/hopr-vault` | Crypto: Argon2id + XChaCha20-Poly1305 | Touch the database directly |
| `crates/hopr-import` | Parsing other clients' configs | Write to storage |
| `src-tauri/src` | **Thin glue**: Tauri commands, app state, events | Contain business logic |
| `src/lib` | Reusable UI primitives, design system, IPC, helpers | Import from `src/features` |
| `src/features/<area>` | One product area (hosts, terminal, sftp, profiles…) | Reach into another feature's internals |

Rules:
- **Dependencies point down only:** `features → lib`, `src-tauri → crates`, `crates → hopr-core`. Never the other way.
- A feature talks to another feature through its **store** (`*.svelte.ts`) or an exported function, never through its components' internals.
- New cross-cutting UI goes in `src/lib/ui`; new cross-cutting Rust goes in `hopr-core`.

---

## 3. Rust

### 3.1 Errors
- **Libraries (crates):** typed errors with `thiserror`, one enum per crate (`StoreError`, `VaultError`…). Never `anyhow` in a crate's public API.
- **IPC boundary:** every Tauri command returns `Result<T, AppError>`. Convert crate errors with `From` impls, never by string-matching.
- **Shared constructors, not copies.** `fn invalid(msg) -> AppError` is currently copied into 8 files; it belongs once in `hopr-core` (`AppError::invalid(msg)`), alongside `AppError::storage`, `AppError::internal`.
- `unwrap()` is already denied by clippy. Use `?`, `expect` only with a reason that proves it can't fail, and never on user or network data.
- Error messages are for people: what happened and what to do ("Switch to another profile before deleting this one"), never internal detail or secrets.

### 3.2 Tauri commands
- **Validate → delegate → map.** A command checks its input, calls a crate or service function, and maps the result. If it grows past ~25 lines, the logic belongs in a crate.
- One file per area (`library.rs`, `sftp.rs`, `profiles.rs`), registered once in `lib.rs`'s `specta_builder`.
- Regenerate bindings after changing a command: `cargo run -q -p hopr -- --export-bindings`. Never edit `bindings.ts` by hand.
- Every command must have a caller. Unused commands are either a missing feature or dead code; see `KNOWN-ISSUES.md`.

### 3.3 State, async and resources
- App state lives in managed structs (`AppState`, `Profiles`, `Files`). Prefer small focused structs over one god object.
- Anything that opens a resource (SSH session, SFTP session, file watcher, temp file) needs a matching close path, and that path must be called. Add a test or a debug assertion when practical.
- Long work runs on the async runtime; never block the main thread. Use `tokio::sync` primitives, keep locks short, never hold a lock across `.await`.

### 3.4 Style and tests
- `cargo fmt` and `cargo clippy --all-targets` clean (workspace lints: `unsafe_code = "deny"`, allowed only in `platform::set_quick_taskbar_identity` (the Win32 property store has no safe API) and `winmgr::on_this_desktop` (IVirtualDesktopManager, likewise), `unwrap_used = "deny"`). Consider enabling selected `clippy::pedantic` lints.
- Unit tests sit next to the code (`#[cfg(test)] mod tests`, at the **end** of the module). Integration tests in `crates/*/tests`.
- Test the rules, not the plumbing: validation, parsing, crypto round-trips, path safety, migrations.

---

## 4. TypeScript

- `strict`, `noUncheckedIndexedAccess` and `noImplicitOverride` are on; keep them on.
- **No `any`.** Use `unknown` for untrusted input and narrow it with a parser. `as` casts only right after a validated parse, with a comment.
- **Model states as discriminated unions** (`{ kind: "connecting" } | { kind: "open" } | { kind: "closed"; reason }`) and handle every case. Let TypeScript prove it: a function with a return type and a `switch` that returns in every case fails to compile when a case is missing, so no "unknown" fallback. When each case only maps to a value, use a lookup table typed `Record<Union, Value>` instead of a `switch` (see `CLOSED` in `terminal/state.ts`).
- **IPC results** always go through `wrap()` into `Result<T>`; check `.ok` before using `.data`. Show errors as inline messages or toasts, never `alert`.
- **Storage** goes through `src/lib/storage.ts` only: `readText`/`writeText`, `readJson(key, parse, fallback)`/`writeJson`, `removeKey`, and `scoped(key)` for profile-owned keys. It owns the try/catch, so callers never touch `localStorage` directly.
- **No import cycles.** When two modules need each other, the lower one exposes a setter and the higher one registers (see `setInFront` in `terminal/terminal-session.svelte.ts`).
- Pure helpers live in `.ts` files with tests (`*.test.ts` beside them). If a function doesn't need Svelte, it doesn't go in a `.svelte` file.

---

## 5. Svelte 5

### 5.1 Runes
- **`$derived` over `$effect`.** If a value can be computed from other state, derive it. `$effect` is only for talking to the outside world (DOM APIs, timers, IPC subscriptions), and it must return a cleanup when it starts something.
- **Never set state inside an effect to mirror other state.** That causes extra renders and loops.
- Use `untrack` when an effect reads something it must not re-run for (for example, initial prefs when creating the terminal).
- Use `$state.raw` for large objects that are replaced, not mutated (xterm instances, big lists).

### 5.2 Shared state
- Shared state is a **class in a `.svelte.ts` file**, exported as a single instance (`profiles`, `appearance`, `sessions`). Expose methods for changes; components don't mutate fields directly unless it's simple UI state like `picking`.
- Don't export reassignable `$state` from a module; export the instance or a getter.
- Keep stores free of UI concerns (toasts and dialogs belong in components).

### 5.3 Components
- **Props in, events out.** A component gets data via props and reports actions through callback props (`onclose`, `onpick`).
- Split when a component mixes jobs. Typical split: a container (data, IPC) plus presentational children.
- **Reuse UI primitives** from `src/lib/ui` instead of rebuilding them:
  - `Modal`: a dialog over the window. Esc, outside click, Tab wrap and focus return; dialogs stack, and only the top one answers Esc.
  - `Menu`: a button that opens actions. Outside click, Esc with focus back to the button, and arrow keys; items are `<button role="menuitem" class="menu-item">`.
  - `Segmented`: one choice from a few, with the sliding pill.
  - `Drawer`, `SelectMenu`, `IconButton`, `FieldError`, `Kbd`, `Count` and the toasts.
  - `ColorSwatches`: pick one colour from a few (profiles, onboarding).
  - `ConfirmBar`: an inline "are you sure?" with Cancel and the action; `tone="danger"` for things that can't be undone.
- Accessibility: real buttons for actions, labels for inputs, `role`/`aria-*` where native semantics aren't enough, and keyboard support for every pointer interaction.

---

## 6. CSS and design

- **Colours only through design tokens.** Raw colours are allowed only in `src/lib/design` (enforced by `scripts/check-tokens.mjs`). Themes are data in `themes.ts`; tokens are computed from them.
- **Readability is tested:** every theme must pass the contrast floors in `themes.test.ts` (text 7:1, muted 6:1, faint 4.5:1, status 3.5:1, accent 3:1).
- Motion goes through `src/lib/motion.ts` and the `--dur-*`/`--ease-*` tokens, and respects the Motion setting (`data-motion`).
- Prefer component-scoped styles; global styles live in `app.css` only for truly global patterns (checkboxes, sliders, focus).

---

## 7. Testing

| Layer | Tool | What to cover |
|---|---|---|
| Rust crates | `cargo test` | Parsing, crypto, storage, path safety, profile rules |
| TS logic | Vitest | Themes, filters, parsers, state helpers |
| UI flows | (to add) Playwright-style tests over CDP | Onboarding, profile switch, connect, settings search |

- A bug fix comes with a test that fails without the fix.
- Order-sensitive UI (for example Settings list vs. page sections) gets a cheap structural test (see `settings-order.test.ts`).

---

## 8. Before every commit

```
npx biome check --css-parse-tailwind-directives=true --write .
pnpm -s check        # svelte-check, no warnings
pnpm -s lint         # biome + token check
pnpm -s test         # vitest
cargo clippy -q --all-targets && cargo test -q   # when Rust changed
```

Commits: one line, imperative, describing the user-visible change.

---

## 9. Refactor log

The file-by-file status of every source file is in [CODE-AUDIT.md](./CODE-AUDIT.md).

### Done

| Change | Where |
|---|---|
| `AppError::invalid` and `AppError::internal` replace 14 copied error helpers | `crates/hopr-core/src/lib.rs`, 26 files |
| One storage module for every saved setting, with tests | `src/lib/storage.ts`, 11 callers |
| Dead `opacity` parameter removed; the terminal background takes only its brightness | `themes.ts` and 3 callers |
| Lint clean, with a reason written next to each deliberate exception | `app.css`, `tokens.css`, `themes.ts` |
| Session state labels: exhaustive by type, lookup table for close reasons | `terminal/state.ts` |
| `Segmented` replaces 7 hand-built radio rows | Settings, host editor, tunnels, hosts view |
| `Menu` replaces 5 hand-built menus and their window listeners | hosts view, files drawer, profile switcher |
| `Modal` replaces 3 hand-built dialogs | Settings, profile dialog, command palette |
| Theme data split from theme maths (806 → 298 + 503 data) | `themes.ts`, `themes.data.ts` |
| Hosts view split (784 → 568): quick connect, bulk bar, actions menu, address line (tested) | `src/features/hosts/` |
| `sftp.rs` (1,311) split: browse, transfers, enqueue, local, edit | `src-tauri/src/sftp/` |
| `sessions.rs` (968) split: registry, route, commands | `src-tauri/src/sessions/` |
| `library.rs` (784) split: hosts, keys, import, system | `src-tauri/src/library/` |
| `model.rs` (1,055) split: host, files, keys, snippets, tunnels, activity | `crates/hopr-core/src/model/` |
| Session store split from one terminal session, no import cycle | `terminal/session.svelte.ts`, `terminal-session.svelte.ts` |
| Shell integration and buffer reading out of the terminal component (754 → 526) | `terminal/integration.ts`, `terminal/buffer.ts` |
| Files drawer split (713 → 584): permissions card, preview, shared time format (tested) | `src/features/sftp/` |
| Settings split (603 → 485): sections list, reusable find-in-page | `shell/settings-sections.ts`, `src/lib/find-in-page.ts` |
| Host editor split (639 → 552): serial and proxy field groups | `hosts/SerialFields.svelte`, `hosts/ProxyFields.svelte` |
| Foreign importers: one file per source | `hopr-import/src/foreign/` |
| Tunnels: running tunnels apart from the commands | `src-tauri/src/tunnels/` |
| SFTP crate: downloads and uploads in their own module (527 → 380 + 157) | `hopr-sftp/src/transfer.rs` |
| Store: host rows and field writers, and the sync folder, in their own modules | `hopr-store/src/hosts/`, `hopr-store/src/sync/` |
| `ColorSwatches` and `ConfirmBar` replace 2 swatch rows and 5 confirm strips | `src/lib/ui/` |

The IPC surface didn't change: `bindings.ts` regenerates identical. Biome, svelte-check, vitest (164), clippy and `cargo test` (138) all pass.

### Next

1. End-to-end tests (see KNOWN-ISSUES #17); the CDP smoke script used for this refactor is a starting point.

---

## Sources

- [Svelte best practices (OpenReplay)](https://blog.openreplay.com/svelte-best-practices/)
- [State management with Svelte 5 runes (OpenReplay)](https://blog.openreplay.com/state-management-svelte-5-runes/)
- [Designing error types with thiserror and anyhow](https://oneuptime.com/blog/post/2026-01-25-error-types-thiserror-anyhow-rust/markdown)
- [Rust error handling in production](https://lucaberton.com/blog/rust-error-handling-production-2026/)
- [Tauri 2 best practices (community guide)](https://clawhub.ai/anjasta-tarigan/skills/tauri2-best-practice)
- [Tauri lifecycle and security (community guide)](https://skills.cat/skills/dchuk/claude-code-tauri-skills/understanding-tauri-lifecycle-security)
- [TypeScript best practices for large codebases](https://dev.to/the_beyond_horizon/typescript-best-practices-for-large-codebases-1ebj)
- [Strict TypeScript mode guidance](https://skills.cat/skills/svenja-dev/claude-code-skills/strict-typescript-mode)

Community guides were checked against how Hopr is already built; prefer the official docs (svelte.dev, v2.tauri.app, the TypeScript handbook, the Rust API guidelines) when they disagree.
