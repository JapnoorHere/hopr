//! SFTP over the same route a terminal uses (jump chain, proxy, saved credentials): browse,
//! file operations, and chunked, resumable, cancellable transfers (HOPR_PLAN §9.9, S1–S8).

#![warn(missing_docs)]

#[cfg(feature = "server")]
pub mod server;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use hopr_core::model::{RemoteEntry, RemoteKind};
use hopr_core::{AppError, ErrorCode};
use hopr_ssh::{ConnectParams, QuietConnection};
use russh_sftp::client::SftpSession;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::protocol::{FileAttributes, OpenFlags, StatusCode};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

/// Bytes per read/write call during transfers.
pub const CHUNK: usize = 64 * 1024;

/// Type bits in a mode word (S_IFMT and friends).
const S_IFMT: u32 = 0o170_000;
const S_IFDIR: u32 = 0o040_000;
const S_IFLNK: u32 = 0o120_000;
const S_IFREG: u32 = 0o100_000;

/// Turn an SFTP error into something a person can act on.
fn sftp_err(what: &str, path: &str, e: SftpError) -> AppError {
    match &e {
        SftpError::Status(s) => match s.status_code {
            StatusCode::NoSuchFile => {
                AppError::new(ErrorCode::InvalidInput, format!("{path} doesn't exist"))
            }
            StatusCode::PermissionDenied => AppError::new(
                ErrorCode::InvalidInput,
                format!("Permission denied: {path}"),
            ),
            _ => AppError::new(ErrorCode::Internal, format!("Couldn't {what} {path}"))
                .with_detail(e.to_string()),
        },
        SftpError::Timeout => AppError::new(
            ErrorCode::Timeout,
            format!("The server took too long to {what} {path}"),
        ),
        _ => AppError::new(ErrorCode::Disconnected, format!("Couldn't {what} {path}"))
            .with_detail(e.to_string()),
    }
}

fn io_err(what: &str, path: &Path, e: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("Couldn't {what} {}", path.display()),
    )
    .with_detail(e.to_string())
}

/// `dir/name` with POSIX separators.
pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// The folder containing `path` (`/` for top-level entries).
pub fn parent(path: &str) -> String {
    match path.trim_end_matches('/').rsplit_once('/') {
        Some(("", _)) | None => "/".to_owned(),
        Some((p, _)) => p.to_owned(),
    }
}

