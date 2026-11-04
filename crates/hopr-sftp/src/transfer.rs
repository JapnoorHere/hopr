//! Downloads and uploads: chunked, resumable from a `.part` file, cancellable between chunks,
//! and paced by the speed limit.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

impl Sftp {
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
