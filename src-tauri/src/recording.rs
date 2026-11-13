//! Session recordings (§9.18): asciicast v2 files in `{data}/recordings/`. Output only,
//! never input, so typed passwords are never recorded.

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hopr_core::cast::{CastHeader, CastWriter, summary};
use hopr_core::model::Recording;
use hopr_core::{AppError, ErrorCode};
use tauri::{AppHandle, State};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::sessions::{Live, Recorder, SessionManager};
use crate::state::AppState;

/// Largest recording the replay will load.
const MAX_READ: u64 = 64 * 1024 * 1024;
/// A recording stops when its output reaches this (the replay can still open it).
pub const MAX_FILE: u64 = MAX_READ - 1024 * 1024;
/// All recordings together; past this the oldest are deleted when a new one starts.
const MAX_TOTAL: u64 = 1024 * 1024 * 1024;

fn io_err(what: &str, e: &std::io::Error) -> AppError {
    AppError::new(ErrorCode::Storage, what.to_owned()).with_detail(e.to_string())
}

pub fn dir(state: &AppState) -> PathBuf {
    state.data_dir().join("recordings")
}

/// A recording's path, refusing anything that isn't a plain `.cast` file name.
fn path_of(state: &AppState, name: &str) -> Result<PathBuf, AppError> {
    let ok = name.ends_with(".cast")
        && name.len() <= 200
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':'])
        && !name.contains("..");
    if !ok {
        return Err(AppError::invalid("Not a recording"));
    }
    Ok(dir(state).join(name))
}

fn slug(label: &str) -> String {
    let s: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(40)
        .collect();
    let s = s.trim_matches('-');
    if s.is_empty() {
        "session".into()
    } else {
        s.into()
    }
}

/// Start recording a session (no-op if it already is). Returns the file name.
pub fn start(state: &AppState, live: &Live) -> Result<String, AppError> {
    let mut rec = live
        .rec
        .lock()
        .map_err(|_| AppError::new(ErrorCode::Internal, "Recorder poisoned"))?;
    if let Some(r) = rec.as_ref() {
        return Ok(r.name.clone());
    }
    let dir = dir(state);
    std::fs::create_dir_all(&dir)
        .map_err(|e| io_err("Couldn't create the recordings folder", &e))?;
    // Room for this one under the total, oldest recordings going first.
    prune(&dir, MAX_TOTAL.saturating_sub(MAX_FILE));
    let now = OffsetDateTime::now_utc();
    let stamp = format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    );
    let base = format!("{stamp}-{}", slug(&live.who.label));
    let mut name = format!("{base}.cast");
    let mut n = 1;
    while dir.join(&name).exists() {
        n += 1;
        name = format!("{base}-{n}.cast");
    }
    let (cols, rows) = live.size.lock().map(|s| *s).unwrap_or((80, 24));
    let file =
        File::create(dir.join(&name)).map_err(|e| io_err("Couldn't create the recording", &e))?;
    let writer = CastWriter::new(
        BufWriter::with_capacity(64 * 1024, file),
        &CastHeader {
            version: 2,
            width: cols,
            height: rows,
            timestamp: Some(now.unix_timestamp()),
            title: Some(live.who.label.clone()),
        },
    )
    .map_err(|e| io_err("Couldn't write the recording", &e))?;
    *rec = Some(Recorder {
        name: name.clone(),
        writer,
        written: 0,
    });
    Ok(name)
}

#[tauri::command]
#[specta::specta]
pub fn recording_start(
    state: State<'_, Arc<AppState>>,
    manager: State<'_, SessionManager>,
    id: String,
) -> Result<String, AppError> {
    let live = manager.live(&id)?;
    start(&state, &live)
}

/// Stop recording. Returns the file name, or nothing if it wasn't recording.
#[tauri::command]
#[specta::specta]
pub fn recording_stop(
    manager: State<'_, SessionManager>,
    id: String,
) -> Result<Option<String>, AppError> {
    Ok(manager.live(&id)?.stop_recording())
}

/// The file a session is recording to, if any.
#[tauri::command]
#[specta::specta]
pub fn recording_status(manager: State<'_, SessionManager>, id: String) -> Option<String> {
    let live = manager.live(&id).ok()?;
    let rec = live.rec.lock().ok()?;
    rec.as_ref().map(|r| r.name.clone())
}

/// First line and last non-empty line of a file, reading only its ends.
fn ends(path: &Path) -> std::io::Result<(String, String)> {
    let mut f = File::open(path)?;
    let mut first = String::new();
    BufReader::new(&mut f).read_line(&mut first)?;
    let len = f.seek(SeekFrom::End(0))?;
    let from = len.saturating_sub(16 * 1024);
    f.seek(SeekFrom::Start(from))?;
    let mut tail = Vec::new();
    f.read_to_end(&mut tail)?;
    let tail = String::from_utf8_lossy(&tail);
    let last = tail
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .to_owned();
    Ok((first, last))
}

