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
//! # Writes are atomic, and one rename carries the bytes and the content type together
//!
//! One object is **one file** under `objects/`: a small header, then the bytes.
//!
//! ```text
//! "RVO1"                 4 bytes   the magic
//! content-type length    2 bytes   big-endian u16; 0 when there is no content type
//! content type           n bytes   at most 255 (C-C2, `MAX_CONTENT_TYPE_BYTES`)
//! the object's bytes     the rest of the file
//! ```
//!
//! The file is written to a temporary name in its destination directory and renamed into place
//! (`cap_tempfile::TempFile::replace`), so a reader sees the old object or the new one and never
//! a partial one — and, because the content type travels in the same file, never one writer's
//! bytes with another writer's content type. That is C-C5 for this port: `put` is
//! last-writer-wins, **whole, never interleaved**. The earlier layout kept the content type in a
//! sidecar under a second tree, written by a second rename; two concurrent writers interleaved
//! the two renames and a reader could pair writer A's bytes with writer B's content type (Codex
//! finding 13). Two files cannot be replaced by one rename; one file can.
//!
//! `head` reads only the header — at most 4 + 2 + 255 bytes, whatever the object's size — and
//! reports the file's length minus the header's. `get` reads the header, checks the bound against
//! the **body's** length before reading a byte of it (the bound on read stays, and the header is
//! the adapter's, not the caller's), then reads the rest. Every read comes from the one handle
//! opened for the operation, so a `put` that lands mid-read replaces the name, not the bytes the
//! reader is holding.
//!
//! A file that does not decode — a bad magic, a header the file ends inside, a content-type
//! length past the bound, a content type that fails the grammar — is corrupt: `head` and `get`
//! answer [`StorageError::Unavailable`] and emit one `ERROR` event on [`STORAGE_EVENT_TARGET`]
//! carrying a closed reason and nothing else (FR-063). `delete` removes the file whatever it
//! holds, so a corrupt object is not a trap.
//!
//! **Pre-release: no compatibility with the earlier two-tree layout is promised or provided.** A
//! root written by that layout holds unframed objects, which this adapter reports as corrupt,
//! beside a `meta/` tree it never reads.
//!
//! # Blocking I/O, bounded
//!
//! Filesystem calls run on Tokio's blocking pool under one timeout. A call that outruns the bound
//! is reported as `TimedOut`; the blocking task itself cannot be cancelled and finishes on its own.
//!
//! # What reaches the log
//!
//! Nothing from a path, a key, or a file. Every `io::Error` is classified into the closed
//! [`StorageError`] by its kind and never rendered (FR-063).

use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cap_std::ambient_authority;
use cap_std::fs::{Dir, File};

use crate::port::{
    ContentType, Deleted, MAX_CONTENT_TYPE_BYTES, Object, ObjectKey, ObjectMeta, ObjectStore,
    STORAGE_EVENT_TARGET, StorageBounds, StorageError, StorageMetrics, StorageRefusal,
};

/// The backend label in metrics and events.
pub const BACKEND: &str = "filesystem";
/// The default bound on one operation.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// The floor and cap on the operation bound.
pub const TIMEOUT_RANGE: (Duration, Duration) = (Duration::from_secs(1), Duration::from_secs(600));
/// The subdirectory objects live under.
const OBJECTS: &str = "objects";
/// The first bytes of every object file.
const MAGIC: [u8; 4] = *b"RVO1";
/// The fixed part of the header: the magic and the big-endian content-type length.
const FIXED_HEADER_BYTES: usize = MAGIC.len() + 2;

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

/// Why an object file could not be decoded. **Closed**: the event carries the label and never
/// the key, the path, or a byte of the file.
#[derive(Clone, Copy, Debug)]
enum Corruption {
    /// The file does not begin with the magic.
    BadMagic,
    /// The file ends inside the header it declares, or inside the body it was measured to hold.
    Truncated,
    /// The header claims a content type longer than the bound (C-C2).
    ContentTypeTooLong,
    /// The header's content type fails the media-type grammar.
    ContentTypeInvalid,
}

impl Corruption {
    const fn as_str(self) -> &'static str {
        match self {
            Self::BadMagic => "bad_magic",
            Self::Truncated => "truncated",
            Self::ContentTypeTooLong => "content_type_too_long",
            Self::ContentTypeInvalid => "content_type_invalid",
        }
    }
}

