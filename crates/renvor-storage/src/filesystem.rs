//! The filesystem adapter on `cap-std` (ADR-0035), behind the `filesystem` feature.
//!
//! # The root is a capability
//!
//! Every operation goes through one [`cap_std::fs::Dir`] opened at construction. A `Dir` cannot
//! open a path outside itself — `..`, an absolute path, or a symbolic link that escapes are refused
//! by the capability's own resolution, not by string checks — so even if the key validator were
//! wrong, a key could not reach outside the root (FR-059, T-3). Both layers are measured.
//!
//! # Symbolic links are refused outright
//!
//! An object path that is a symbolic link is `Denied` before it is read, written, or removed,
//! whether or not it stays inside the root. The root is a store, not a view onto other files; a
//! link there is something an operator put in place, and answering through it would make the
//! store return bytes nobody stored. Links in **intermediate** directories inside the root are
//! resolved by the capability within the sandbox; that residual is stated here.
//!
//! # Writes are atomic
//!
//! An object is written to a temporary file in its destination directory and renamed into place
//! (`cap_tempfile::TempFile::replace`), so a reader sees the old object or the new one and never a
//! partial one. The content type is a small sidecar under a separate tree (`meta/`), written
//! before the bytes; a crash between the two leaves a sidecar without an object, and `head`
//! answers from the object tree first so an orphaned sidecar is invisible.
//!
//! # Blocking I/O, bounded
//!
//! Filesystem calls run on Tokio's blocking pool under one timeout. A call that outruns the bound
//! is reported as `TimedOut`; the blocking task itself cannot be cancelled and finishes on its own.
//!
//! # What reaches the log
//!
//! Nothing from a path or a key. Every `io::Error` is classified into the closed
//! [`StorageError`] by its kind and never rendered (FR-063).

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cap_std::ambient_authority;
use cap_std::fs::Dir;

use crate::port::{
    ContentType, Deleted, Object, ObjectKey, ObjectMeta, ObjectStore, STORAGE_EVENT_TARGET,
    StorageBounds, StorageError, StorageMetrics, StorageRefusal,
};

/// The backend label in metrics and events.
pub const BACKEND: &str = "filesystem";
/// The default bound on one operation.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// The floor and cap on the operation bound.
pub const TIMEOUT_RANGE: (Duration, Duration) = (Duration::from_secs(1), Duration::from_secs(600));
/// The subdirectory objects live under.
const OBJECTS: &str = "objects";
/// The subdirectory content-type sidecars live under.
const META: &str = "meta";
/// The most bytes a sidecar is allowed to be when read back.
const MAX_SIDECAR_BYTES: u64 = 512;

/// Settings for the filesystem adapter.
#[derive(Clone)]
pub struct FilesystemSettings {
    root: PathBuf,
    bounds: StorageBounds,
    timeout: Duration,
}

