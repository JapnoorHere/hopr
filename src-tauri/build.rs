fn main() {
    // New icons go into the Windows resources on the next dev build, not only after a clean.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build();
}
