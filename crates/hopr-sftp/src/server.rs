//! A small SFTP server over a local folder, for tests and hopr-devsshd. Paths are confined
//! to the root (`/` is the root folder; `..` can't climb out). Windows has no Unix
//! permission bits, so `chmod` values are remembered in memory (shared across sessions).

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};

/// Modes set with `chmod`, by real path (only consulted where the OS has no Unix bits).
pub type Modes = Arc<Mutex<HashMap<PathBuf, u32>>>;

enum Open {
    File(fs::File),
    Dir { path: PathBuf, sent: bool },
}

/// One SFTP session serving `root`.
pub struct LocalFs {
    root: PathBuf,
    modes: Modes,
    handles: HashMap<String, Open>,
    next: u64,
}

impl LocalFs {
    /// Serve `root`. Pass the same `modes` to every session to keep chmod results.
    pub fn new(root: impl Into<PathBuf>, modes: Modes) -> Self {
        Self {
            root: root.into(),
            modes,
            handles: HashMap::new(),
            next: 0,
        }
    }

    /// Map an SFTP path to a real path inside the root.
    fn real(&self, path: &str) -> Result<PathBuf, StatusCode> {
        let mut out = self.root.clone();
        let mut depth = 0usize;
        for c in Path::new(path.trim_start_matches('/')).components() {
            match c {
                Component::Normal(p) => {
                    out.push(p);
                    depth += 1;
                }
                Component::ParentDir => {
                    if depth == 0 {
                        return Err(StatusCode::PermissionDenied);
                    }
                    out.pop();
                    depth -= 1;
                }
                _ => {}
            }
        }
        Ok(out)
    }

    /// The SFTP path of a real path.
    fn virt(&self, real: &Path) -> String {
        let rel = real.strip_prefix(&self.root).unwrap_or(real);
        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        format!("/{}", parts.join("/"))
    }

    fn attrs(&self, real: &Path) -> Result<FileAttributes, StatusCode> {
        let m = fs::symlink_metadata(real).map_err(io_status)?;
        let kind = if m.is_dir() {
            0o040_000
        } else if m.file_type().is_symlink() {
            0o120_000
        } else {
            0o100_000
        };
        let perms = self.mode_of(real, &m);
        let mtime = m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .and_then(|d| u32::try_from(d.as_secs()).ok());
        Ok(FileAttributes {
            size: Some(m.len()),
            permissions: Some(kind | perms),
            mtime,
            atime: mtime,
            ..FileAttributes::empty()
        })
    }

    #[cfg(unix)]
    fn mode_of(&self, _real: &Path, m: &fs::Metadata) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        m.permissions().mode() & 0o7777
    }

    #[cfg(not(unix))]
    fn mode_of(&self, real: &Path, m: &fs::Metadata) -> u32 {
        let remembered = self.modes.lock().ok().and_then(|g| g.get(real).copied());
        remembered.unwrap_or(if m.is_dir() { 0o755 } else { 0o644 })
    }

    fn set_mode(&self, real: &Path, mode: u32) -> Result<(), StatusCode> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(real, fs::Permissions::from_mode(mode & 0o7777))
                .map_err(io_status)?;
        }
        if let Ok(mut g) = self.modes.lock() {
            g.insert(real.to_path_buf(), mode & 0o7777);
        }
        Ok(())
    }

    fn handle(&mut self, open: Open) -> String {
        self.next += 1;
        let h = format!("h{}", self.next);
        self.handles.insert(h.clone(), open);
        h
    }
}

fn io_status(e: std::io::Error) -> StatusCode {
    match e.kind() {
        std::io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        std::io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        _ => StatusCode::Failure,
    }
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".into(),
        language_tag: "en-US".into(),
    }
}

