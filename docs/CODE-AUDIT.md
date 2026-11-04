# Code audit, file by file

Every source file (Rust, TypeScript, Svelte; generated `bindings.ts` left out) checked against
[CODING-STANDARDS.md](./CODING-STANDARDS.md). Size guide: about 400 lines for code files and 300
for Svelte components, tests excluded. Generated on 2026-10-25.

**Status:** ✓ within the guide · **Split** already broken up in this refactor · **OK** over the
guide for a stated reason · **Next** a planned split (see the backlog in CODING-STANDARDS.md).

Checks that apply to every file and pass across the codebase today: no `any`, no raw colours
outside `src/lib/design`, no `unwrap()` in Rust, browser storage only through
`src/lib/storage.ts`, menus, dialogs and segmented controls through the shared components,
Biome, svelte-check and clippy clean.

| Area | Files | Lines |
|---|---|---|
| `crates/hopr-core` | 11 | 2,031 |
| `crates/hopr-devsshd` | 1 | 775 |
| `crates/hopr-import` | 7 | 1,524 |
| `crates/hopr-sftp` | 4 | 1,277 |
| `crates/hopr-ssh` | 13 | 4,594 |
| `crates/hopr-store` | 14 | 3,875 |
| `crates/hopr-term` | 5 | 1,061 |
| `crates/hopr-vault` | 1 | 385 |
| `src-tauri/src` | 35 | 7,564 |
| `src/App.svelte` | 1 | 292 |
| `src/features/broadcast` | 2 | 117 |
| `src/features/grouprun` | 4 | 498 |
| `src/features/history` | 6 | 939 |
| `src/features/hosts` | 15 | 2,408 |
| `src/features/keys` | 2 | 549 |
| `src/features/palette` | 4 | 313 |
| `src/features/profiles` | 6 | 882 |
| `src/features/settings` | 14 | 1,743 |
| `src/features/sftp` | 10 | 1,812 |
| `src/features/shell` | 9 | 1,018 |
| `src/features/snippets` | 4 | 671 |
| `src/features/sync` | 1 | 82 |
| `src/features/terminal` | 28 | 4,113 |
| `src/features/tunnels` | 4 | 615 |
| `src/features/vault` | 6 | 1,209 |
| `src/features/workspace` | 7 | 1,167 |
| `src/lib/design` | 8 | 1,390 |
| `src/lib/download.ts` | 1 | 9 |
| `src/lib/find-in-page.ts` | 1 | 66 |
| `src/lib/fuzzy.test.ts` | 1 | 16 |
| `src/lib/fuzzy.ts` | 1 | 33 |
| `src/lib/i18n` | 4 | 218 |
| `src/lib/icons` | 1 | 111 |
| `src/lib/ipc` | 1 | 6 |
| `src/lib/motion.ts` | 1 | 243 |
| `src/lib/platform.test.ts` | 1 | 13 |
| `src/lib/platform.ts` | 1 | 27 |
| `src/lib/shortcuts.test.ts` | 1 | 75 |
| `src/lib/shortcuts.ts` | 1 | 161 |
| `src/lib/storage.test.ts` | 1 | 78 |
| `src/lib/storage.ts` | 1 | 71 |
| `src/lib/ui` | 12 | 1,212 |
| `src/lib/window.ts` | 1 | 12 |
| `src/main.ts` | 1 | 45 |
| **Total** | **253** | **45,300** |

## `crates/hopr-core`

| File | Lines | Status |
|---|---|---|
| `src/cast.rs` | 178 | ✓ |
| `src/hotkey.rs` | 61 | ✓ |
| `src/lib.rs` | 294 | ✓ |
| `src/model/activity.rs` | 90 | ✓ |
| `src/model/files.rs` | 41 | ✓ |
| `src/model/host.rs` | 491 | **OK** host types and their validation (split out of the old model.rs). |
| `src/model/keys.rs` | 88 | ✓ |
| `src/model/mod.rs` | 158 | ✓ |
| `src/model/snippets.rs` | 109 | ✓ |
| `src/model/tunnels.rs` | 122 | ✓ |
| `src/session.rs` | 399 | ✓ |

## `crates/hopr-devsshd`

| File | Lines | Status |
|---|---|---|
| `src/main.rs` | 775 | Dev-only test SSH server; low priority. |

## `crates/hopr-import`

