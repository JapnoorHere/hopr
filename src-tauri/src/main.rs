// No console window on Windows, dev builds included (their logs still reach `pnpm tauri dev`,
// which pipes them). Do not remove.
#![windows_subsystem = "windows"]

fn main() {
    hopr_lib::run();
}
