# Hopr known issues

Things that were broken, half-wired, or didn't behave the way the UI or config suggested. Each item says what was wrong, where, why it mattered, what was done and how urgent it was.

**Priority:** P1 = users lose data, leak resources or hit it often · P2 = visible wrong behaviour or misleading config · P3 = cleanup, hygiene, dev experience.

**Status:** ✅ fixed · ✏️ the original entry was wrong, corrected below · ❓ needs a decision from the owner · ⏳ open.

---

## Summary

| # | Issue | Area | Priority | Status |
|---|---|---|---|---|
| 1 | SFTP sessions were never closed | SFTP | P1 | ✅ |
| 2 | Remote-edit temp files and watchers were never cleaned up | SFTP | P1 | ✅ |
| 3 | Device-wide settings were saved per profile | Settings / Profiles | P2 | ✅ |
| 4 | Groups and keys couldn't be renamed although the backend supported it | Hosts / Keys | P2 | ✅ |
| 5 | `transparent: true` and the window-effects permission were left over from Mica | Config | P2 | ✅ |
| 6 | The profile picker showed at launch even with one profile | Profiles | P2 | ✅ |
| 7 | Onboarding "remember me" unlocked the vault twice | Onboarding | P2 | ✅ |
| 8 | The website's download button points to a release that doesn't exist | Site | P2 | ❓ |
| 9 | No LICENSE although the site invites contributions | Repo | P2 | ✅ |
| 10 | Switching profiles in dev mode killed `tauri dev` | Dev | P3 | ✅ |
| 11 | `windowVisible` command looked unused | Backend | P3 | ✏️ |
| 12 | `TerminalBackground.opacity` was a dead parameter | Terminal | P3 | ✅ |
| 13 | Most UI text is hardcoded English; i18n has 45 keys | i18n | P3 | ❓ |
| 14 | Taskbar icon didn't refresh in dev after icon changes | Dev | P3 | ✅ |
| 15 | One unexplained app restart seen in testing | App | P3 | ✅ (now traceable) |
| 16 | No `.gitattributes`: line endings depended on each machine | Repo | P3 | ✅ |
| 17 | "No end-to-end tests" | Testing | P3 | ✏️ |
| 18 | Duplicated helpers and very large files | Code health | P3 | ✅ |
| 19 | The app crate's tests never ran in `just test` or CI | Testing | P2 | ✅ |
| 20 | Two splits from the refactor broke behaviour, caught by the end-to-end suite | Files | P1 | ✅ |

---

## P1: resource leaks

### 1. SFTP sessions were never closed ✅
- **What:** the backend had `sftp_close`, but nothing called it. Each visit to the Files panel left an SSH connection open until the app quit; servers that cap sessions (`MaxSessions`) could then refuse new ones.
- **Fixed:** closing the last files view (the drawer and the dual pane) releases the host's SFTP connection, and switching to another host releases the previous one. If transfers are still queued or running for that host, the backend waits and closes the connection when the last one ends; browsing again reconnects on demand. (`files.svelte.ts` `release()`, `sftp/browse.rs`, `sftp/transfers.rs` `host_busy` and `release_if_idle`.)

### 2. Remote-edit temp files and watchers were never cleaned up ✅
- **What:** "Edit locally" downloads into a temp folder and watches it, and `sftp_edit_stop` had no caller. Copies of remote files (possibly holding secrets) stayed on disk.
- **Fixed:** the Files drawer lists files being edited, each with a Stop button (`EditsBar.svelte`, which calls `sftpEditStop` and deletes the copy). The whole `hopr-edit` temp folder is deleted at startup (leftovers from a crash) and on exit (`sftp::edit::clear_temp`). `sftp_edit_path` stays: the end-to-end suite uses it.

### 20. Two refactor regressions caught by the end-to-end suite ✅
- **What:** running `scripts/e2e-smoke.mjs` after the coding-standards refactor found:
  - **Permissions never applied:** the new `PermissionsCard` closed itself before reading its `entry` prop, which then followed the parent's state (now empty), so `chmod` never ran.
  - **The delete confirmation lost its accessible name:** it came back with `ConfirmBar`'s new `label` prop.
- **Fixed:** the card reads everything before closing, and `ConfirmBar` takes a `label`. The same "read props after closing" pattern was checked in every component that was split; no others had it.

---

## P2: wrong or misleading behaviour

### 3. Device-wide settings were saved per profile ✅
- **What:** close-to-tray, the global hotkey and transfer limits lived in each profile's database, so switching profile could change whether the window hid to the tray, or which hotkey worked.
- **Fixed:** they live in `device.json` next to `profiles.json` (`src-tauri/src/device.rs`). On first start, each value is carried over once from the open profile, so nothing is lost. Settings shows "For this computer: the same in every profile" on both cards.

### 4. Groups and keys couldn't be renamed ✅
- **Fixed:** group headers in Hosts and key names in Keys rename in place: the pencil or a double-click, then Enter to save and Esc to cancel (`InlineRename.svelte`, `library.renameGroup` / `renameKey`).
- **Still open:** `group_delete` has no UI either. Deleting a group isn't something the UI offers today, so it's left as is.