impl core::fmt::Debug for FilesystemSettings {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The root is an operator's path: not rendered.
        f.debug_struct("FilesystemSettings")
            .field("bounds", &self.bounds)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl FilesystemSettings {
    /// A store rooted at `root`, which must already exist.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, bounds: StorageBounds) -> Self {
        Self {
            root: root.into(),
            bounds,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Replaces the operation bound (1 s – 10 min).
    ///
    /// # Errors
    ///
    /// [`StorageError::Refused`] with [`StorageRefusal::BoundOutOfRange`].
    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self, StorageError> {
        if timeout < TIMEOUT_RANGE.0 || timeout > TIMEOUT_RANGE.1 {
            return Err(StorageError::Refused(StorageRefusal::BoundOutOfRange));
        }
        self.timeout = timeout;
        Ok(self)
    }
}

/// Maps an I/O failure onto the closed error by its kind. The error is never rendered.
fn classify(error: &io::Error) -> StorageError {
    match error.kind() {
        io::ErrorKind::PermissionDenied => StorageError::Denied,
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => StorageError::Capacity,
        io::ErrorKind::TimedOut => StorageError::TimedOut,
        _ => StorageError::Unavailable,
    }
}

/// The relative path a key maps to under a tree: the segments joined by `/`, which `cap-std`
/// resolves on every platform.
fn relative(tree: &str, key: &ObjectKey) -> PathBuf {
    let mut path = PathBuf::from(tree);
    for segment in key.segments() {
        path.push(segment);
    }
    path
}

/// `Denied` when `path` is a symbolic link; `Ok(false)` when absent; `Ok(true)` when a file.
fn present_not_link(dir: &Dir, path: &Path) -> Result<bool, StorageError> {
    match dir.symlink_metadata(path) {
        Ok(meta) if meta.is_symlink() => Err(StorageError::Denied),
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(classify(&error)),
    }
}

/// Writes `bytes` atomically at `path` under `dir`: temporary file beside it, then rename.
fn write_atomically(dir: &Dir, path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let (parent, name) = match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => (parent, name),
        _ => return Err(StorageError::Refused(StorageRefusal::KeyInvalid)),
    };
    if !parent.as_os_str().is_empty() {
        dir.create_dir_all(parent)
            .map_err(|error| classify(&error))?;
    }
    let target = if parent.as_os_str().is_empty() {
        dir.open_dir(".").map_err(|error| classify(&error))?
    } else {
        dir.open_dir(parent).map_err(|error| classify(&error))?
    };
    let mut temporary = cap_tempfile::TempFile::new(&target).map_err(|error| classify(&error))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.flush())
        .map_err(|error| classify(&error))?;
    temporary.replace(name).map_err(|error| classify(&error))
}

/// An object store rooted in one directory.
pub struct FilesystemStore {
    dir: Arc<Dir>,
    bounds: StorageBounds,
    timeout: Duration,
    metrics: Option<StorageMetrics>,
}

impl core::fmt::Debug for FilesystemStore {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FilesystemStore")
            .field("bounds", &self.bounds)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl FilesystemStore {
    /// Opens the root as a capability. The root must exist; writability is proved by
    /// [`ObjectStore::probe`], which the provider calls at Boot.
    ///
    /// # Errors
    ///
    /// [`StorageError::Unavailable`] when the root does not exist, [`StorageError::Denied`] when
    /// it cannot be opened.
    pub fn open(settings: &FilesystemSettings) -> Result<Self, StorageError> {
        let dir = Dir::open_ambient_dir(&settings.root, ambient_authority())
            .map_err(|error| classify(&error))?;
        Ok(Self {
            dir: Arc::new(dir),
            bounds: settings.bounds,
            timeout: settings.timeout,
            metrics: None,
        })
    }

    /// Counts operations in `metrics`.
    #[must_use]
    pub fn with_metrics(mut self, metrics: StorageMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// The bounds this store validates against.
    #[must_use]
    pub const fn bounds(&self) -> &StorageBounds {
        &self.bounds
    }

    /// Runs `work` on the blocking pool under the operation bound, recording the outcome.
    async fn run<T, F>(&self, op: &'static str, work: F) -> Result<T, StorageError>
    where
        T: Send + 'static,
        F: FnOnce(&Dir) -> Result<T, StorageError> + Send + 'static,
    {
        let started = Instant::now();
        let dir = Arc::clone(&self.dir);
        let outcome = match tokio::time::timeout(
            self.timeout,
            tokio::task::spawn_blocking(move || work(&dir)),
        )
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(_join)) => Err(StorageError::Unavailable),
            Err(_elapsed) => Err(StorageError::TimedOut),
        };
        let label = match &outcome {
            Ok(_) => Ok(()),
            Err(error) => Err(*error),
        };
        if let Some(metrics) = &self.metrics {
            metrics.record(BACKEND, op, label);
        }
        tracing::debug!(
            target: STORAGE_EVENT_TARGET,
            backend = BACKEND,
            op,
            outcome = match label {
                Ok(()) => "ok",
                Err(error) => error.as_str(),
            },
            duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "object store operation finished"
        );
        outcome
    }
}

impl ObjectStore for FilesystemStore {
    async fn put(
        &self,
        key: &ObjectKey,
        bytes: Vec<u8>,
        content_type: Option<ContentType>,
    ) -> Result<(), StorageError> {
        self.bounds.check(bytes.len() as u64)?;
        let object = relative(OBJECTS, key);
        let meta = relative(META, key);
        self.run("put", move |dir| {
            present_not_link(dir, &object)?;
            match content_type {
                Some(content_type) => {
                    write_atomically(dir, &meta, content_type.as_str().as_bytes())?
                }
                None => match dir.remove_file(&meta) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(classify(&error)),
                },
            }
            write_atomically(dir, &object, &bytes)
        })
        .await
    }