/// Delete the oldest recordings until together they take at most `keep` bytes. One still
/// being written can't be deleted (it is open) and is skipped.
fn prune(dir: &Path, keep: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".cast"))
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((m.modified().ok()?, m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.0);
    for (_, len, path) in files {
        if total <= keep {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
        }
    }
}

/// Recordings, newest first.
#[tauri::command]
#[specta::specta]
pub fn recordings_list(state: State<'_, Arc<AppState>>) -> Result<Vec<Recording>, AppError> {
    let dir = dir(&state);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new()); // nothing recorded yet
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".cast") {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        let Ok((first, last)) = ends(&path) else {
            continue;
        };
        let Some((header, duration)) = summary(&first, &last) else {
            continue;
        };
        let started = header
            .timestamp
            .and_then(|t| OffsetDateTime::from_unix_timestamp(t).ok())
            .or_else(|| meta.modified().ok().map(OffsetDateTime::from))
            .and_then(|t| t.format(&Rfc3339).ok())
            .unwrap_or_default();
        #[allow(clippy::cast_precision_loss)] // file sizes stay far below 2^53
        out.push(Recording {
            title: header.title.unwrap_or_else(|| name.clone()),
            name,
            started_at: started,
            size: meta.len() as f64,
            duration,
        });
    }
    out.sort_by(|a, b| b.started_at.cmp(&a.started_at).then(b.name.cmp(&a.name)));
    Ok(out)
}

/// A recording's contents (asciicast text) for replay.
#[tauri::command]
#[specta::specta]
pub async fn recording_read(
    state: State<'_, Arc<AppState>>,
    name: String,
) -> Result<String, AppError> {
    let path = path_of(&state, &name)?;
    let meta =
        std::fs::metadata(&path).map_err(|e| io_err("That recording no longer exists", &e))?;
    if meta.len() > MAX_READ {
        return Err(AppError::invalid(
            "This recording is too large to replay here (over 64 MB)",
        ));
    }
    tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| io_err("Couldn't read the recording", &e))
}

#[tauri::command]
#[specta::specta]
pub fn recording_delete(state: State<'_, Arc<AppState>>, name: String) -> Result<(), AppError> {
    let path = path_of(&state, &name)?;
    std::fs::remove_file(&path).map_err(|e| io_err("Couldn't delete the recording", &e))
}

/// Save a copy of a recording where the user picks: the `.cast` file, or `text` (the
/// replay's plain-text log) as `.txt`. Returns the saved path, or nothing if cancelled.
#[tauri::command]
#[specta::specta]
pub async fn recording_export(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    name: String,
    text: Option<String>,
) -> Result<Option<String>, AppError> {
    use tauri_plugin_dialog::DialogExt;
    let src = path_of(&state, &name)?;
    let stem = name.trim_end_matches(".cast").to_owned();
    let (file_name, filter, ext) = if text.is_some() {
        (format!("{stem}.txt"), "Text log", "txt")
    } else {
        (name.clone(), "asciicast", "cast")
    };
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_file_name(file_name)
            .add_filter(filter, &[ext])
            .blocking_save_file()
    })
    .await
    .map_err(|e| AppError::internal("Save dialog failed", e))?;
    let Some(dest) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let res = match text {
        Some(t) => std::fs::write(&dest, t),
        None => std::fs::copy(&src, &dest).map(|_| ()),
    };
    res.map_err(|e| io_err("Couldn't save the file", &e))?;
    Ok(Some(dest.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::{prune, slug};

    #[test]
    fn prune_deletes_the_oldest_until_under_the_limit() {
        let dir = std::env::temp_dir().join(format!("hopr-rec-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        for (i, name) in ["a.cast", "b.cast", "c.cast"].iter().enumerate() {
            std::fs::write(dir.join(name), vec![b'x'; 100]).unwrap();
            let t = std::time::SystemTime::UNIX_EPOCH
                + std::time::Duration::from_secs(1_000 + i as u64);
            std::fs::File::options()
                .write(true)
                .open(dir.join(name))
                .unwrap()
                .set_modified(t)
                .unwrap();
        }
        std::fs::write(dir.join("notes.txt"), vec![b'x'; 500]).unwrap();
        prune(&dir, 200);
        assert!(!dir.join("a.cast").exists(), "oldest deleted");
        assert!(dir.join("b.cast").exists() && dir.join("c.cast").exists());
        assert!(
            dir.join("notes.txt").exists(),
            "only recordings are touched"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slugs_are_file_safe() {
        assert_eq!(slug("prod-db"), "prod-db");
        assert_eq!(slug("root@10.0.0.1:22"), "root-10-0-0-1-22");
        assert_eq!(slug("../../x"), "x");
        assert_eq!(slug("…"), "session");
    }
}