| File | Lines | Status |
|---|---|---|
| `src/export.rs` | 209 | ✓ |
| `src/foreign/csv.rs` | 110 | ✓ |
| `src/foreign/mobaxterm.rs` | 82 | ✓ |
| `src/foreign/mod.rs` | 311 | ✓ |
| `src/foreign/putty.rs` | 157 | ✓ |
| `src/foreign/securecrt.rs` | 141 | ✓ |
| `src/lib.rs` | 514 | **OK** 405 lines of code, the rest tests. |

## `crates/hopr-sftp`

| File | Lines | Status |
|---|---|---|
| `src/lib.rs` | 380 | ✓ |
| `src/server.rs` | 370 | ✓ |
| `src/transfer.rs` | 157 | ✓ |
| `tests/sftp.rs` | 370 | ✓ test |

## `crates/hopr-ssh`

| File | Lines | Status |
|---|---|---|
| `examples/throughput.rs` | 91 | ✓ |
| `src/agent.rs` | 91 | ✓ |
| `src/auth.rs` | 296 | ✓ |
| `src/command.rs` | 146 | ✓ |
| `src/connect.rs` | 450 | **OK** the connect sequence reads top to bottom. |
| `src/exec.rs` | 252 | ✓ |
| `src/forward.rs` | 350 | ✓ |
| `src/keys.rs` | 592 | **OK** 332 lines of code, the rest tests. |
| `src/known.rs` | 158 | ✓ |
| `src/lib.rs` | 360 | ✓ |
| `src/proxy.rs` | 382 | ✓ |
| `src/totp.rs` | 67 | ✓ |
| `tests/session.rs` | 1359 | Integration tests against a real SSH server; size is fine for tests. |

## `crates/hopr-store`

| File | Lines | Status |
|---|---|---|
| `src/audit.rs` | 107 | ✓ |
| `src/backup.rs` | 268 | ✓ |
| `src/history.rs` | 412 | **OK** 304 lines of code, the rest tests. |
| `src/hosts/fields.rs` | 116 | ✓ |
| `src/hosts/mod.rs` | 706 | **Split** rows and field writers moved out; 447 lines of host queries, the rest tests. |
| `src/hosts/rows.rs` | 94 | ✓ |
| `src/keys.rs` | 265 | ✓ |
| `src/known.rs` | 172 | ✓ |
| `src/lib.rs` | 239 | ✓ |
| `src/snippets.rs` | 245 | ✓ |
| `src/sync/folder.rs` | 102 | ✓ |
| `src/sync/mod.rs` | 734 | **Split** the sync folder moved out; 538 lines of database sync, the rest tests. |
| `src/tunnels.rs` | 243 | ✓ |
| `src/workspaces.rs` | 172 | ✓ |

## `crates/hopr-term`

| File | Lines | Status |
|---|---|---|
| `src/lib.rs` | 308 | ✓ |
| `src/serial.rs` | 188 | ✓ |
| `src/telnet.rs` | 372 | ✓ |
| `tests/spawn.rs` | 54 | ✓ test |
| `tests/telnet.rs` | 139 | ✓ test |

## `crates/hopr-vault`

| File | Lines | Status |
|---|---|---|
| `src/lib.rs` | 385 | ✓ |

## `src-tauri/src`

| File | Lines | Status |
|---|---|---|
| `backups.rs` | 186 | ✓ |
| `biometric.rs` | 51 | ✓ |
| `clipboard.rs` | 66 | ✓ |
| `commands.rs` | 10 | ✓ |
| `foreign.rs` | 264 | ✓ |
| `grouprun.rs` | 248 | ✓ |
| `history.rs` | 219 | ✓ |
| `lib.rs` | 324 | ✓ |
| `library/hosts.rs` | 254 | ✓ |
| `library/import.rs` | 214 | ✓ |
| `library/keys.rs` | 245 | ✓ |
| `library/mod.rs` | 85 | ✓ |
| `library/system.rs` | 24 | ✓ |
| `main.rs` | 6 | ✓ |
| `platform.rs` | 434 | **OK** small window and OS commands, grouped. |
| `profiles.rs` | 492 | **OK** 396 lines of code, the rest tests. |
| `recording.rs` | 280 | ✓ |
| `sessions/commands.rs` | 392 | ✓ |
| `sessions/mod.rs` | 208 | ✓ |
| `sessions/route.rs` | 390 | ✓ |
| `sftp/browse.rs` | 213 | ✓ |
| `sftp/edit.rs` | 211 | ✓ |
| `sftp/enqueue.rs` | 238 | ✓ |
| `sftp/local.rs` | 71 | ✓ |
| `sftp/mod.rs` | 117 | ✓ |
| `sftp/transfers.rs` | 515 | **OK** the transfer queue in one place (split out of the old 1,311-line sftp.rs). |
| `snippets.rs` | 174 | ✓ |
| `state.rs` | 281 | ✓ |
| `sync.rs` | 259 | ✓ |
| `tunnels/commands.rs` | 132 | ✓ |
| `tunnels/mod.rs` | 86 | ✓ |
| `tunnels/runtime.rs` | 312 | ✓ |
| `updates.rs` | 108 | ✓ |
| `vault.rs` | 382 | ✓ |
| `workspaces.rs` | 73 | ✓ |

