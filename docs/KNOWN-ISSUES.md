# Hopr known issues

Things that are broken, half-wired, or don't behave the way the UI or config suggests. Each item says what's wrong, where, why it matters, how to fix it and how urgent it is. This is a plan: nothing here is fixed yet.

**Priority:** P1 = users lose data, leak resources or hit it often · P2 = visible wrong behaviour or misleading config · P3 = cleanup, hygiene, dev experience.

---

## Summary

| # | Issue | Area | Priority |
|---|---|---|---|
| 1 | SFTP sessions are never closed | SFTP | P1 |
| 2 | Remote-edit temp files and watchers are never cleaned up | SFTP | P1 |
| 3 | Device-wide settings are saved per profile | Settings / Profiles | P2 |
| 4 | Groups and keys can't be renamed although the backend supports it | Hosts / Keys | P2 |
| 5 | `transparent: true` and the window-effects permission are left over from Mica | Config | P2 |
| 6 | The profile picker shows at launch even with one profile | Profiles | P2 |
| 7 | Onboarding "remember me" unlocks the vault twice | Onboarding | P2 |
| 8 | The website's download button points to a release that doesn't exist | Site | P2 |
| 9 | No LICENSE although the site invites contributions | Repo | P2 |
| 10 | Switching profiles in dev mode kills `tauri dev` | Dev | P3 |
| 11 | `windowVisible` command has no caller | Backend | P3 |
| 12 | `TerminalBackground.opacity` is a dead parameter | Terminal | P3 |
| 13 | Most UI text is hardcoded English; i18n has 45 keys | i18n | P3 |
| 14 | Taskbar icon doesn't refresh in dev after icon changes | Dev | P3 |
| 15 | One unexplained app restart seen in testing | App | P3 |
| 16 | No `.gitattributes`: line endings depend on each machine | Repo | P3 |
| 17 | No end-to-end tests | Testing | P3 |
| 18 | Duplicated helpers and very large files | Code health | P3 |

---

## P1: resource leaks

### 1. SFTP sessions are never closed
- **What:** the backend has `sftp_close` (`sftpClose` in `bindings.ts`), but nothing in `src/` calls it. Closing the Files panel or a tab leaves the SFTP session open in `Files` state.
- **Where:** `src-tauri/src/sftp.rs` (command), `src/features/sftp/` (no caller).
- **Impact:** each visit to the Files panel can leave an SSH channel and memory behind until the app quits. Long sessions pile up channels; some servers cap them (`MaxSessions`), so later connections can fail.
- **Fix:** call `sftpClose(id)` when the Files panel unmounts, its tab closes, or its SSH session ends. On the Rust side, also close SFTP sessions tied to a terminal session when that session closes. Add a test that counts open sessions after open/close.

### 2. Remote-edit temp files and watchers are never cleaned up
- **What:** "Edit remotely" downloads a file to a temp folder and watches it. `sftp_edit_stop` and `sftp_edit_path` exist but have no caller.
- **Where:** `src-tauri/src/sftp.rs`; `src/features/sftp/`.
- **Impact:** temp copies of remote files (which may hold secrets) stay on disk, and file watchers keep running for the rest of the app's life.
- **Fix:** show open edits in the Files panel with a "Stop editing" action that calls `sftpEditStop`; stop all edits for a session when it closes; delete the temp folder on app start and exit. Use `sftpEditPath` to show where the local copy is.

---

## P2: wrong or misleading behaviour

### 3. Device-wide settings are saved per profile
- **What:** close-to-tray, the global hotkey and transfer limits are stored in each profile's database, but they describe the machine, not the person.
- **Impact:** switching profile can silently change whether the window hides to the tray or which hotkey works; the global hotkey can be registered twice or not at all.
- **Fix:** split prefs into **device** (stored next to `profiles.json` in the data root) and **profile** (stored in the profile DB). Move the three settings, migrate existing values from the default profile once, and label them "This device" in Settings.

### 4. Groups and keys can't be renamed
- **What:** `group_rename` and `key_rename` commands exist with no UI.
- **Impact:** the only way to fix a typo in a group or key name is to delete and recreate it, losing links to hosts.
- **Fix:** add Rename to the group header menu in Hosts and to the key row in Keys, using the existing commands. Inline edit with Enter to save, Esc to cancel.

### 5. Leftover window transparency config
- **What:** `src-tauri/tauri.conf.json` still sets `"transparent": true`, and the capability file still grants `core:window:allow-set-effects`. Both were for the removed Mica/Acrylic backdrops.
- **Impact:** a transparent window costs extra compositing, can flash or show through during resize or startup, and grants a permission the app no longer needs (least privilege).
- **Fix:** set `transparent` to `false`, remove the capability, and check `appearance.svelte.ts` no longer needs `clearEffects` (keep it one release if older installs may have effects applied).

