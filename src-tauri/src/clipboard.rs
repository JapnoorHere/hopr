//! Copying secrets (X9): written from Rust so the secret never passes through the web view,
//! kept out of clipboard history/sync where the OS allows, and cleared after a delay unless
//! the user has copied something else since.

use std::hash::{BuildHasher, RandomState};
use std::time::Duration;

use hopr_core::{AppError, ErrorCode};

/// Settings key: seconds before a copied secret is cleared (0 = never).
pub const CLEAR_KEY: &str = "security.clipboard_clear_secs";
/// Default delay.
pub const CLEAR_DEFAULT: u32 = 30;

fn clip_err(e: arboard::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Couldn't use the clipboard").with_detail(e.to_string())
}

fn set_secret(clip: &mut arboard::Clipboard, text: &str) -> Result<(), arboard::Error> {
    #[cfg(windows)]
    {
        use arboard::SetExtWindows;
        clip.set().exclude_from_history().text(text)
    }
    #[cfg(target_os = "macos")]
    {
        use arboard::SetExtApple;
        clip.set().exclude_from_history().text(text)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use arboard::SetExtLinux;
        clip.set().exclude_from_history().text(text)
    }
}

/// Copy `text`, then clear it after `clear_after_secs` (0 = leave it).
pub async fn copy_secret(text: String, clear_after_secs: u32) -> Result<(), AppError> {
    // Only a keyed hash of the secret is kept for the later comparison.
    let hasher = RandomState::new();
    let digest = hasher.hash_one(&text);
    tokio::task::spawn_blocking(move || {
        let mut clip = arboard::Clipboard::new().map_err(clip_err)?;
        set_secret(&mut clip, &text).map_err(clip_err)
    })
    .await
    .map_err(|e| {
        AppError::new(ErrorCode::Internal, "Background task failed").with_detail(e.to_string())
    })??;
    if clear_after_secs > 0 {
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(u64::from(clear_after_secs))).await;
            let _ = tokio::task::spawn_blocking(move || {
                let Ok(mut clip) = arboard::Clipboard::new() else {
                    return;
                };
                let still_ours = clip
                    .get_text()
                    .is_ok_and(|current| hasher.hash_one(&current) == digest);
                if still_ours {
                    let _ = clip.clear();
                }
            })
            .await;
        });
    }
    Ok(())
}