## `src/App.svelte`

| File | Lines | Status |
|---|---|---|
| `` | 292 | ✓ |

## `src/features/broadcast`

| File | Lines | Status |
|---|---|---|
| `BroadcastBanner.svelte` | 47 | ✓ |
| `broadcast.svelte.ts` | 70 | ✓ |

## `src/features/grouprun`

| File | Lines | Status |
|---|---|---|
| `RunOnHosts.svelte` | 348 | **OK** slightly over; 141 lines are CSS. |
| `export.test.ts` | 41 | ✓ test |
| `export.ts` | 34 | ✓ |
| `grouprun.svelte.ts` | 75 | ✓ |

## `src/features/history`

| File | Lines | Status |
|---|---|---|
| `HistoryDrawer.svelte` | 305 | **OK** slightly over. |
| `Replay.svelte` | 315 | **OK** slightly over. |
| `cast.test.ts` | 44 | ✓ test |
| `cast.ts` | 94 | ✓ |
| `history.svelte.ts` | 149 | ✓ |
| `prefs.svelte.ts` | 32 | ✓ |

## `src/features/hosts`

| File | Lines | Status |
|---|---|---|
| `BulkBar.svelte` | 108 | ✓ |
| `HostActionsMenu.svelte` | 61 | ✓ |
| `HostEditor.svelte` | 551 | **Split** serial and proxy field groups moved out; the rest is one form and its validation. |
| `HostsView.svelte` | 568 | **Split** quick connect, bulk bar, actions menu and address line moved out. |
| `ImportDrawer.svelte` | 281 | ✓ |
| `ProxyFields.svelte` | 100 | ✓ |
| `QuickConnect.svelte` | 65 | ✓ |
| `SerialFields.svelte` | 96 | ✓ |
| `chain.test.ts` | 73 | ✓ test |
| `chain.ts` | 51 | ✓ |
| `describe.test.ts` | 69 | ✓ test |
| `describe.ts` | 26 | ✓ |
| `filter.test.ts` | 93 | ✓ test |
| `filter.ts` | 93 | ✓ |
| `library.svelte.ts` | 173 | ✓ |

## `src/features/keys`

| File | Lines | Status |
|---|---|---|
| `DeployKeyDrawer.svelte` | 136 | ✓ |
| `KeysDrawer.svelte` | 413 | **OK** slightly over. |

## `src/features/palette`

| File | Lines | Status |
|---|---|---|
| `Palette.svelte` | 241 | ✓ |
| `frecency.test.ts` | 16 | ✓ test |
| `frecency.ts` | 39 | ✓ |
| `palette-state.svelte.ts` | 17 | ✓ |

## `src/features/profiles`

| File | Lines | Status |
|---|---|---|
| `ProfileAvatar.svelte` | 33 | ✓ |
| `ProfileDialog.svelte` | 212 | ✓ |
| `ProfilePicker.svelte` | 289 | **OK** slightly over; 155 lines are CSS. |
| `ProfileSwitcher.svelte` | 151 | ✓ |
| `ProfilesSettings.svelte` | 99 | ✓ |
| `profiles.svelte.ts` | 98 | ✓ |

## `src/features/settings`

