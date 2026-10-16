# Hopr: Complete Product & Engineering Plan

**Version:** 1.0 (2026-10-16)
**Source:** `chat.txt` (stack research, feature list, UI direction, coding practices), plus gaps filled in by this plan (marked **[ADDED]**)
**Status:** In development. See [§17 Roadmap](#17-phased-roadmap-milestones-tasks-effort) for current phase

---

## Table of Contents
1. [Context & Vision](#1-context--vision)
2. [Personas, Use Cases, Non-Goals](#2-personas-use-cases-non-goals)
3. [Locked Decisions (Tech Stack)](#3-locked-decisions-tech-stack)
4. [System Architecture](#4-system-architecture)
5. [Data Model (SQLite Schema)](#5-data-model-sqlite-schema)
6. [Security & Threat Model](#6-security--threat-model)
7. [Design System](#7-design-system)
8. [App Shell & Navigation Model](#8-app-shell--navigation-model)
9. [Screen-by-Screen UI Specification](#9-screen-by-screen-ui-specification)
10. [Feature Specifications](#10-feature-specifications-with-acceptance-criteria)
11. [Keyboard Shortcut Map](#11-keyboard-shortcut-map)
12. [Repository Layout & Tooling](#12-repository-layout--tooling)
13. [Coding Standards](#13-coding-standards)
14. [Testing Strategy](#14-testing-strategy)
15. [CI/CD, Release, Updates](#15-cicd-release-updates)
16. [Performance Budgets](#16-performance-budgets)
17. [Phased Roadmap](#17-phased-roadmap-milestones-tasks-effort)
18. [Business, Legal, Launch](#18-business-legal-launch-added)
19. [Risks & Mitigations](#19-risks--mitigations)
20. [Definition of Done & Launch Checklist](#20-definition-of-done--launch-checklist)
21. [Open Questions](#21-open-questions)
22. [Appendix: What Was Added Beyond chat.txt](#22-appendix-what-was-added-beyond-chattxt)

---

## 1. Context & Vision

### 1.1 What Hopr is
Hopr is a fast, lightweight, beautiful cross-platform SSH client in the same category as Termius. It covers the terminal, the host vault, SFTP, port forwarding, snippets and sync. It's built on Tauri 2 + Rust + Svelte 5 so the installer stays at about 5–10 MB and idle RAM is a fraction of what Electron apps use.

### 1.2 Why it should exist
- Electron-based clients are heavy, at 150 MB+ idle RAM and large installers.
- Most SSH clients look like "VS Code with a sidebar" and have no identity.
- Jump-host chains, which are everyday life for anyone working behind bastions, are invisible in other clients.

### 1.3 Core identity: "hopping"
The path to a server is the hero of the UI:
- **Hop trail** in tabs: `laptop › bastion › prod-db`, and each segment is clickable.
- **Status as light**: a soft glow or colored edge (green = live, amber = reconnecting, red = dropped) instead of badge dots.
- **Command-bar-first**: Ctrl/Cmd+K is the primary way to get around.
- **Floating drawers**: the terminal fills the window edge to edge. SFTP, tunnels and snippets slide in as translucent drawers and close with Esc.
- **Constellation view** (later): a graph of hosts and their jump relationships.

### 1.4 Product principles
1. The terminal is the hero, and the chrome recedes.
2. Quiet by default, expressive when you interact.
3. Keyboard first, mouse friendly.
4. Local-first. It works fully offline, and sync is optional and zero-knowledge.
5. Secure by default. Nothing secret ever sits in plaintext on disk.
6. Fast is a feature, so every feature has a performance budget (§16).

### 1.5 Success metrics [ADDED]
| Metric | Target (v1.0) |
|---|---|
| Installer size | ≤ 12 MB (Win/mac), ≤ 15 MB (Linux AppImage) |
| Idle RAM, 1 SSH tab | ≤ 120 MB total (webview included) |
| Cold start to usable | ≤ 800 ms |
| Connect overhead added by Hopr | ≤ 50 ms over raw `ssh` |
| Terminal throughput (`cat` 100 MB) | No UI freeze, ≥ 50 MB/s sustained |
| Crash-free sessions | ≥ 99.5% |

---

## 2. Personas, Use Cases, Non-Goals

### 2.1 Personas [ADDED]
| Persona | Needs | Key features |
|---|---|---|
| Solo dev / indie hacker | A handful of VPSes, quick SSH, file edits | Quick connect, SFTP, snippets, themes |
| DevOps / SRE | Dozens to hundreds of hosts behind bastions, tunnels, incidents | Jump chains, tunnels, broadcast, cloud import, monitoring, history |
| Security-conscious admin | Hardware keys, certificates, audits | FIDO2, SSH certs, strict known-hosts, audit log, vault |
| Team lead (post-v1) | Shared hosts and credentials, onboarding | Team vaults, RBAC, SSO |
| Mobile on-call (post-v1) | Fix prod from a phone | Mobile app, extra keys row, sync |

### 2.2 Top user journeys
1. First launch → create vault → import `~/.ssh/config` → connect to a host in under 60 s.
2. Connect through `bastion → prod-db` and see the hop trail, then open SFTP on prod-db and drag in a file.
3. Start a saved tunnel (`localhost:5432 → prod-db:5432`) and watch its traffic in the dashboard.
4. Run a snippet with variables across 5 hosts with broadcast.
5. Restore yesterday's workspace (tabs + splits) with one keystroke.
6. Edit `/etc/nginx/nginx.conf` remotely in a local editor, with auto-upload on save.

### 2.3 Non-goals for v1.0
Team vaults, RBAC, SSO (Phase 7) · mobile apps (Phase 8) · live session sharing (Phase 7) · plugin marketplace (an API preview only in Phase 6) · RDP/VNC (out of scope).

---

## 3. Locked Decisions (Tech Stack)

### 3.1 Backend (Rust)
| Layer | Choice | Notes |
|---|---|---|
| Shell | **Tauri 2** | OS webview, mobile-capable |
| Language | **Rust 2024 edition**, stable | `rust-toolchain.toml` |
| Async | **tokio** | |
| SSH / SFTP | **russh** + **russh-sftp** | Pure Rust, async |
| Keys | russh key module / `ssh-key` | Parse, generate, fingerprint |
| Local PTY | **portable-pty** | ConPTY on Windows |
| Serial [ADDED] | `serialport` | |
| Telnet [ADDED] | Own minimal RFC 854 + NAWS/ECHO/SGA | |
| Storage | **SQLite via sqlx** (decision: compile-time checks + built-in migrations) | WAL |
| Crypto | argon2, chacha20poly1305 (XChaCha20-Poly1305), zeroize, secrecy, keyring | |
| Errors / logs | thiserror, anyhow, tracing (+ rolling files, redaction layer) | |
| IPC types | serde + specta + tauri-specta | |
| File watch [ADDED] | notify | Edit-locally auto-upload |
| Updates | tauri-plugin-updater | Signed payloads |
| Tauri plugins | dialog, fs (scoped), clipboard-manager, notification, global-shortcut, deep-link, single-instance, window-state, os, process | Least privilege |

### 3.2 Frontend
| Layer | Choice |
|---|---|
| Framework | **Svelte 5 (runes) + TypeScript strict** |
| Build | Vite |
| Styling | **Tailwind CSS v4** + CSS-variable tokens |
| Components | bits-ui (headless), restyled |
| Terminal | @xterm/xterm + webgl, fit, search, unicode11, web-links, serialize, image (later) |
| Palette | bits-ui Command + a custom frecency ranker |
| Splits | paneforge |
| Icons | Phosphor + 8 custom signature icons |
| Virtual lists | @tanstack/svelte-virtual |
| Graph view | d3-force on canvas (later) |
| Highlight | Shiki (lazy) |
| i18n [ADDED] | paraglide-js |
| Package manager | pnpm |

### 3.3 Tooling
Biome + svelte-check · clippy `-D warnings` + rustfmt · cargo-nextest · cargo-deny + cargo-audit + Renovate · Vitest · WebdriverIO + tauri-driver (E2E) + Playwright on a mocked-IPC web build (visual) · an in-process russh test server (local), testcontainers OpenSSH (CI) · cargo-fuzz + proptest · criterion · **just** · release-plz + Conventional Commits · GitHub Actions + tauri-action.

### 3.4 Alternatives rejected
Electron (weight) · Flutter (terminal maturity) · native egui/iced/GPUI (widget cost) · Go + Wails (ecosystem) · ssh2/libssh2 (sync, cross-compile pain) · React (runtime overhead).

---

## 4. System Architecture

### 4.1 Diagram
```
┌──────────────────────────── Webview (Svelte 5) ─────────────────────────────┐
│ features/terminal  vault  sftp  tunnels  snippets  palette  settings ...     │
│ xterm.js (WebGL) ◄── binary Channel ──┐   rune stores ◄── events            │
│ typed invoke() (tauri-specta) ────────┼──────────────────────────────────►  │
└───────────────────────────────────────┼─────────────────────────────────────┘
┌──────────────── src-tauri (thin glue: commands, validation, error mapping) ─┐
└───────────────────────────────────────┬─────────────────────────────────────┘
┌──────────────────────────────── hopr-core ──────────────────────────────────┐
│ SessionManager (actor registry)  EventBus  Settings  Workspaces             │
│   ├─ SshSessionActor (tokio task, owns russh Handle)                        │
│   │    ├─ Shell channels → OutputBatcher (8–16 ms / 64 KB) → Channel<bytes> │
│   │    ├─ SFTP → TransferQueue (bounded) → progress ≤10 Hz                  │
│   │    └─ Forward listeners (L / R / SOCKS5)                                │
│   ├─ LocalPtyActor · SerialActor · TelnetActor · MonitorActor               │
├─ hopr-ssh · hopr-sftp · hopr-vault · hopr-store · hopr-import · hopr-term   │
└─ hopr-sync (Phase 5) · hopr-cli                                             │
```

### 4.2 Rules
1. Thin UI, fat core.
2. One actor per session (mpsc commands, watch/broadcast state, no shared Mutex).
3. The hot path is binary: `tauri::ipc::Channel` raw bytes, batched every 8–16 ms or at 64 KB.
4. Bounded channels everywhere. When the UI is slow, output coalesces and window acks pause.
5. Typed IPC: bindings are generated, and CI fails if they're stale.
6. Errors cross the boundary as `AppError { code, message, detail?, retryable }`.

### 4.3 Session state machine
```
Idle → Connecting(hop i/n) → VerifyingHostKey ⇄ prompt → Authenticating(method) ⇄ prompt
     → Ready → (net loss) Reconnecting(k, backoff) → Ready
     → Closing → Closed(reason)        any → Failed(AppError)
```
A Rust enum with data. The UI mirrors it through `session-state` events, and tab glow comes from the state.

### 4.4 Data flows
- **Input:** `xterm.onData` → `session_write(id, bytes)` → actor → `channel.data()`.
- **Output:** russh Data → batcher → `Channel<Vec<u8>>` → `term.write(Uint8Array)`.
- **Resize:** ResizeObserver → fit → debounce 50 ms → `session_resize` → `window_change`.
- **Jump chain:** connect to A → `direct-tcpip` to B → SSH over the channel stream → repeat. Per-hop progress events light up the hop trail.
- **SFTP:** enqueue → bounded queue (concurrency 3) → 256 KB chunks + a token bucket → progress ≤10 Hz.
- **Unlock:** password → Argon2id → KEK → unwrap DEK → SecretBox in memory → per-field decrypt on demand.

### 4.5 Process & window model [ADDED]
Single instance (deep links forwarded) · main window + tear-off windows · quick-terminal dropdown window · tray (sessions, tunnels, lock, quit) · confirm close with active sessions / keep running in tray.

---

## 5. Data Model (SQLite Schema)
UUIDv7 ids, UTC ISO-8601 timestamps. `_enc` = XChaCha20-Poly1305 (nonce‖ct‖tag) under the DEK. Syncable tables have `created_at, updated_at, deleted_at, rev`.

```sql
vault_meta(id, version, kdf_algo, kdf_salt, kdf_m_cost, kdf_t_cost, kdf_p_cost,
           dek_wrapped_enc, recovery_wrapped_enc, verifier_enc, created_at)
app_settings(key PRIMARY KEY, value_json)
groups(id, parent_id, name, icon, color, sort_order, defaults_json)
tags(id, name UNIQUE, color)            host_tags(host_id, tag_id)
hosts(id, group_id, label, hostname, port, protocol, username, identity_id,
      auth_method_order_json, jump_chain_json, proxy_id, env_json, startup_commands_json,
      terminal_profile_id, theme_override, keepalive_s, auto_reconnect, agent_forward,
      x11_forward, compression, environment, notes_md, favorite, color, icon,
      last_connected_at, connect_count, template_id, cloud_source_json)
host_secrets(host_id PK, password_enc, totp_seed_enc)
identities(id, label, username, key_id, password_enc)
ssh_keys(id, label, algo, bits, fingerprint_sha256, public_key, private_key_enc,
         passphrase_enc, is_hardware, certificate, comment)
known_hosts(id, host_pattern, port, key_type, key_b64, fingerprint_sha256,
            first_seen_at, last_seen_at, source)
proxies(id, label, kind, host, port, username, password_enc)
tunnels(id, host_id, label, kind, bind_addr, bind_port, dest_host, dest_port, auto_start, enabled)
snippet_folders(id, parent_id, name)
snippets(id, folder_id, name, body, description, variables_json, shortcut, run_mode, tags_json)
command_history(id, host_id, session_id, command, cwd, exit_code, started_at, duration_ms)
workspaces(id, name, layout_json, is_autosave)
terminal_profiles(id, name, font_family, font_size, line_height, cursor_style, cursor_blink,
                  palette_id, scrollback, opacity)
themes(id, name, kind, json, is_builtin)
tasks(id, name, target_json, script, schedule_cron, last_run_at)
session_logs(id, host_id, started_at, ended_at, file_path, kind)
audit_events(id, ts, kind, host_id, detail_json)
connection_history(id, host_id, started_at, ended_at, end_reason, bytes_in, bytes_out)
sync_state(table_name, last_pulled_rev, last_pushed_rev)
sync_outbox(id, table_name, row_id, op, payload_enc, created_at)
```
Indexes on label, group, last_connected, history(host, time), known_hosts(pattern, port). FTS5 over hosts and snippets. Migrations run at boot in a transaction after an automatic DB backup. Paths: `{appData}/hopr.db`, `{appLog}/`, `{appData}/recordings/`, `{appCache}/edit/`. **Portable mode** when `hopr.portable` sits next to the exe.

---

## 6. Security & Threat Model
| Threat | Control |
|---|---|
| Disk theft or backup leak | Argon2id (64 MiB, t=3, p=1, calibrated to about 500 ms) → KEK wraps a random 256-bit DEK. Per-field XChaCha20-Poly1305 |
| Memory scraping | secrecy/zeroize, JIT decrypt, auto-lock wipes the DEK |
| MITM | TOFU prompt with SHA256 + randomart. A **changed key is a hard stop**, and overriding means typing the host label |
| Hostile remote output | Sanitize titles, file names, MOTD, OSC. OSC 52 = ask. Links need a modifier |
| Webview compromise | Strict CSP, capability allow-lists, no eval, private keys never reach JS |
| Agent forwarding abuse | Off by default, per-host, optional per-signature confirmation |
| Malicious update | Signed updater payloads. Signed + notarized binaries |
| Supply chain | Lockfiles, cargo-deny, cargo-audit, Renovate, pinned Action SHAs |
| Secrets in logs | Redaction layer + a log-leak test + a DB plaintext scan test |
| Brute force | KDF cost + UI backoff after 5 failures |
| Lost password [ADDED] | A recovery key wraps the DEK separately |
| Privacy | No telemetry. Crash reports opt-in and scrubbed |

**Lock:** idle timeout (default 15 min), lock on sleep/screen lock, manual Ctrl+L. Sessions stay connected (option: disconnect). **Biometrics:** macOS Keychain with user presence, Windows Hello (KeyCredentialManager), Linux password only, all isolated in `hopr-vault::platform`. **Audit log:** unlocks, connects, auth failures, host-key events, key export, secret reveal, tunnels, broadcasts, security setting changes. Local and exportable.

---

## 7. Design System

### 7.1 Principles
The terminal is the hero · quiet by default · **one accent** · radius 6–10 px · 1 px hairlines at about 8% · layered surfaces, no shadows · motion 150–200 ms ease-out, reduced-motion aware.
**Avoid:** purple-blue gradients, heavy glass, pure #000/#FFF, thick borders, big shadows, multiple accents, emoji icons, mixed icon styles, cramped tabs.

### 7.2 Tokens (`src/lib/design/tokens.css`)
Primitive (OKLCH per theme) → semantic (`--bg`, `--surface-1..4`, `--border`, `--text`, `--text-muted`, `--text-faint`, `--accent`, `--accent-fg`, `--success`, `--warning`, `--danger`, `--focus-ring`) → component. Spacing × `--density` (0.8 / 1 / 1.25). `--radius` (default 8). Motion `--dur-fast 120ms`, `--dur 160ms`, `--dur-slow 200ms`. `--blur` 12 px (drawers and palette only). Tailwind v4 `@theme` maps to these, and **raw colors in components are forbidden** (lint).

### 7.3 Themes
| Theme | Background | Accent | Feel |
|---|---|---|---|
| **Ember** (default) | `#0F0E0D` | amber `#F2A33A` | cozy, distinctive |
| Moss | `#0B100E` | lime `#B6F36A` | hacker, refined |
| Tide | `#0A1114` | aqua `#4FD1C5` | calm, techy |
| Paper (light) | `#F4F1EA` | vermilion `#E4572E` | rare light theme |
Plus high-contrast Ember/Paper [ADDED] and Auto (follow the OS).

### 7.4 Theme JSON
```json
{ "name": "Ember", "ui": { "bg": "oklch(0.16 0.01 60)", "surface": "oklch(0.2 0.012 60)",
  "border": "oklch(1 0 0 / 0.08)", "text": "oklch(0.93 0.01 80)", "accent": "oklch(0.78 0.15 70)" },
  "radius": 8, "density": "comfortable", "iconWeight": "light", "fontUi": "Geist",
  "fontMono": "Commit Mono", "blur": 12, "glow": true,
  "terminal": { "palette": "ember-ansi", "cursor": "bar", "opacity": 0.96 } }
```
Validated on import. **ANSI palettes are separate** from UI themes (Ember/Moss/Tide/Paper-ANSI, Dracula, Solarized, Gruvbox, Nord, Catppuccin, Tokyo Night, One Dark). Imports iTerm2, Windows Terminal and Alacritty formats [ADDED].

### 7.5 Typography
UI: **Geist Sans** (alternatives General Sans, Instrument Sans, Onest). Terminal: **Commit Mono** default, JetBrains Mono, Geist Mono, plus any installed monospace. Labels: **Departure Mono** (tiny uppercase). Scale 12/13/14/16/20, **13 px base**, tabular numerals for ports, IPs and speeds.

### 7.6 Icons
Phosphor with a user-selectable weight (default light), 16/20 px. Custom 1.5 px set: logo (hop-arc), hop, tunnel, vault, broadcast, constellation, snippet, quick-connect.

### 7.7 Motion
Palette fade+scale 140 ms · drawer slide 16 px 180 ms · connect glow sweep · reconnecting amber breathing pulse · hop segments light up in sequence · toasts slide 8 px (errors sticky).

### 7.8 Configurable axes
Accent, density, radius, icon weight, UI and terminal fonts, blur, glow, sidebar position, tab style (top / vertical / hop-trail), opacity, cursor, custom CSS.

---

## 8. App Shell & Navigation Model

### 8.1 Layout
```
┌─────────────────────────────────────────────────────────────────────────────┐
│ ◠ │ ⌁ laptop › bastion › prod-db PROD ✕ │ ⌁ laptop › staging-api │ ▭ local │ +  ⌘K  – □ ✕ │
├───┼─────────────────────────────────────────────────────────────────────────┤
│ ▤ │                                                       ┌───────────────┐ │
│ ▣ │                 TERMINAL (edge to edge)               │ Drawer (float │ │
│ ⇄ │                                                       │ or pinned)    │ │
│ ⟨⟩│                                                       └───────────────┘ │
│ ⚙ │                                         [transfer pill]                 │
├───┴─────────────────────────────────────────────────────────────────────────┤
│ ● Connected via 2 hops  38 ms  ↓1.2 MB/s ↑84 KB/s  ⇄ 2 tunnels  🔒  ⌘K      │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 8.2 Regions
Custom title/tab bar (drag region, native controls) · 48 px icon rail (Hosts, Files, Tunnels, Snippets, Keys, History, Monitoring; Search, Settings and Lock at the bottom; expands with Ctrl+B) · workspace (splits) · drawers (right by default; float or pin) · overlays (palette, modals, toasts) · status bar (optional).

### 8.3 Navigation rules
Every action is in the palette with its shortcut shown · Esc closes the top-most overlay · focus returns to the terminal · one floating drawer at a time.

### 8.4 Reference mockup & revisions (review of the first mockup)
Keep: Ember, hop-trail tabs, an edge-to-edge terminal, a files drawer with "Esc to close", a status bar with hops/latency/throughput, the Ctrl K hint, terminal colors.
Changes:
1. **Tab status dots → bottom-edge glow** by state. Active tab = raised surface + bright text, so amber isn't overloaded.
2. **PROD environment badge** (Departure Mono + a red hairline) on production hosts.
3. Tabs get a **20 px ✕**, a protocol icon, a **state tooltip**, and **clickable hop segments**.
4. **Remove the 4 theme swatches** from the status bar (one-accent rule). Theme lives in the palette and Settings.
5. The **active rail item matches the open drawer**. Every rail icon gets a tooltip with its shortcut.
6. Drawer: **translucent + blur when floating**, plus a **pin** to dock it (the terminal reflows).
7. Files drawer: **path bar, toolbar, file/folder icons, consistent indent grid** (fixes the `.env` misalignment), empty and loading states.
8. Upload progress becomes a **global transfer pill** with pause/cancel/ETA/count.
9. Status bar: **per-hop latency tooltip**, tunnel count, lock state, ⌘K on macOS.
10. **Hop-arc logo**, drag region, native window controls.
11. **WCAG AA contrast** for muted text.

---

## 9. Screen-by-Screen UI Specification

### 9.1 Splash / Boot [ADDED]
Shown only if boot takes more than 300 ms. Errors: DB corrupt / migration failed → Restore backup · Start fresh (keeps the old file) · Show file · Copy diagnostic.

### 9.2 First-Run Onboarding [ADDED]
1. **Welcome**: live theme cards.
2. **Create vault**: master password + confirm, a strength meter, a biometric toggle, "can't be recovered". Then the **recovery key** screen (Copy / Save .txt / Print, plus a required "I saved it" checkbox). KDF strength under Advanced.
3. **Import**: detected `~/.ssh/config`, `known_hosts`, `id_*` keys, PuTTY sessions, each with a count and a checkbox → preview → confirm.
4. **Terminal style**: font + size with a live preview.
5. **Done**: the "Press ⌘K anywhere" tip → Hosts.

### 9.3 Unlock
A blurred snapshot (or solid if "hide content when locked"), a card with the logo, "VAULT LOCKED", password, Unlock, and a biometric button. Wrong password shakes. After 5 failures there's a backoff countdown. "Use recovery key" → set a new password. Enter submits, and biometrics are tried automatically.

### 9.4 Hosts / Vault
Header: fuzzy search, view toggle (List / Grid / Constellation), sort, "+ New" (Host / Group / Import). Left: group tree (drag, nest, counts), Favorites, Recents, All, Tags. Rows: status edge light, color/icon, label, `user@host:port` muted, a mini hop trail (`via bastion`), tags, last connected, and on hover Connect / SFTP / ⋯. Context menu: Connect, Connect in split, SFTP, New window, Edit, Duplicate, Copy SSH command, Copy hostname, Favorite, Move, Tag, Start tunnels, Ping, Delete. Multi-select: bulk edit, move, tag, delete, connect all, broadcast. Ping: TCP probe for visible rows every 60 s. Empty state: New host · Import ~/.ssh/config · Quick connect. Keys: ↑↓ Enter, Ctrl+Enter split, E edit, Del, F favorite, / search, Space quick look. Drag a host to the tab bar or a pane to connect.

### 9.5 Host Editor
Tabs:
- **General**: label, hostname, port, protocol, group, tags, color, icon, favorite, environment (none/dev/staging/prod).
- **Authentication**: username, identity *or* inline method order (key / password / agent / keyboard-interactive), key picker/import, password (reveal is audited), save toggle, TOTP seed, agent forwarding.
- **Connection**: jump chain builder with a live hop-trail preview, proxy, keepalive, auto-reconnect + max, timeout, compression, algorithms, IPv4/6.
- **Terminal**: profile, palette, font size, encoding, backspace, startup commands, env vars (supports `{{secret:x}}`), initial dir, logging.
- **Tunnels**: inline list.
- **Notes**: Markdown.
- **Advanced**: X11, SendEnv, TERM, raw ssh_config lines, inherited fields with override toggles.

Footer: Cancel · **Test connection** [ADDED] · Save · Save & Connect. Validation includes jump-cycle detection. Bulk mode applies changed fields only.

### 9.6 Terminal Workspace & Tabs
**Tab:** [status glow] [protocol icon] `laptop › bastion › prod-db` [PROD] [activity dot] [bell] [✕]. Hop popover: info, latency, "shell on this hop", "SFTP on this hop". Long chains collapse to `laptop › … › prod-db`. Styles: top / vertical / hop-trail. Context: rename, duplicate, reconnect, disconnect, split, new window, SFTP, color, pin, close / others / right. Drag reorder, drag out to detach, middle-click close, Ctrl+1–9.
**Splits:** nested via paneforge, 1 px accent on the active pane, Ctrl+Alt+arrows, zoom pane.
**Pane states:**
- *Connecting*: hop trail lights up + a step log + Cancel.
- *Host key*: inline card.
- *Auth*: password / passphrase / dynamic keyboard-interactive / TOTP card with "save to vault".
- *Ready*.
- *Reconnecting*: amber strip "attempt 2/10, next in 4 s", buffer kept.
- *Disconnected*: reason + Reconnect (Enter) / Close (Esc).
- *Failed*: friendly error + details + Retry / Edit host.

**Features:** context menu, copy-on-select, middle-click paste, **multi-line paste warning**, Ctrl-click links, path actions, Ctrl+F search (regex/case/word), bell modes, font zoom. **Shell integration (OSC 133/7)** [ADDED]: command-finished notifications, exit markers, prompt jumping, history, cwd. Drop a file on the terminal → SFTP upload [ADDED].

### 9.7 Command Palette (Ctrl/Cmd+K)
640 px centered, surface-4 + blur, footer key hints. Modes: none = everything · `>` actions · `@` hosts · `#` snippets · `:` settings · `?` help. Typing `user@host:port` shows a quick-connect result. Frecency ranking. Tab opens secondary actions. Empty query shows recents, suggestions and open tabs.

### 9.8 Quick Connect
Ctrl+Shift+N. Accepts `user@host:port`, `ssh -p 2222 -J bastion user@host` and `ssh://`. Autocomplete from history. After connecting: "Save this host?".

### 9.9 SFTP Drawer / Dual Pane
Drawer (480 px, remote) or full dual pane (local | remote). Path bar (breadcrumbs, editable, bookmarks), toolbar (new folder/file, upload, download, rename, delete, permissions, hidden toggle, filter, view, sort), virtualized list (icon, name, size, modified, perms, owner), footer (count, selection, free space). Double-click opens or previews. Drag & drop in every direction including the OS. F2/Del/Enter. Context: open, **edit locally** (temp + watcher + auto-upload + mtime conflict dialog), download (to…), rename, duplicate, delete, permissions (rwx grid + octal + chown + recursive), copy path, copy scp command, compress/extract, properties, open terminal here. Compare & sync in 4b. States: skeleton, permission denied, disconnected, empty.

### 9.10 Transfer Queue
Bottom-right pill ("3 transfers · 42% · 12 MB/s") that expands to a list with direction, name, from→to, progress, speed, ETA, status, and pause/resume/cancel/retry/reveal. Global: pause/resume all, clear, concurrency 1–8, speed limits. Resume by offset, 3 retries, survives reconnect, conflict dialog with "apply to all", OS notification when the window is unfocused.

### 9.11 Tunnels
A list grouped by host: toggle (glows), L/R/D badge, `127.0.0.1:5432 → prod-db:5432`, hop trail, live ↑↓, connection count, 60 s sparkline. Form: kind segmented control with a **live diagram**, bind addr (warns on 0.0.0.0), port (auto-pick), destination, via host, auto-start with host / on launch [ADDED]. Port-in-use check. Headless sessions. Dashboard view with a per-connection log.

### 9.12 Snippets
Folders + search. Click runs it. Editor: name, folder, tags, description, mono body, variables `{{name}}`, `{{name:default}}`, `{{a|b|c}}`, `{{secret:x}}`, built-ins `{{host}} {{user}} {{date}} {{clipboard}}`, run mode (paste / paste+enter / confirm), shortcut. Variable form popover. Prod confirmation.

### 9.13 Broadcast
Mode A: live input mirroring to selected panes (accent stripe, a "Broadcasting to 5 · Stop" banner). Mode B: run on group via exec with parallelism and stop-on-fail, a results grid (exit, duration, output), and CSV/JSON export. Prod confirmation and a dry-run preview.

### 9.14 Keys Manager
List: label, algorithm badge, SHA256 fingerprint, used-by count, hardware/cert badges. Generate (Ed25519 / RSA 3072-4096 / ECDSA / sk-ed25519), import (OpenSSH / PEM / PPK), export (re-auth + audit), copy public, **deploy to host** [ADDED], attach certificate, rename, delete (blocked if in use). A read-only section for system agent keys.

### 9.15 Host Key Verification
TOFU card: fingerprint large, randomart, hop trail. Trust & connect / Connect once / Cancel. **Changed key:** a full-pane red takeover showing old vs new, with Disconnect as the primary button and the override requiring the host label to be typed (audited). Known Hosts table in Settings with import/export and strict mode.

### 9.16 Local Terminal
Shell detection: pwsh, Windows PowerShell, cmd, Git Bash, WSL distros, MSYS2 / `$SHELL`, `/etc/shells`. Custom profiles.

### 9.17 Serial & Telnet
Serial: port, baud, data bits, parity, stop bits, flow control, echo, line ending, hot-plug. Telnet: an "Unencrypted" danger edge.

### 9.18 Recording & Replay
asciicast v2 + text logs, a REC chip on the tab. Replay: play/pause, 0.5–8×, scrub, skip idle, search, export. Password input is never recorded.

### 9.19 History
Command history across hosts (exact with shell integration, approximate otherwise): search, filters, run again, copy, save as snippet. Connection history timeline + uptime. Disable globally or per host, exclude patterns, clear.

### 9.20 Workspaces
Save, restore and autosave the layout (windows, tabs, splits, hosts, cwd, drawers). Set one as the startup workspace.

### 9.21 Monitoring (Phase 6)
OS, uptime, CPU (sparkline, per core), load, RAM/swap, disks, network, a process table with SIGTERM/SIGKILL. Polls a script over an exec channel every 2 s. Docker/K8s: list, shell into, logs, restart, stop. Log viewer with highlight rules, level filter, follow, wrap.

### 9.22 Constellation (Phase 6)
A canvas graph of hosts and jump edges rooted at "laptop". Zoom/pan, node popovers, filters, force/radial/tree layouts.

### 9.23 Settings
General · Appearance (live preview) · Terminal · Connection defaults (+ watch ssh_config) · SFTP · Security (change password re-wraps the DEK, recovery key, biometrics, auto-lock, clipboard auto-clear, known hosts, audit log) · Keyboard (rebind, conflicts, presets, import/export, global hotkey) · Snippets & History · Notifications · Sync · Integrations (cloud, editor, AI) · Advanced (logs, diagnostics bundle, reset) · About (version, updates, licenses, report issue).

### 9.24 Import / Export Center
Sources: ssh_config (Include, Host *, ProxyJump), known_hosts (hashed), PuTTY, MobaXterm, SecureCRT, Termius CSV, `.hopr` backup, CSV. Flow: parse → preview with new/duplicate/conflict → strategy → import → summary. Export: encrypted `.hopr`, ssh_config, CSV, themes, keymap. Daily automatic encrypted backups (keep 7).

### 9.25 Cloud Inventory (Phase 6)
AWS, GCP, Azure, DO credentials in the vault → region discovery → mapping rules → a group with auto-refresh. Hosts that disappear get flagged.

### 9.26 Notifications
Toasts (max 3, errors sticky). OS notifications when the window is unfocused.

### 9.27 Global patterns
Confirm (safe default focus), secret prompt, error card, empty state (heading + line + actions), skeletons, thin progress line.

### 9.28 Quick Terminal
A global hotkey (Ctrl+`) slides down a window at 40% height with a local shell or a pinned host.

### 9.29 Tray
Show/hide, quick connect, recent hosts, tunnels, sessions, lock, settings, quit.

### 9.30 CLI & Deep Links
`hopr connect|sftp|tunnel|list|open|import` over a local socket with a 0600 token. `hopr://connect/<id>`, `hopr://quick?...`, an optional `ssh://` handler.

### 9.31 AI Assistant (Phase 6, opt-in)
BYO key (Claude default) or a local Ollama endpoint. Ctrl+I NL→command (never auto-run), explain selection, explain error chip, summarize logs. Redaction and a per-request preview.

### 9.32 Mobile (Phase 8)
Unlock → Hosts → full-screen terminal with an extra keys row, pinch zoom, swipe scrollback, bottom tabs, biometric plugin, reconnect on resume.

---

## 10. Feature Specifications (with Acceptance Criteria)

### 10.1 Core Terminal
| ID | Feature | Ph | AC |
|---|---|---|---|
| T1 | xterm + WebGL | 1 | 256 + truecolor correct. Falls back if WebGL is lost |
| T2 | Binary batched pipeline | 1 | `cat` 100 MB: input latency < 50 ms, memory flat ±30 MB |
| T3 | Resize | 1 | Remote size updates within 100 ms. vim/htop redraw |
| T4 | Tabs | 2 | Open/close/reorder/rename, Ctrl+1–9 |
| T5 | Splits | 3 | Nested, keyboard focus, zoom, persisted |
| T6 | Unicode/emoji/ligatures | 3 | unicode11 widths. Ligature toggle |
| T7 | Search | 3 | Regex/case/word with a match count |
| T8 | Copy/paste | 2 | Copy-on-select, bracketed paste, multi-line warning |
| T9 | Links & paths | 3 | Ctrl-click |
| T10 | Themes/fonts/cursor/opacity | 3 | Live apply |
| T11 | Local PTY | 2 | Shell detection. ConPTY resize |
| T12 | Logging & asciicast | 4 | Plays in asciinema. No password input recorded |
| T13 | Rebindable shortcuts | 3 | Registry, conflicts, import/export |
| T14 | Bell & notifications | 3 | |
| T15 | Shell integration [ADDED] | 4 | bash/zsh/fish/pwsh |
| T16 | OSC 52 gating [ADDED] | 3 | Deny/ask/allow |
| T17 | Image protocols [ADDED] | 6 | |
| T18 | Tear-off tabs [ADDED] | 3 | No reconnect |

### 10.2 Connection & Auth
| ID | Feature | Ph | AC |
|---|---|---|---|
| C1 | Password | 1 | Re-prompt up to 3 times |
| C2 | Public key (Ed25519/ECDSA/RSA) | 2 | Passphrase prompt. rsa-sha2 |
| C3 | System agent | 2 | Pageant / OpenSSH agent / SSH_AUTH_SOCK |
| C4 | Keyboard-interactive + TOTP | 2 | Dynamic prompts. TOTP autofill |
| C5 | Known hosts | 2 | TOFU + changed-key block |
| C6 | Jump chains | 3 | 3 hops, per-hop prompts and progress |
| C7 | SOCKS5/HTTP proxy | 3 | |
| C8 | Keepalive + auto-reconnect | 3 | Buffer kept |
| C9 | Certificates | 4 | |
| C10 | FIDO2 | 5 | Risk R2 |
| C11 | Agent forwarding | 3 | |
| C12 | X11 [ADDED] | 6 | |
| C13 | Telnet | 4 | |
| C14 | Serial | 4 | |
| C15 | Mosh | 7 | Risk R3 |
| C16 | Key manager | 2–3 | PPK |
| C17 | Deploy key [ADDED] | 3 | Idempotent, fixes permissions |
| C18 | ProxyCommand [ADDED] | 4 | stdio transport |

### 10.3 Hosts
H1 CRUD + undo (2) · H2 groups/tags/favorites/recents, search < 16 ms @ 5k (2) · H3 per-host settings (2–3) · H4 inheritance + templates (3) · H5 bulk edit/duplicate (3) · H6 ssh_config + known_hosts import (2) · H7 PuTTY/MobaXterm/SecureCRT/Termius (4) · H8 export (3) · H9 ping (3) · H10 notes (2) · H11 cloud (6) · H12 env badge [ADDED] (3) · H13 watch ssh_config [ADDED] (4).

### 10.4 SFTP
S1 drawer, 10k entries < 200 ms (4) · S2 dual pane (4) · S3 DnD incl. OS (4) · S4 queue survives reconnect (4) · S5 resume + throttle (4) · S6 edit locally (4) · S7 chmod/chown (4) · S8 file ops (4) · S9 archive (4b) · S10 preview (4) · S11 compare/sync (4b) · S12 bookmarks (4) · S13 SCP fallback (6).

### 10.5 Tunnels
P1 local, 100 concurrent connections · P2 remote · P3 SOCKS5 · P4 saved toggles · P5 auto-start · P6 dashboard 1 Hz · P7 via jump chain · P8 port-in-use + auto-pick. All Phase 5.

### 10.6 Productivity
R1 snippets (5) · R2 live broadcast < 10 ms added (5) · R3 run on group (5) · R4 autocomplete (6) · R5 history search (4) · R6 workspaces (3) · R7 quick connect (2) · R8 scripting (6) · R9 scheduled tasks (6) · R10 secret injection (3).

### 10.7 Security
X1 encrypted vault with a DB-bytes scan test (2) · X2 master password + recovery (2) · X3 biometrics (3) · X4 keychain (2) · X5 auto-lock (2) · X6 masking + log test (2) · X7 audit (3) · X8 ZK sync (5) · X9 clipboard auto-clear (3).

### 10.8 Sync & Teams
Y1 E2E sync with CRDT convergence (5) · Y2 team vaults (7) · Y3 RBAC (7) · Y4 shared snippets/tunnels (7) · Y5 session sharing (7) · Y6 SSO (7).

### 10.9 Monitoring (6)
M1 stats · M2 processes + kill · M3 Docker/K8s · M4 log tail · M5 history/uptime.

### 10.10 UX & Platform
U1 palette < 50 ms (3) · U2 light/dark/auto + CSS (3) · U3 three OS builds in CI (1+) · U4 mobile (8) · U5 menus, tray, global hotkey (3) · U6 signed auto-updates (3) · U7 CLI + deep links (6) · U8 plugins (6+) · U9 i18n (4) · U10 WCAG 2.2 AA + xterm screenReaderMode (1+).

### 10.11 Plugins [ADDED]
Sandboxed iframe/worker with a `plugin.json` permissions manifest and a brokered postMessage API. Extension points: palette commands, themes, snippet packs, status items, link matchers.

### 10.12 AI
NL→command, explain, error help, summarize, autocomplete. Opt-in, BYO key, local model option, redaction.

---

## 11. Keyboard Shortcut Map
| Action | Shortcut |
|---|---|
| Palette | Ctrl+K |
| Quick connect | Ctrl+Shift+N |
| Local terminal | Ctrl+Shift+` |
| New tab | Ctrl+T |
| Close tab/pane | Ctrl+W |
| Reopen closed tab | Ctrl+Shift+T |
| Next/prev tab | Ctrl+Tab / Ctrl+Shift+Tab |
| Tab N | Ctrl+1…9 |
| Split right/down | Ctrl+Shift+D / Ctrl+Shift+E |
| Focus pane | Ctrl+Alt+Arrows |
| Zoom pane | Ctrl+Shift+Enter |
| Sidebar | Ctrl+B |
| Hosts / Files / Tunnels / Snippets | Ctrl+Shift+H / F / U / S |
| Broadcast | Ctrl+Shift+B |
| Find | Ctrl+F |
| Copy/Paste | Ctrl+Shift+C/V (Win/Linux), Cmd+C/V (mac) |
| Font zoom | Ctrl+= / - / 0 |
| Prev/next prompt | Ctrl+↑/↓ |
| AI | Ctrl+I |
| Lock | Ctrl+L |
| Settings | Ctrl+, |
| Quick terminal (global) | Ctrl+` |

---

## 12. Repository Layout & Tooling
```
hopr/
├─ Cargo.toml (workspace) rust-toolchain.toml deny.toml clippy.toml rustfmt.toml
├─ package.json pnpm-lock.yaml biome.json tsconfig.json vite.config.ts svelte.config.js
├─ justfile
├─ crates/ hopr-core hopr-ssh hopr-sftp hopr-vault hopr-store hopr-import hopr-term hopr-sync hopr-cli
├─ src-tauri/ src/{main,lib}.rs src/commands/*.rs capabilities/*.json icons/ tauri.conf.json
├─ src/ lib/{ipc,design,icons,ui,stores,utils} features/{shell,terminal,hosts,palette,sftp,
│        transfers,tunnels,snippets,broadcast,keys,history,workspaces,settings,onboarding,unlock,...}
├─ tests/ integration/ e2e/ fixtures/
├─ docs/ adr/ architecture.md security.md theming.md
└─ .github/workflows/ ci.yml release.yml
```
**just:** setup · dev · test · test-int · e2e · lint · bindings · bench · fuzz · release · icons.
**Feature module:** `index.ts`, presentational `.svelte`, `*.state.svelte.ts` (logic), `*.api.ts` (IPC), `*.test.ts`.
**ADRs:** 0001 Tauri · 0002 russh · 0003 Svelte · 0004 sqlx · 0005 vault crypto · 0006 binary channel · 0007 actors · 0008 Biome · 0009 asciicast · 0010 sync.

---

## 13. Coding Standards
**Rust:** invalid states unrepresentable · no unwrap/expect outside tests (clippy deny) · no lock across await · bounded channels only · SecretString + zeroize · log-leak test · `forbid(unsafe_code)` except the reviewed platform module · tracing spans per session · thiserror → AppError.
**TS/Svelte:** strict, `noUncheckedIndexedAccess`, no `any` · small presentational components, logic in `.ts` · throttle to 10 fps · virtualize lists over 100 items · lazy-load heavy panels · reduced motion · a11y · **no raw colors outside `design/`** (lint).
**Security:** strict capabilities + CSP, no remote content, signed releases, pinned deps.
**Workflow:** trunk-based, small PRs, Conventional Commits, a PR template with a security checklist, CODEOWNERS for vault/capabilities/release, ADRs, ship the data path first.

---

## 14. Testing Strategy
| Level | Tool | Scope |
|---|---|---|
| Unit Rust | nextest | Crypto KATs, parsers, batcher, state machines |
| Unit TS | Vitest | Stores, ranking, shortcuts, theme validation |
| Component | @testing-library/svelte | Editor, palette, dialogs |
| Integration | in-process russh server (local) + testcontainers OpenSSH (CI) | Auth methods, jump chains, proxy, SFTP + resume, forwards, reconnect, certs, host-key change |
| E2E | WebdriverIO + tauri-driver | Onboarding → connect → SFTP → tunnel → lock |
| Visual | Playwright, mocked IPC | Screens × themes × densities |
| Fuzz | cargo-fuzz, proptest | ssh_config, known_hosts, PPK, CSV, theme JSON, sanitizer |
| Security | log-leak, DB scan, CSP, audit | |
| Perf | criterion | Throughput, SFTP, palette, startup |
| Manual QA | checklist | Ubuntu, Alpine/dropbear, FreeBSD, network gear, Windows OpenSSH, YubiKey, biometrics, HiDPI |
Coverage: ≥ 80% vault/import/store/parsers, ≥ 60% Rust overall, ≥ 70% TS logic.

---

## 15. CI/CD, Release, Updates
CI on PR (win/mac/linux): lint → bindings diff → nextest → vitest → integration (Linux) → cargo deny → debug build. Release on `v*`: tauri-action → MSI/NSIS, signed+notarized universal DMG, AppImage/deb/rpm, a signed `latest.json`, a draft release. Channels: stable / beta / nightly. Distribution: GitHub, winget, Homebrew, Scoop, AUR, Flathub later. Update UX: check at start + every 6 h, a toast, never forced, install on quit.

---

## 16. Performance Budgets
Cold start ≤ 800 ms · initial JS ≤ 250 KB gz · idle RAM ≤ 120 MB · +15 MB per tab · echo overhead ≤ 5 ms · flush 8–16 ms / 64 KB · palette ≤ 50 ms open, ≤ 16 ms per keystroke @ 5k hosts · SFTP ≥ 80% of OpenSSH · progress ≤ 10 Hz · ~0% idle CPU.

---

## 17. Phased Roadmap (Milestones, Tasks, Effort)
Effort is in ideal dev-weeks for one developer, plus 30% buffer. **Each phase is completed and verified end to end before the next starts.**

### Phase 0: Foundations (1.5 w) ✅ Done 2026-10-16
- [x] Tauri 2 + Svelte 5 + TS scaffold (plain Vite SPA, ADR 0003), Cargo workspace with `hopr-core`
- [x] Toolchain configs (rust-toolchain, clippy, rustfmt, deny, Biome, svelte-check)
- [x] justfile, CI skeleton (3 OS)
- [x] tauri-specta bindings pipeline (`hopr --export-bindings`; CI diff check)
- [x] Design tokens + Ember/Moss/Tide/Paper + density + Geist/Commit Mono + Phosphor
- [x] UI primitives (IconButton with tooltip, Kbd, Drawer); Dialog/Input/Toast are added in the phase that first needs them
- [x] App shell: title bar with drag region + window controls, icon rail, status bar (version over IPC), empty workspace, Settings drawer (theme + density, float/pin, Esc)
- [x] ADRs 0001–0008
- **Verified:** cargo test (3) + vitest (10) green · clippy/rustfmt/Biome/svelte-check/token check clean · production build 34 KB gz JS · native window runs (~30 MB process) · theme switch + Esc verified in browser
- Follow-ups: bundle Departure Mono (falls back to Geist Mono for now); macOS overlay title bar (Phase 3)
- **Exit:** a themed shell window builds and runs; lint and tests are green.

### Phase 1: Data Path (2 w) 🟡 In progress (2026-10-16)
> Built and passing: hopr-core session plumbing (batcher + flow control, 8 tests), hopr-ssh actor (7 integration tests incl. 8 MiB flood under backpressure), hopr-devsshd, Tauri session IPC with raw binary channel, xterm terminal + tabs + connect form + state overlays (29 TS tests), native E2E smoke `scripts/e2e-smoke.mjs` 7/7 PASS.
> **Open before exit:** bulk output through devsshd measured ~0.09 MB/s in the app (UI lag stayed 31 ms). Headless probe `cargo run --release -p hopr-ssh --example throughput` is running to isolate ConPTY/devsshd vs IPC/xterm. Fix, then re-run E2E and tick the boxes below.
- [ ] hopr-ssh: password connect, PTY shell, data/eof/exit
- [ ] hopr-core: session actor (write/resize/close), state watch
- [ ] OutputBatcher → Channel<bytes>
- [ ] Terminal component (xterm + webgl + fit + unicode11), temporary connect form, resize
- [ ] Backpressure test + bench, in-process SSH server integration test
- [ ] Session state UI (connecting / ready / closed / error)
- **Exit:** a real remote shell works, vim/htop are fine, a 100 MB cat doesn't freeze.

### Phase 2: Vault, Auth, Tabs (4 w)
Store + migrations · vault crypto + recovery + keyring + auto-lock + leak/scan tests · onboarding · unlock · hosts screen · host editor · key/agent/kbd-interactive/TOTP auth · TOFU + changed-key block · keys manager · tabs with glow · local PTY · quick connect · ssh_config + known_hosts import · paste behaviors · AppError cards. **Exit:** internal alpha.

### Phase 3: Workspace UX & Identity (4 w)
Hop-trail tabs + jump chains · proxies · keepalive/reconnect · splits + tear-off · palette · drawers infrastructure · all themes + ANSI palettes + import/export · terminal settings · shortcut registry · Settings · inheritance/templates/bulk/env badge · biometrics, clipboard clear, audit · workspaces · tray/menus/global hotkey/quick terminal · updater + signing · backups · ping, deploy key, PPK, agent forwarding. **Exit:** private beta v0.5.

### Phase 4: SFTP & Files (4 w + 1.5 w 4b)
hopr-sftp · transfer engine · drawer + dual pane · DnD · queue pill · edit locally · permissions · preview · drop-to-upload · shell integration · recording + replay · history · telnet, serial, ProxyCommand, certs · more importers · i18n. 4b: archive, compare/sync. **Exit:** public beta v0.8.

### Phase 5: Tunnels, Productivity, Sync (5 w)
L/R/D forwards · tunnels UI + dashboard · snippets · broadcast + run-on-group · secret injection · FIDO2 · sync v1. **Exit: v1.0 GA.**

### Phase 6: Power (6 w)
Monitoring, Docker/K8s, logs, uptime, constellation, cloud import, CLI + deep links, autocomplete, AI, scripting/tasks, plugin preview, images, X11, SCP, built-in agent.

### Phase 7: Teams (8+ w)
Team vaults, RBAC, shared items, admin, billing, SSO, session sharing, Mosh.

### Phase 8: Mobile (6–8 w)
iOS/Android targets, mobile layout, extra keys, biometrics, resume reconnect, stores.

| Phase | Weeks | Cumulative | Milestone |
|---|---|---|---|
| 0 | 1.5 | 1.5 | Skeleton |
| 1 | 2 | 3.5 | Data path |
| 2 | 4 | 7.5 | Alpha |
| 3 | 4 | 11.5 | Beta v0.5 |
| 4 | 5.5 | 17 | Beta v0.8 |
| 5 | 5 | 22 | **v1.0** (~29 w with buffer) |

**MVP cut:** tabs/splits, vault, key/password auth, terminal, SFTP + queue, port forwarding, snippets, ssh_config import, encrypted storage, palette. Sync, teams and AI come after.

---

## 18. Business, Legal, Launch [ADDED]
- **Model:** open core. Free local app · Pro ~$5/mo (sync, mobile, AI credits) · Team ~$10/user/mo.
- **License:** GPL-3.0/AGPL vs MIT/Apache, decided by ADR. A cargo-deny license allow-list. Fonts are OFL, Phosphor is MIT. Third-party notices.
- **Legal:** privacy policy, ToS (sync), a **trademark search for "Hopr"**, crypto export self-classification.
- **Sync infrastructure:** axum or Workers + R2/S3 + Postgres/D1, passkeys/magic links, rate limiting.
- **Crash reports:** opt-in, self-hosted GlitchTip/Sentry, scrubbed.
- **Docs & community:** Starlight docs, Ember landing page, Discussions/Discord, a community themes repo.
- **Launch:** private beta (50) → public beta (HN, Reddit) → v1.0 (Product Hunt, a benchmark blog post vs Electron).

---

## 19. Risks & Mitigations
| # | Risk | Mitigation |
|---|---|---|
| R1 | russh gaps or edge servers | Early server matrix, contribute upstream, keep the hopr-ssh abstraction |
| R2 | FIDO2 client support | Spike in Phase 2. Fall back to the system agent. ctap-hid-fido2 |
| R3 | No Rust Mosh | Defer. Bundle mosh-client (GPL, separate process) or implement SSP |
| R4 | Desktop biometrics FFI | Isolated module, password fallback |
| R5 | WebKitGTK perf/WebGL | Early Linux testing, canvas fallback |
| R6 | Terminal perf | Batching, backpressure, benchmarks |
| R7 | ConPTY quirks | portable-pty, Windows QA |
| R8 | Sync data loss | Soft deletes, outbox, CRDT tests, backups, ship as beta |
| R9 | Scope creep | Phase gates, MVP cut |
| R10 | Trademark | Search in Phase 0 |
| R11 | Signing cost/delay | Procure in Phase 2 |
| R12 | Store sandboxing | Direct + package managers first |

---

## 20. Definition of Done & Launch Checklist
**DoD per feature:** ACs met · tests, CI green on 3 OSes · strings externalized · keyboard + palette reachable · works in all themes and densities · empty/loading/error states · no secrets in logs, IPC validated, minimal capabilities · docs updated · budget checked.
**v1.0:** MVP done, no P0/P1 · security review · signed builds + updater 0.8→1.0 tested · manual QA matrix · docs, landing page, privacy · package managers · crash opt-in + feedback · benchmark post · 5k-host vault migration tested.

---

## 21. Open Questions
1. License and business model (assumed open core).
2. Mobile priority (assumed Phase 8).
3. Sync in 1.0 or 1.1 (assumed 1.0 beta, droppable).
4. AI default and hosted credits (assumed BYO key).
5. Team size (estimates are for one developer).
6. Final logo, and the trademark check.

---

## 22. Appendix: What Was Added Beyond chat.txt
Success metrics, personas, journeys, non-goals · locked decisions · full schema, state machine, data flows, process model · threat model, recovery key, unlock backoff, clipboard auto-clear, OSC 52 gating, portable mode · screens: boot, onboarding, unlock/recovery, import center, quick terminal, tray, history, workspaces, replay, known hosts, settings detail, mobile · shell integration · environment badges, Test connection, tunnel diagrams, port checks · extra importers, ssh_config watch, ProxyCommand, SCP fallback · high-contrast themes, ANSI imports, token lint, visual tests · plugin sandbox, local AI, AI privacy · perf budgets, distribution, crash policy, docs/launch, licensing, trademark, business model, risks, DoD · §8.4 mockup review.