/// Reports a corrupt object file: one fixed event with closed fields, then `Unavailable`.
fn corrupt(reason: Corruption) -> StorageError {
    tracing::error!(
        target: STORAGE_EVENT_TARGET,
        backend = BACKEND,
        reason = reason.as_str(),
        "an object file does not decode; the object is unreadable"
    );
    StorageError::Unavailable
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

/// The header for `content_type`: the magic, the big-endian length, the media type.
fn header(content_type: Option<&ContentType>) -> Result<Vec<u8>, StorageError> {
    let text = content_type.map_or("", ContentType::as_str).as_bytes();
    // `ContentType::new` bounds the text at `MAX_CONTENT_TYPE_BYTES` (C-C2), so the length fits
    // the field; a value that did not would be a defect in the port, refused rather than written.
    let length = u16::try_from(text.len())
        .map_err(|_| StorageError::Refused(StorageRefusal::ContentTypeInvalid))?;
    let mut header = Vec::with_capacity(FIXED_HEADER_BYTES + text.len());
    header.extend_from_slice(&MAGIC);
    header.extend_from_slice(&length.to_be_bytes());
    header.extend_from_slice(text);
    Ok(header)
}

/// Writes one object file at `path` under `dir` — the header, then `bytes` — to a temporary name
/// beside it and renames it into place, so one rename carries the content type and the bytes.
fn write_atomically(
    dir: &Dir,
    path: &Path,
    content_type: Option<&ContentType>,
    bytes: &[u8],
) -> Result<(), StorageError> {
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
    let header = header(content_type)?;
    let mut temporary = cap_tempfile::TempFile::new(&target).map_err(|error| classify(&error))?;
    temporary
        .write_all(&header)
        .and_then(|()| temporary.write_all(bytes))
        .and_then(|()| temporary.flush())
        .map_err(|error| classify(&error))?;
    temporary.replace(name).map_err(|error| classify(&error))
}

/// Opens the object at `path` for reading and measures it through the handle, so the length and
/// every read that follows describe one version of the file even while a `put` replaces the
/// name. `Ok(None)` when absent; `Denied` when a symbolic link.
fn open_object(dir: &Dir, path: &Path) -> Result<Option<(File, u64)>, StorageError> {
    if !present_not_link(dir, path)? {
        return Ok(None);
    }
    let file = match dir.open(path) {
        Ok(file) => file,
        // Removed between the check and the open: absent, not a failure.
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(classify(&error)),
    };
    let length = file.metadata().map_err(|error| classify(&error))?.len();
    Ok(Some((file, length)))
}

/// Fills `buffer` from `file`; a file that ends first is truncated.
fn read_fully(file: &mut File, buffer: &mut [u8]) -> Result<(), StorageError> {
    file.read_exact(buffer).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            corrupt(Corruption::Truncated)
        } else {
            classify(&error)
        }
    })
}

/// Reads the header from the front of an open object file — at most `FIXED_HEADER_BYTES +
/// MAX_CONTENT_TYPE_BYTES` bytes, whatever the file's length — and returns its length in bytes
/// with the content type it carries.
fn read_header(file: &mut File) -> Result<(u64, Option<ContentType>), StorageError> {
    let mut fixed = [0u8; FIXED_HEADER_BYTES];
    read_fully(file, &mut fixed)?;
    if fixed[..MAGIC.len()] != MAGIC {
        return Err(corrupt(Corruption::BadMagic));
    }
    let length = usize::from(u16::from_be_bytes([fixed[4], fixed[5]]));
    if length > MAX_CONTENT_TYPE_BYTES {
        return Err(corrupt(Corruption::ContentTypeTooLong));
    }
    let content_type = if length == 0 {
        None
    } else {
        let mut raw = [0u8; MAX_CONTENT_TYPE_BYTES];
        read_fully(file, &mut raw[..length])?;
        let text = core::str::from_utf8(&raw[..length])
            .map_err(|_| corrupt(Corruption::ContentTypeInvalid))?;
        Some(ContentType::new(text).map_err(|_| corrupt(Corruption::ContentTypeInvalid))?)
    };
    Ok(((FIXED_HEADER_BYTES + length) as u64, content_type))
}

/// The body's length: the file's length less the header's. A file measured shorter than the
/// header it was just read from was truncated under the reader.
fn body_length(file_length: u64, header_length: u64) -> Result<u64, StorageError> {
    file_length
        .checked_sub(header_length)
        .ok_or_else(|| corrupt(Corruption::Truncated))
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
        self.run("put", move |dir| {
            present_not_link(dir, &object)?;
            write_atomically(dir, &object, content_type.as_ref(), &bytes)
        })
        .await
    }

    async fn get(&self, key: &ObjectKey) -> Result<Option<Object>, StorageError> {
        let object = relative(OBJECTS, key);
        let bounds = self.bounds;
        self.run("get", move |dir| {
            let Some((mut file, length)) = open_object(dir, &object)? else {
                return Ok(None);
            };
            let (header, content_type) = read_header(&mut file)?;
            let size = body_length(length, header)?;
            // The bound on read, against the body: a file grown past the ceiling by something
            // else is refused before a byte of the body is read.
            bounds.check(size)?;
            // A capacity hint only; the bound has already held the size to at most 1 GiB.
            let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
            io::Read::by_ref(&mut file)
                .take(size)
                .read_to_end(&mut bytes)
                .map_err(|error| classify(&error))?;
            if bytes.len() as u64 != size {
                return Err(corrupt(Corruption::Truncated));
            }
            Ok(Some(Object {
                bytes,
                content_type,
            }))
        })
        .await
    }

    async fn head(&self, key: &ObjectKey) -> Result<Option<ObjectMeta>, StorageError> {
        let object = relative(OBJECTS, key);
        self.run("head", move |dir| {
            let Some((mut file, length)) = open_object(dir, &object)? else {
                return Ok(None);
            };
            let (header, content_type) = read_header(&mut file)?;
            let size = body_length(length, header)?;
            Ok(Some(ObjectMeta { size, content_type }))
        })
        .await
    }

    async fn delete(&self, key: &ObjectKey) -> Result<Deleted, StorageError> {
        let object = relative(OBJECTS, key);
        self.run("delete", move |dir| {
            if !present_not_link(dir, &object)? {
                return Ok(Deleted::Absent);
            }
            match dir.remove_file(&object) {
                Ok(()) => Ok(Deleted::Deleted),
                // Removed between the check and the removal: absent, not a failure.
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Deleted::Absent),
                Err(error) => Err(classify(&error)),
            }
        })
        .await
    }

    async fn probe(&self) -> Result<(), StorageError> {
        // A temporary file created, written, and removed on drop: the root exists, is a
        // directory this process may write in, and has room for one small file (FR-060).
        self.run("probe", |dir| {
            dir.create_dir_all(OBJECTS)
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