| File | Lines | Status |
|---|---|---|
| `AuditDrawer.svelte` | 104 | ✓ |
| `BackupSettings.svelte` | 199 | ✓ |
| `DesktopSettings.svelte` | 129 | ✓ |
| `HistorySettings.svelte` | 134 | ✓ |
| `KeyboardSettings.svelte` | 90 | ✓ |
| `SyncSettings.svelte` | 195 | ✓ |
| `TerminalSettings.svelte` | 177 | ✓ |
| `ThemePicker.svelte` | 352 | **OK** gallery plus the custom theme editor; 121 lines are CSS. |
| `TransfersSettings.svelte` | 62 | ✓ |
| `UpdateToast.svelte` | 37 | ✓ |
| `WorkspacesSettings.svelte` | 112 | ✓ |
| `audit.test.ts` | 33 | ✓ test |
| `audit.ts` | 70 | ✓ |
| `updates.svelte.ts` | 49 | ✓ |

## `src/features/sftp`

| File | Lines | Status |
|---|---|---|
| `ClashBar.svelte` | 31 | ✓ |
| `DualPane.svelte` | 320 | **OK** slightly over. |
| `FilePreview.svelte` | 51 | ✓ |
| `FilesDrawer.svelte` | 582 | **Split** preview and permissions moved out; the rest is the file list. |
| `PermissionsCard.svelte` | 99 | ✓ |
| `TransferPill.svelte` | 229 | ✓ |
| `files.svelte.ts` | 259 | ✓ |
| `format.test.ts` | 80 | ✓ test |
| `format.ts` | 102 | ✓ |
| `transfers.svelte.ts` | 59 | ✓ |

## `src/features/shell`

| File | Lines | Status |
|---|---|---|
| `QuickTerminal.svelte` | 88 | ✓ |
| `Rail.svelte` | 124 | ✓ |
| `SettingsDrawer.svelte` | 483 | **Split** sections list and find-in-page moved out; the rest is layout, scroll spy and the sections. |
| `StatusBar.svelte` | 63 | ✓ |
| `TitleBar.svelte` | 85 | ✓ |
| `settings-order.test.ts` | 15 | ✓ test |
| `settings-sections.ts` | 86 | ✓ |
| `shell.state.svelte.ts` | 52 | ✓ |
| `shell.state.test.ts` | 22 | ✓ test |

## `src/features/snippets`

| File | Lines | Status |
|---|---|---|
| `SnippetsDrawer.svelte` | 328 | **OK** slightly over. |
| `snippets.svelte.ts` | 184 | ✓ |
| `template.test.ts` | 51 | ✓ test |
| `template.ts` | 108 | ✓ |

## `src/features/sync`

| File | Lines | Status |
|---|---|---|
| `sync.svelte.ts` | 82 | ✓ |

## `src/features/terminal`

| File | Lines | Status |
|---|---|---|
| `AuthPromptCard.svelte` | 66 | ✓ |
| `ConnectCard.svelte` | 130 | ✓ |
| `ConnectRoute.svelte` | 238 | ✓ |
| `HostKeyCard.svelte` | 89 | ✓ |
| `SearchBar.svelte` | 118 | ✓ |
| `TabStrip.svelte` | 217 | ✓ |
| `Terminal.svelte` | 526 | **Split** shell integration and buffer reading moved out; the rest is xterm setup. |
| `TerminalPane.svelte` | 433 | **OK** slightly over; 139 lines are CSS. |
| `buffer.ts` | 27 | ✓ |
| `connect.test.ts` | 72 | ✓ test |
| `connect.ts` | 139 | ✓ |
| `debug.ts` | 217 | ✓ |
| `glidecursor.ts` | 93 | ✓ |
| `integration.ts` | 241 | ✓ |
| `paste.test.ts` | 43 | ✓ test |
| `paste.ts` | 41 | ✓ |
| `prefs.svelte.ts` | 21 | ✓ |
| `prefs.test.ts` | 26 | ✓ test |
| `prefs.ts` | 87 | ✓ |
| `session.svelte.ts` | 314 | ✓ |
| `sessions.test.ts` | 75 | ✓ test |
| `shellint.test.ts` | 88 | ✓ test |
| `shellint.ts` | 169 | ✓ |
| `state.test.ts` | 54 | ✓ test |
| `state.ts` | 52 | ✓ |
| `target.test.ts` | 24 | ✓ test |
| `target.ts` | 53 | ✓ |
| `terminal-session.svelte.ts` | 460 | **OK** one session's lifecycle (split out of the old session store). |

## `src/features/tunnels`

| File | Lines | Status |
|---|---|---|
| `TunnelsDrawer.svelte` | 407 | **OK** slightly over. |
| `format.test.ts` | 45 | ✓ test |
| `format.ts` | 39 | ✓ |
| `tunnels.svelte.ts` | 124 | ✓ |