    async fn get(&self, key: &ObjectKey) -> Result<Option<Object>, StorageError> {
        let object = relative(OBJECTS, key);
        let meta = relative(META, key);
        let bounds = self.bounds;
        self.run("get", move |dir| {
            if !present_not_link(dir, &object)? {
                return Ok(None);
            }
            let size = dir
                .metadata(&object)
                .map_err(|error| classify(&error))?
                .len();
            // The bound on read: a file grown past the ceiling by something else is refused.
            bounds.check(size)?;
            let bytes = dir.read(&object).map_err(|error| classify(&error))?;
            let content_type = read_sidecar(dir, &meta)?;
            Ok(Some(Object {
                bytes,
                content_type,
            }))
        })
        .await
    }

    async fn head(&self, key: &ObjectKey) -> Result<Option<ObjectMeta>, StorageError> {
        let object = relative(OBJECTS, key);
        let meta = relative(META, key);
        self.run("head", move |dir| {
            if !present_not_link(dir, &object)? {
                return Ok(None);
            }
            let size = dir
                .metadata(&object)
                .map_err(|error| classify(&error))?
                .len();
            let content_type = read_sidecar(dir, &meta)?;
            Ok(Some(ObjectMeta { size, content_type }))
        })
        .await
    }

    async fn delete(&self, key: &ObjectKey) -> Result<Deleted, StorageError> {
        let object = relative(OBJECTS, key);
        let meta = relative(META, key);
        self.run("delete", move |dir| {
            if !present_not_link(dir, &object)? {
                return Ok(Deleted::Absent);
            }
            dir.remove_file(&object).map_err(|error| classify(&error))?;
            match dir.remove_file(&meta) {
                Ok(()) | Err(_) => {}
            }
            Ok(Deleted::Deleted)
        })
        .await
    }

    async fn probe(&self) -> Result<(), StorageError> {
        // A temporary file created, written, and removed on drop: the root exists, is a
        // directory this process may write in, and has room for one small file (FR-060).
        self.run("probe", |dir| {
            dir.create_dir_all(OBJECTS)
                .and_then(|()| dir.create_dir_all(META))
                .map_err(|error| classify(&error))?;
            let mut temporary =
                cap_tempfile::TempFile::new(dir).map_err(|error| classify(&error))?;
            temporary
                .write_all(b"renvor")
                .and_then(|()| temporary.flush())
                .map_err(|error| classify(&error))?;
            drop(temporary);
            Ok(())
        })
        .await
    }
}

/// The content type recorded beside an object, if the sidecar is present and still valid.
fn read_sidecar(dir: &Dir, meta: &Path) -> Result<Option<ContentType>, StorageError> {
    match dir.symlink_metadata(meta) {
        Ok(found) if found.is_symlink() => return Err(StorageError::Denied),
        Ok(found) if found.len() > MAX_SIDECAR_BYTES => return Ok(None),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(classify(&error)),
    }
    let bytes = dir.read(meta).map_err(|error| classify(&error))?;
    // A sidecar that no longer parses is treated as absent: the object is still the object.
    Ok(core::str::from_utf8(&bytes)
        .ok()
        .and_then(|text| ContentType::new(text).ok()))
}