impl russh_sftp::server::Handler for LocalFs {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        Ok(Version::new())
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let real = self.real(&filename)?;
        let mut o = fs::OpenOptions::new();
        o.read(pflags.contains(OpenFlags::READ) || !pflags.contains(OpenFlags::WRITE))
            .write(pflags.contains(OpenFlags::WRITE) || pflags.contains(OpenFlags::APPEND))
            .append(pflags.contains(OpenFlags::APPEND))
            .truncate(pflags.contains(OpenFlags::TRUNCATE));
        if pflags.contains(OpenFlags::EXCLUDE) {
            o.create_new(true);
        } else if pflags.contains(OpenFlags::CREATE) {
            o.create(true);
        }
        let f = o.open(&real).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                StatusCode::Failure
            } else {
                io_status(e)
            }
        })?;
        let handle = self.handle(Open::File(f));
        Ok(Handle { id, handle })
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.handles.remove(&handle);
        Ok(ok(id))
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let Some(Open::File(f)) = self.handles.get_mut(&handle) else {
            return Err(StatusCode::Failure);
        };
        f.seek(SeekFrom::Start(offset)).map_err(io_status)?;
        let mut buf = vec![0u8; len.min(256 * 1024) as usize];
        let n = f.read(&mut buf).map_err(io_status)?;
        if n == 0 {
            return Err(StatusCode::Eof);
        }
        buf.truncate(n);
        Ok(Data { id, data: buf })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let Some(Open::File(f)) = self.handles.get_mut(&handle) else {
            return Err(StatusCode::Failure);
        };
        f.seek(SeekFrom::Start(offset)).map_err(io_status)?;
        f.write_all(&data).map_err(io_status)?;
        Ok(ok(id))
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let real = self.real(&path)?;
        Ok(Attrs {
            id,
            attrs: self.attrs(&real)?,
        })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        self.lstat(id, path).await
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
        match self.handles.get(&handle) {
            Some(Open::File(f)) => {
                let m = f.metadata().map_err(io_status)?;
                Ok(Attrs {
                    id,
                    attrs: FileAttributes {
                        size: Some(m.len()),
                        permissions: Some(0o100_644),
                        ..FileAttributes::empty()
                    },
                })
            }
            Some(Open::Dir { path, .. }) => {
                let path = path.clone();
                Ok(Attrs {
                    id,
                    attrs: self.attrs(&path)?,
                })
            }
            None => Err(StatusCode::Failure),
        }
    }

    async fn setstat(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        let real = self.real(&path)?;
        if let Some(size) = attrs.size {
            fs::OpenOptions::new()
                .write(true)
                .open(&real)
                .and_then(|f| f.set_len(size))
                .map_err(io_status)?;
        }
        if let Some(mode) = attrs.permissions {
            self.set_mode(&real, mode)?;
        }
        Ok(ok(id))
    }

    async fn fsetstat(
        &mut self,
        id: u32,
        handle: String,
        attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        if let (Some(size), Some(Open::File(f))) = (attrs.size, self.handles.get(&handle)) {
            f.set_len(size).map_err(io_status)?;
        }
        Ok(ok(id))
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let real = self.real(&path)?;
        if !real.is_dir() {
            return Err(StatusCode::NoSuchFile);
        }
        let handle = self.handle(Open::Dir {
            path: real,
            sent: false,
        });
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        let path = match self.handles.get_mut(&handle) {
            Some(Open::Dir { path, sent }) if !*sent => {
                *sent = true;
                path.clone()
            }
            Some(Open::Dir { .. }) => return Err(StatusCode::Eof),
            _ => return Err(StatusCode::Failure),
        };
        let mut files = Vec::new();
        for e in fs::read_dir(&path).map_err(io_status)?.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Ok(attrs) = self.attrs(&e.path()) {
                files.push(File::new(name, attrs));
            }
        }
        Ok(Name { id, files })
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        fs::remove_file(self.real(&filename)?).map_err(io_status)?;
        Ok(ok(id))
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        fs::create_dir(self.real(&path)?).map_err(io_status)?;
        Ok(ok(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        fs::remove_dir(self.real(&path)?).map_err(io_status)?;
        Ok(ok(id))
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let real = self.real(&path)?;
        Ok(Name {
            id,
            files: vec![File::dummy(self.virt(&real))],
        })
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        fs::rename(self.real(&oldpath)?, self.real(&newpath)?).map_err(io_status)?;
        Ok(ok(id))
    }
}