## `src/features/vault`

| File | Lines | Status |
|---|---|---|
| `Onboarding.svelte` | 649 | **Kept** one wizard whose steps share state; 286 of its lines are CSS. |
| `SecuritySettings.svelte` | 135 | ✓ |
| `UnlockScreen.svelte` | 226 | ✓ |
| `strength.test.ts` | 17 | ✓ test |
| `strength.ts` | 16 | ✓ |
| `vault.svelte.ts` | 166 | ✓ |

## `src/features/workspace`

| File | Lines | Status |
|---|---|---|
| `Workspace.svelte` | 196 | ✓ |
| `commands.ts` | 470 | **OK** the command list for the palette; data-like. |
| `layout.test.ts` | 34 | ✓ test |
| `layout.ts` | 152 | ✓ |
| `snapshot.test.ts` | 80 | ✓ test |
| `snapshot.ts` | 121 | ✓ |
| `workspaces.svelte.ts` | 114 | ✓ |

## `src/lib/design`

| File | Lines | Status |
|---|---|---|
| `appearance.svelte.ts` | 58 | ✓ |
| `palettes.test.ts` | 24 | ✓ test |
| `palettes.ts` | 176 | ✓ |
| `theme.test.ts` | 99 | ✓ test |
| `theme.ts` | 96 | ✓ |
| `themes.data.ts` | 503 | **OK** data only (17 theme definitions). |
| `themes.test.ts` | 146 | ✓ test |
| `themes.ts` | 288 | ✓ |

## `src/lib/download.ts`

| File | Lines | Status |
|---|---|---|
| `` | 9 | ✓ |

## `src/lib/find-in-page.ts`

| File | Lines | Status |
|---|---|---|
| `` | 66 | ✓ |

## `src/lib/fuzzy.test.ts`

| File | Lines | Status |
|---|---|---|
| `` | 16 | ✓ test |

## `src/lib/fuzzy.ts`

| File | Lines | Status |
|---|---|---|
| `` | 33 | ✓ |

## `src/lib/i18n`

| File | Lines | Status |
|---|---|---|
| `de.ts` | 56 | ✓ |
| `en.ts` | 59 | ✓ |
| `i18n.svelte.ts` | 72 | ✓ |
| `i18n.test.ts` | 31 | ✓ test |

## `src/lib/icons`

| File | Lines | Status |
|---|---|---|
| `HoprMark.svelte` | 111 | ✓ |

## `src/lib/ipc`

| File | Lines | Status |
|---|---|---|
| `index.ts` | 6 | ✓ |

## `src/lib/motion.ts`

| File | Lines | Status |
|---|---|---|
| `` | 243 | ✓ |

## `src/lib/platform.test.ts`

| File | Lines | Status |
|---|---|---|
| `` | 13 | ✓ test |

## `src/lib/platform.ts`

| File | Lines | Status |
|---|---|---|
| `` | 27 | ✓ |

## `src/lib/shortcuts.test.ts`

| File | Lines | Status |
|---|---|---|
| `` | 75 | ✓ test |

## `src/lib/shortcuts.ts`

| File | Lines | Status |
|---|---|---|
| `` | 161 | ✓ |

## `src/lib/storage.test.ts`

| File | Lines | Status |
|---|---|---|
| `` | 78 | ✓ test |

## `src/lib/storage.ts`

| File | Lines | Status |
|---|---|---|
| `` | 71 | ✓ |

## `src/lib/ui`

| File | Lines | Status |
|---|---|---|
| `Count.svelte` | 21 | ✓ |
| `Drawer.svelte` | 152 | ✓ |
| `FieldError.svelte` | 28 | ✓ |
| `IconButton.svelte` | 137 | ✓ |
| `Kbd.svelte` | 10 | ✓ |
| `Menu.svelte` | 152 | ✓ |
| `Modal.svelte` | 101 | ✓ |
| `Segmented.svelte` | 83 | ✓ |
| `SelectMenu.svelte` | 285 | ✓ |
| `Toaster.svelte` | 133 | ✓ |
| `form.ts` | 33 | ✓ |
| `toast.svelte.ts` | 77 | ✓ |

## `src/lib/window.ts`

| File | Lines | Status |
|---|---|---|
| `` | 12 | ✓ |

## `src/main.ts`

| File | Lines | Status |
|---|---|---|
| `` | 45 | ✓ |