### 6. Profile picker shows with only one profile
- **What:** the Chrome-style picker appears at every launch, even when there is just the default profile.
- **Impact:** an extra click on every launch for most users.
- **Fix:** skip the picker when there is a single profile (unless "Ask at launch" is explicitly on); keep the title bar switcher for adding profiles.

### 7. Onboarding "remember me" unlocks twice
- **What:** in the secure step, choosing Windows Hello or remember re-derives the key with Argon2 a second time after the vault was just created.
- **Impact:** a noticeable pause (about a second) at the end of setup.
- **Fix:** have vault creation return or keep the data key in memory and store it in the keychain directly, without a second unlock.

### 8. Website download link has no release behind it
- **What:** the site's download button points to `github.com/JapnoorHere/hopr/releases/latest`, and no release is published yet.
- **Impact:** visitors land on an empty Releases page or a 404.
- **Fix:** publish a signed release with the Tauri bundler (a GitHub Actions workflow on tag push), or until then change the button to "Build from source" pointing to the Open source section.

### 9. No license
- **What:** `package.json` says `UNLICENSED` and there is no LICENSE file, but the website invites issues and pull requests.
- **Impact:** without a license nobody can legally reuse or contribute code, and contributors don't know the terms.
- **Fix:** choose a license (MIT or Apache-2.0 are typical for desktop tools; GPL-3.0 if derivatives must stay open), add `LICENSE`, update `package.json` and `Cargo.toml`, and add a short `CONTRIBUTING.md` that links to `CODING-STANDARDS.md`.

---

## P3: cleanup and developer experience

### 10. Profile switch kills `tauri dev`
- **What:** switching profiles calls `app.restart()`. In dev that ends the process that `tauri dev` is watching, so the dev session stops.
- **Fix:** in debug builds, reopen the profile in-process instead of restarting: close the vault and DB, open the new profile's storage, reload the webview. Longer term, do that in release too, which also makes switching faster.

### 11. `windowVisible` has no caller
- **What:** the command exists but nothing uses it.
- **Fix:** either use it (for example to skip notifications while the window is visible) or delete it and regenerate bindings.

### 12. Dead `opacity` parameter
- **What:** `TerminalBackground.opacity` in `themes.ts` is always 100 since see-through terminals were removed.
- **Fix:** remove the field and its arguments; keep the brightness logic.

### 13. i18n coverage
- **What:** the i18n table has about 45 keys; most strings in components are hardcoded English.
- **Impact:** a translation would show mixed languages.
- **Fix:** decide whether translation is a goal. If yes, move strings feature by feature into the table and add a lint that flags raw text in `.svelte` markup. If not, remove the partial i18n layer to keep things simple.

### 14. Taskbar icon doesn't refresh in dev
- **What:** after changing icons, the running dev app keeps the old taskbar icon because the dev watcher doesn't rebuild resources.
- **Fix:** document it (touch `src-tauri/build.rs` or restart `tauri dev`), or add icons to `build.rs`'s `rerun-if-changed`.

### 15. One unexplained restart
- **What:** during testing the app restarted once with no profile switch.
- **Fix:** log the reason before every `restart()` call (profile switch, update, reset) so the next occurrence can be traced.

### 16. No `.gitattributes`
- **What:** line endings depend on each developer's `core.autocrlf`. A history rewrite already left CRLF files that broke biome.
- **Fix:** add `.gitattributes` with `* text=auto eol=lf` plus binary rules for icons and fonts, then renormalise once.

### 17. No end-to-end tests
- **What:** there are 155 unit tests and Rust tests, but no automated test drives the real app. Onboarding, profile switch, connect and settings search were only checked by hand.
- **Fix:** add a small suite (WebDriver via `tauri-driver`, or CDP against WebView2) covering first run, unlock, profile switch, connect to a local SSH server in CI, and Settings search.

### 18. Duplicated helpers and large files
- **What:** `fn invalid` is copied into 8 Rust files; several files are far over the size guideline (`sftp.rs` 1,311 lines, `sessions.rs` 968, `HostsView.svelte` 880, `themes.ts` 806, `library.rs` 784, `Terminal.svelte` 754, `session.svelte.ts` 753).
- **Fix:** see the refactor backlog in [`CODING-STANDARDS.md`](./CODING-STANDARDS.md#9-refactor-backlog-planned-not-done).

---

## Suggested order

1. Fix the leaks (1, 2): small changes, real impact.
2. Device vs. profile settings (3) and the transparency config (5).
3. Quick UX wins: rename (4), single-profile picker (6), double unlock (7).
4. Release readiness: license (9), a first release and the download link (8), `.gitattributes` (16).
5. Developer experience and code health (10–18).