/// The last component of `path`.
pub fn file_name(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

/// Map protocol attributes to the UI's entry.
pub fn entry(name: &str, path: String, attrs: &FileAttributes) -> RemoteEntry {
    let raw = attrs.permissions.unwrap_or(0);
    let kind = match raw & S_IFMT {
        S_IFDIR => RemoteKind::Dir,
        S_IFLNK => RemoteKind::Link,
        S_IFREG | 0 => RemoteKind::File,
        _ => RemoteKind::Other,
    };
    #[allow(clippy::cast_precision_loss)]
    let size = attrs.size.unwrap_or(0) as f64;
    RemoteEntry {
        name: name.to_owned(),
        path,
        kind,
        size,
        modified: attrs.mtime,
        mode: raw & 0o7777,
        owner: attrs
            .user
            .clone()
            .or_else(|| attrs.uid.map(|u| u.to_string())),
        group: attrs
            .group
            .clone()
            .or_else(|| attrs.gid.map(|g| g.to_string())),
    }
}

/// An open SFTP session. Share it behind an `Arc`; requests run concurrently.
pub struct Sftp {
    session: SftpSession,
    connection: QuietConnection,
    throttle: Option<std::sync::Arc<Throttle>>,
}

/// Open SFTP to the last hop of `params`.
pub async fn connect(params: ConnectParams) -> Result<Sftp, AppError> {
    let sub = hopr_ssh::subsystem(params, "sftp").await?;
    let session = SftpSession::new(sub.stream).await.map_err(|e| {
        AppError::new(ErrorCode::Internal, "The server's SFTP didn't start")
            .with_detail(e.to_string())
    })?;
    session.set_timeout(30);
    Ok(Sftp {
        session,
        connection: sub.connection,
        throttle: None,
    })
}

/// A speed limit shared by every transfer that holds it (S5). 0 = unlimited.
#[derive(Default)]
pub struct Throttle {
    bytes_per_sec: std::sync::atomic::AtomicU64,
    next: std::sync::Mutex<Option<tokio::time::Instant>>,
}

impl Throttle {
    /// Set the limit in bytes per second (0 turns it off).
    pub fn set(&self, bytes_per_sec: u64) {
        self.bytes_per_sec.store(bytes_per_sec, Ordering::Relaxed);
    }

    /// The current limit.
    pub fn get(&self) -> u64 {
        self.bytes_per_sec.load(Ordering::Relaxed)
    }

    /// Wait until `n` more bytes fit in the budget. Concurrent transfers share it, so the
    /// total stays under the limit.
    pub async fn take(&self, n: usize) {
        let rate = self.get();
        if rate == 0 {
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        let cost = std::time::Duration::from_secs_f64(n as f64 / rate as f64);
        let start = {
            let Ok(mut next) = self.next.lock() else {
                return;
            };
            let now = tokio::time::Instant::now();
            // Idle time doesn't bank up into a burst.
            let start = next.filter(|t| *t > now).unwrap_or(now);
            *next = Some(start + cost);
            start
        };
        tokio::time::sleep_until(start + cost).await;
    }
}

/// Open SFTP whose transfers share `throttle`.
pub async fn connect_throttled(
    params: ConnectParams,
    throttle: std::sync::Arc<Throttle>,
) -> Result<Sftp, AppError> {
    let mut s = connect(params).await?;
    s.throttle = Some(throttle);
    Ok(s)
}

impl Sftp {
    async fn pace(&self, n: usize) {
        if let Some(t) = &self.throttle {
            t.take(n).await;
        }
    }

    /// The SSH connection underneath is still up.
    pub fn is_open(&self) -> bool {
        self.connection.is_open()
    }

    /// Close the session and its connection.
    pub async fn close(&self) {
        let _ = self.session.close().await;
        self.connection.disconnect().await;
    }

    /// The login folder (where SFTP starts).
    pub async fn home(&self) -> Result<String, AppError> {
        self.session
            .canonicalize(".")
            .await
            .map_err(|e| sftp_err("open", "your home folder", e))
    }

    /// Resolve `path` to an absolute path.
    pub async fn canonicalize(&self, path: &str) -> Result<String, AppError> {
        self.session
            .canonicalize(path)
            .await
            .map_err(|e| sftp_err("open", path, e))
    }

    /// A folder's entries: folders first, then by name.
    pub async fn list(&self, dir: &str) -> Result<Vec<RemoteEntry>, AppError> {
        let rd = self
            .session
            .read_dir(dir)
            .await
            .map_err(|e| sftp_err("open", dir, e))?;
        let mut out: Vec<RemoteEntry> = rd
            .filter(|e| e.file_name() != "." && e.file_name() != "..")
            .map(|e| {
                let name = e.file_name();
                entry(&name, join(dir, &name), &e.metadata())
            })
            .collect();
        out.sort_by(|a, b| {
            (b.kind == RemoteKind::Dir)
                .cmp(&(a.kind == RemoteKind::Dir))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(out)
    }

    /// One entry.
    pub async fn stat(&self, path: &str) -> Result<RemoteEntry, AppError> {
        let attrs = self
            .session
            .metadata(path)
            .await
            .map_err(|e| sftp_err("read", path, e))?;
        Ok(entry(file_name(path), path.to_owned(), &attrs))
    }

    /// Whether `path` exists.
    pub async fn exists(&self, path: &str) -> bool {
        self.session.metadata(path).await.is_ok()
    }

    /// New folder.
    pub async fn mkdir(&self, path: &str) -> Result<(), AppError> {
        self.session
            .create_dir(path)
            .await
            .map_err(|e| sftp_err("create", path, e))
    }

    /// New empty file (fails if it exists).
    pub async fn touch(&self, path: &str) -> Result<(), AppError> {
        let f = self
            .session
            .open_with_flags(
                path,
                OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
            )
            .await
            .map_err(|e| sftp_err("create", path, e))?;
        f.close()
            .await
            .map_err(|e| sftp_err("create", path, SftpError::from(e)))
    }

    /// Rename or move.
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), AppError> {
        self.session
            .rename(from, to)
            .await
            .map_err(|e| sftp_err("rename", from, e))
    }

    /// Delete a file, or a folder with everything in it.
    pub async fn remove(&self, path: &str) -> Result<(), AppError> {
        let attrs = self
            .session
            .symlink_metadata(path)
            .await
            .map_err(|e| sftp_err("delete", path, e))?;
        if attrs.permissions.unwrap_or(0) & S_IFMT == S_IFDIR {
            for child in self.list(path).await? {
                Box::pin(self.remove(&child.path)).await?;
            }
            self.session
                .remove_dir(path)
                .await
                .map_err(|e| sftp_err("delete", path, e))
        } else {
            self.session
                .remove_file(path)
                .await
                .map_err(|e| sftp_err("delete", path, e))
        }
    }

    /// Change permission bits (optionally everything inside a folder too).
    pub async fn chmod(&self, path: &str, mode: u32, recursive: bool) -> Result<(), AppError> {
        let attrs = FileAttributes {
            permissions: Some(mode & 0o7777),
            ..FileAttributes::empty()
        };
        self.session
            .set_metadata(path, attrs)
            .await
            .map_err(|e| sftp_err("change permissions of", path, e))?;
        if recursive && self.stat(path).await?.kind == RemoteKind::Dir {
            for child in self.list(path).await? {
                if child.kind != RemoteKind::Link {
                    Box::pin(self.chmod(&child.path, mode, true)).await?;
                }
            }
        }
        Ok(())
    }

    /// Up to `max` bytes from the start of a file (preview).
    pub async fn read_head(&self, path: &str, max: usize) -> Result<Vec<u8>, AppError> {
        let f = self
            .session
            .open(path)
            .await
            .map_err(|e| sftp_err("open", path, e))?;
        let mut buf = Vec::new();
        f.take(max as u64)
            .read_to_end(&mut buf)
            .await
            .map_err(|e| sftp_err("read", path, SftpError::from(e)))?;
        Ok(buf)
    }

    /// Download `remote` to `local`. With `resume`, a `local.part` left by an earlier
    /// attempt is continued. Returns the file size; `cancel` stops it between chunks.
    pub async fn download(
        &self,
        remote: &str,
        local: &Path,
        resume: bool,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64, u64) + Send,
    ) -> Result<u64, AppError> {
        let size = self
            .session
            .metadata(remote)
            .await
            .map_err(|e| sftp_err("read", remote, e))?
            .size
            .unwrap_or(0);
        let part = part_path(local);
        let mut offset = 0;
        if resume
            && let Ok(m) = tokio::fs::metadata(&part).await
            && m.len() <= size
        {
            offset = m.len();
        }
        let mut src = self
            .session
            .open(remote)
            .await
            .map_err(|e| sftp_err("open", remote, e))?;
        if offset > 0 {
            src.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|e| sftp_err("read", remote, SftpError::from(e)))?;
        }
        if let Some(dir) = local.parent() {
            tokio::fs::create_dir_all(dir)
                .await
                .map_err(|e| io_err("create", dir, e))?;
        }
        let mut dst = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(offset > 0)
            .truncate(offset == 0)
            .open(&part)
            .await
            .map_err(|e| io_err("write", &part, e))?;
        let mut done = offset;
        progress(done, size);
        let mut buf = vec![0u8; CHUNK];
        loop {
            if cancel.load(Ordering::Relaxed) {
                dst.flush().await.map_err(|e| io_err("write", &part, e))?;
                return Err(cancelled());
            }
            let n = src
                .read(&mut buf)
                .await
                .map_err(|e| sftp_err("read", remote, SftpError::from(e)))?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n])
                .await
                .map_err(|e| io_err("write", &part, e))?;
            self.pace(n).await;
            done += n as u64;
            progress(done, size);
        }
        dst.flush().await.map_err(|e| io_err("write", &part, e))?;
        drop(dst);
        let _ = tokio::fs::remove_file(local).await;
        tokio::fs::rename(&part, local)
            .await
            .map_err(|e| io_err("save", local, e))?;
        Ok(done)
    }

    /// Upload `local` to `remote`. With `resume`, a shorter file already at `remote` is
    /// continued. Returns the file size; `cancel` stops it between chunks.
    pub async fn upload(
        &self,
        local: &Path,
        remote: &str,
        resume: bool,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64, u64) + Send,
    ) -> Result<u64, AppError> {
        let size = tokio::fs::metadata(local)
            .await
            .map_err(|e| io_err("read", local, e))?
            .len();
        let mut offset = 0;
        if resume
            && let Ok(m) = self.session.metadata(remote).await
            && m.size.unwrap_or(0) <= size
        {
            offset = m.size.unwrap_or(0);
        }
        let mut flags = OpenFlags::WRITE | OpenFlags::CREATE;
        if offset == 0 {
            flags |= OpenFlags::TRUNCATE;
        }
        let mut dst = self
            .session
            .open_with_flags(remote, flags)
            .await
            .map_err(|e| sftp_err("write", remote, e))?;
        let mut src = tokio::fs::File::open(local)
            .await
            .map_err(|e| io_err("read", local, e))?;
        if offset > 0 {
            dst.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|e| sftp_err("write", remote, SftpError::from(e)))?;
            src.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|e| io_err("read", local, e))?;
        }
        let mut done = offset;
        progress(done, size);
        let mut buf = vec![0u8; CHUNK];
        loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = dst.shutdown().await;
                return Err(cancelled());
            }
            let n = src
                .read(&mut buf)
                .await
                .map_err(|e| io_err("read", local, e))?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n])
                .await
                .map_err(|e| sftp_err("write", remote, SftpError::from(e)))?;
            self.pace(n).await;
            done += n as u64;
            progress(done, size);
        }
        dst.shutdown()
            .await
            .map_err(|e| sftp_err("write", remote, SftpError::from(e)))?;
        Ok(done)
    }
}

/// Where a download is written until it completes.
pub fn part_path(local: &Path) -> PathBuf {
    let mut name = local.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    local.with_file_name(name)
}

/// The error a cancelled transfer ends with.
pub fn cancelled() -> AppError {
    AppError::new(ErrorCode::InvalidInput, "Cancelled")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_paths() {
        assert_eq!(join("/home/u", "a.txt"), "/home/u/a.txt");
        assert_eq!(join("/", "etc"), "/etc");
        assert_eq!(parent("/home/u/a.txt"), "/home/u");
        assert_eq!(parent("/etc"), "/");
        assert_eq!(parent("/"), "/");
        assert_eq!(file_name("/home/u/dir/"), "dir");
    }

    #[test]
    fn modes_split_type_and_permissions() {
        let attrs = FileAttributes {
            permissions: Some(S_IFDIR | 0o755),
            size: Some(4096),
            ..FileAttributes::empty()
        };
        let e = entry("d", "/d".into(), &attrs);
        assert_eq!(e.kind, RemoteKind::Dir);
        assert_eq!(e.mode, 0o755);
    }
}
