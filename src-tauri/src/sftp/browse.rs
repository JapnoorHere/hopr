//! Browsing a host: list, resolve, create, rename, delete, permissions and preview.

use data_encoding::BASE64;
use hopr_core::AppError;
use hopr_core::model::{RemoteEntry, RemoteKind};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

use super::*;

/// Start browsing a host: its home folder.
#[tauri::command]
#[specta::specta]
pub async fn sftp_open(app: AppHandle, host_id: String) -> Result<String, AppError> {
    sftp(&app, &host_id).await?.home().await
}

/// A folder's entries.
#[tauri::command]
#[specta::specta]
pub async fn sftp_list(
    app: AppHandle,
    host_id: String,
    path: String,
) -> Result<Vec<RemoteEntry>, AppError> {
    check_path(&path)?;
    sftp(&app, &host_id).await?.list(&path).await
}

/// Resolve a typed path (`~`, `..`) to an absolute one.
#[tauri::command]
#[specta::specta]
pub async fn sftp_resolve(
    app: AppHandle,
    host_id: String,
    path: String,
) -> Result<String, AppError> {
    check_path(&path)?;
    let s = sftp(&app, &host_id).await?;
    let p = if path == "~" || path.starts_with("~/") {
        format!("{}{}", s.home().await?, &path[1..])
    } else {
        path
    };
    s.canonicalize(&p).await
}

/// New folder in `dir`.
#[tauri::command]
#[specta::specta]
pub async fn sftp_mkdir(
    app: AppHandle,
    host_id: String,
    dir: String,
    name: String,
) -> Result<(), AppError> {
    check_name(&name)?;
    sftp(&app, &host_id)
        .await?
        .mkdir(&hopr_sftp::join(&dir, name.trim()))
        .await
}

/// New empty file in `dir`.
#[tauri::command]
#[specta::specta]
pub async fn sftp_touch(
    app: AppHandle,
    host_id: String,
    dir: String,
    name: String,
) -> Result<(), AppError> {
    check_name(&name)?;
    sftp(&app, &host_id)
        .await?
        .touch(&hopr_sftp::join(&dir, name.trim()))
        .await
}

/// Rename an entry within its folder.
#[tauri::command]
#[specta::specta]
pub async fn sftp_rename(
    app: AppHandle,
    host_id: String,
    path: String,
    name: String,
) -> Result<(), AppError> {
    check_path(&path)?;
    check_name(&name)?;
    let to = hopr_sftp::join(&hopr_sftp::parent(&path), name.trim());
    sftp(&app, &host_id).await?.rename(&path, &to).await
}

/// Delete entries (folders with their contents).
#[tauri::command]
#[specta::specta]
pub async fn sftp_remove(
    app: AppHandle,
    host_id: String,
    paths: Vec<String>,
) -> Result<(), AppError> {
    let s = sftp(&app, &host_id).await?;
    for p in &paths {
        check_path(p)?;
        if p == "/" {
            return Err(AppError::invalid("Refusing to delete /"));
        }
        s.remove(p).await?;
    }
    Ok(())
}

/// Change permissions.
#[tauri::command]
#[specta::specta]
pub async fn sftp_chmod(
    app: AppHandle,
    host_id: String,
    path: String,
    mode: u32,
    recursive: bool,
) -> Result<(), AppError> {
    check_path(&path)?;
    if mode > 0o7777 {
        return Err(AppError::invalid("Invalid permissions"));
    }
    sftp(&app, &host_id)
        .await?
        .chmod(&path, mode, recursive)
        .await
}

/// What the preview pane can show.
#[derive(Serialize, Deserialize, Clone, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Preview {
    /// UTF-8 text (possibly cut off).
    Text {
        /// Content.
        text: String,
        /// Only the start of the file is shown.
        truncated: bool,
    },
    /// An image, as a data URL.
    #[serde(rename_all = "camelCase")]
    Image {
        /// `data:image/...;base64,...`.
        data_url: String,
    },
    /// Not previewable.
    Binary,
}

pub(super) fn image_type(name: &str) -> Option<&'static str> {
    let ext = name.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => return None,
    })
}

/// Preview a file: text, image, or nothing (S10).
#[tauri::command]
#[specta::specta]
pub async fn sftp_preview(
    app: AppHandle,
    host_id: String,
    path: String,
) -> Result<Preview, AppError> {
    check_path(&path)?;
    let s = sftp(&app, &host_id).await?;
    let entry = s.stat(&path).await?;
    if entry.kind != RemoteKind::File {
        return Ok(Preview::Binary);
    }
    if let Some(mime) = image_type(&entry.name) {
        #[allow(clippy::cast_precision_loss)]
        if entry.size > IMAGE_MAX as f64 {
            return Ok(Preview::Binary);
        }
        let bytes = s.read_head(&path, IMAGE_MAX).await?;
        return Ok(Preview::Image {
            data_url: format!("data:{mime};base64,{}", BASE64.encode(&bytes)),
        });
    }
    let bytes = s.read_head(&path, PREVIEW_MAX).await?;
    if bytes.contains(&0) {
        return Ok(Preview::Binary);
    }
    #[allow(clippy::cast_precision_loss)]
    let truncated = entry.size > bytes.len() as f64;
    Ok(Preview::Text {
        text: String::from_utf8_lossy(&bytes).into_owned(),
        truncated,
    })
}

/// Drop the SFTP session for a host.
#[tauri::command]
#[specta::specta]
pub async fn sftp_close(app: AppHandle, host_id: String) -> Result<(), AppError> {
    let s = app.state::<Files>().sessions.lock().await.remove(&host_id);
    if let Some(s) = s {
        s.close().await;
    }
    Ok(())
}