### 5. Leftover window transparency config ✅
- **Fixed:** `transparent` is gone from `tauri.conf.json` (the existing `backgroundColor` covers startup), `core:window:allow-set-effects` is gone from the capability, and the `clearEffects()` call is removed. Window effects aren't kept across restarts, so there was nothing left to clear.

### 6. Profile picker showed with only one profile ✅
- **Fixed:** with a single profile there's nothing to choose, so Hopr opens straight in. "Show profile picker" in the title bar menu still works. Covered by a test in `profiles.rs`.

### 7. Onboarding "remember me" unlocked twice ✅
- **Fixed:** `vault_create` takes `remember` and stores the key in the keychain as the vault is created, so the password goes through Argon2 once.

### 8. Website download link has no release behind it ❓
- **What:** the site's download button points to `github.com/JapnoorHere/hopr/releases/latest`, and no release is published yet.
- **Decision needed:** `.github/workflows/release.yml` already builds and publishes on a `v*` tag, so the fix is a first tag. That needs the signing key secrets in the repo and a version number, which is your call. Until then, the button could point at the Open source section ("Build from source").

### 9. No license ✅
- **Fixed:** MIT (`LICENSE`), declared in the workspace `Cargo.toml` and `package.json`. A short `CONTRIBUTING.md` linking to `CODING-STANDARDS.md` is still worth adding.

### 19. The app crate's tests never ran ✅
- **What:** `src-tauri/Cargo.toml` sets `test = false` (its test binary can't load the webview DLLs), so `cargo test --workspace` skipped its 15 tests (profiles, device settings, keychain naming), in `just test` and in CI.
- **Fixed:** `just test` and the CI "Rust tests" step also run `cargo test -p hopr --lib`, which does work.

---

## P3: cleanup and developer experience

### 10. Profile switch killed `tauri dev` ✅
- **What:** switching profiles restarts Hopr. In dev, that ended the process `tauri dev` watches, which stopped the dev server.
- **Fixed:** `pnpm dev:app` (and `just dev`) runs `tauri dev` in a loop (`scripts/dev-loop.mjs`). In a debug build under that loop, a restart leaves a marker and exits instead of relaunching, and the loop starts Hopr again. A release build restarts as before.
- **Possible later:** reopening a profile in-process would also make switching faster in release, but it means making the store and vault swappable everywhere `AppState` is used.

### 11. `windowVisible` looked unused ✏️
- **Correction:** the UI doesn't call it, but the end-to-end suite does (tray and quick-terminal checks). It was removed once and then restored with a comment saying so. Before deleting any command, also search `scripts/`.

### 12. Dead `opacity` parameter ✅
- **Fixed:** removed; `terminalBackground` and `terminalColors` take only the brightness.

### 13. i18n coverage ❓
- **Decision needed:** whether translation is a goal. If it is, move strings into the table feature by feature and add a lint for raw text in markup. If not, remove the partial i18n layer to keep things simple.

### 14. Taskbar icon didn't refresh in dev ✅
- **Fixed:** `build.rs` re-runs when anything in `icons/` changes, so new icons reach the Windows resources on the next dev build.

### 15. One unexplained restart ✅ (now traceable)
- **Fixed:** every restart goes through `platform::restart(app, reason)`, which appends a line with the time and reason to `restarts.log` in the app's log folder. That covers profile switches, installed updates, and restores (the UI now calls `appRestart("restore a backup")` instead of the plugin's `relaunch`). If it happens again, the log shows whether Hopr asked for it.

### 16. No `.gitattributes` ✅
- **Fixed:** `* text=auto eol=lf`, with binary rules for images, icons and fonts. Every text file in the index was already LF, so nothing needed renormalising.

### 17. "No end-to-end tests" ✏️
- **Correction:** there is a thorough suite, `scripts/e2e-smoke.mjs`, with about 50 checks. It covers SSH, jump hosts, proxies, telnet, SFTP, transfers, edit locally, splits, the palette, menus, backups, the tray and the quick terminal. It drives a dev build over CDP against `hopr-devsshd`.
- **Real gaps:**
  - It isn't in CI.
  - The runner needs `HOPR_DEVSSHD_HOME` set too, not only the server.
  - In a debug build the throughput check (≥ 50 MB/s) fails by design.
  - The update check needs a signing key and `HOPR_E2E_UPDATE_PORT`.
  - Onboarding, the profile picker and Settings search aren't covered (a CDP script for them was used during the refactor and could be added).

### 18. Duplicated helpers and large files ✅
- **Fixed:** see the refactor log in [CODING-STANDARDS.md](./CODING-STANDARDS.md#9-refactor-log); every file's status is in [CODE-AUDIT.md](./CODE-AUDIT.md).

---

## What's left

1. **Your decisions:** the first release tag or a temporary download link (8), and whether to translate the UI (13).
2. **Open, no decision needed:** a Delete action for groups (in 4), in-process profile reopen (in 10), and running the end-to-end suite in CI with Onboarding and Settings added (17).
