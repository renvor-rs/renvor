//! The storage port: a key that exists cannot traverse, and an object that exists is within its
//! bound.
//!
//! # The key is the first layer
//!
//! [`ObjectKey::new`] refuses every shape a traversal or a platform surprise could take: empty
//! segments (which cover a leading, trailing, or doubled `/`), `.` and `..` segments, backslashes,
//! control characters, the characters Windows forbids in a name, names ending in a dot or a
//! space (which Windows strips), and Windows reserved device names in any case and with any
//! extension (FR-056, FR-061, T-3). The filesystem adapter's `cap_std::fs::Dir` is the second
//! layer; both are measured, so the defence is in depth rather than in prose.
//!
//! # Keys are bytes
//!
//! Two keys are equal when their bytes are equal. On a case-insensitive filesystem two keys that
//! differ only in case share one file; that residual is stated here and in the contract rather
//! than papered over by lowercasing, which would make the port lossy on the systems where it is
//! not needed.
//!
//! # `put` is last-writer-wins
//!
//! There is no conditional put (ADR-0035 decision 5). Two writers to one key leave the bytes of
//! whichever finished last, whole — never interleaved, because every adapter writes atomically.

use core::fmt;
use core::future::Future;
use std::sync::Arc;

use renvor_core::observe::metrics::{Counter, MetricsError, Registry};

/// The most bytes a key may carry.
pub const MAX_KEY_BYTES: usize = 1024;
/// The default ceiling on one object.
pub const DEFAULT_MAX_OBJECT_BYTES: u64 = 64 * 1024 * 1024;
/// The hard cap on the configurable object ceiling.
pub const MAX_OBJECT_BYTES_CAP: u64 = 1024 * 1024 * 1024;
/// The most bytes a content type may carry.
pub const MAX_CONTENT_TYPE_BYTES: usize = 255;
/// The tracing target every storage event is emitted on.
pub const STORAGE_EVENT_TARGET: &str = "renvor.storage";

/// Why an input was refused before any I/O. **Closed and fieldless.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StorageRefusal {
    /// The key broke a rule in the module documentation.
    KeyInvalid,
    /// The object exceeds the configured ceiling, on write or on read.
    ObjectTooLarge,
    /// The content type is not an RFC 9110 media type within 255 bytes.
    ContentTypeInvalid,
    /// A configured bound exceeded its cap or fell below its floor.
    BoundOutOfRange,
    /// A backend setting that cannot be used.
    SettingsInvalid,
}

impl StorageRefusal {
    /// A stable label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KeyInvalid => "key_invalid",
            Self::ObjectTooLarge => "object_too_large",
            Self::ContentTypeInvalid => "content_type_invalid",
            Self::BoundOutOfRange => "bound_out_of_range",
            Self::SettingsInvalid => "settings_invalid",
        }
    }
}

/// Why an operation failed. **Closed; no variant carries text**, so no path, bucket, or backend
/// message travels (FR-063).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum StorageError {
    /// The backend could not be reached or the operation failed for a reason that is not a
    /// refusal, a denial, or capacity.
    #[error("the object store is unavailable")]
    Unavailable,
    /// The operation ran past its bound.
    #[error("the object store operation timed out")]
    TimedOut,
    /// A Renvor bound refused the input before any I/O.
    #[error("the object store refused an input: {}", .0.as_str())]
    Refused(StorageRefusal),
    /// The backend refused the credential or the operation.
    #[error("the object store denied the operation")]
    Denied,
    /// The backend or the substitute has no room.
    #[error("the object store is at capacity")]
    Capacity,
}

impl StorageError {
    /// A stable label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::TimedOut => "timed_out",
            Self::Refused(_) => "refused",
            Self::Denied => "denied",
            Self::Capacity => "capacity",
        }
    }
}

/// Windows reserved device names, refused as a segment stem in any case.
const RESERVED_STEMS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// True when one segment passes every rule.
fn valid_segment(segment: &str) -> bool {
    if segment.is_empty() || segment == "." || segment == ".." {
        return false;
    }
    if segment.ends_with('.') || segment.ends_with(' ') {
        return false;
    }
    if segment.bytes().any(|byte| {
        byte < 0x20
            || byte == 0x7f
            || matches!(byte, b'\\' | b':' | b'*' | b'?' | b'"' | b'<' | b'>' | b'|')
    }) {
        return false;
    }
    // The stem is everything before the first dot: `CON.txt` and `con` are both the device.
    let stem = segment.split('.').next().unwrap_or("");
    !RESERVED_STEMS
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
}

/// A validated object key.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectKey(String);

impl ObjectKey {
    /// Validates `text` against every rule in the module documentation.
    ///
    /// # Errors
    ///
    /// [`StorageError::Refused`] with [`StorageRefusal::KeyInvalid`].
    pub fn new(text: &str) -> Result<Self, StorageError> {
        let valid =
            !text.is_empty() && text.len() <= MAX_KEY_BYTES && text.split('/').all(valid_segment);
        if valid {
            Ok(Self(text.to_owned()))
        } else {
            Err(StorageError::Refused(StorageRefusal::KeyInvalid))
        }
    }

    /// The key as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The segments between the separators, each already validated.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }
}

impl fmt::Debug for ObjectKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // A key may name a user or a document; the shape is enough for a test log.
        write!(
            f,
            "ObjectKey({} bytes, {} segments)",
            self.0.len(),
            self.0.split('/').count()
        )
    }
}

/// True for an RFC 9110 `tchar`.
const fn is_tchar(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn is_token(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(is_tchar)
}

/// `"…"` with no control character inside; `\` escapes are accepted as RFC 9110 allows.
fn is_quoted_string(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 2
        && bytes[0] == b'"'
        && bytes[bytes.len() - 1] == b'"'
        && !bytes[1..bytes.len() - 1]
            .iter()
            .any(|byte| *byte < 0x20 || *byte == 0x7f)
}

/// A bounded RFC 9110 media type: `type/subtype` with optional `;name=value` parameters.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ContentType(String);

impl ContentType {
    /// Validates `text` against the media-type grammar and the 255-byte bound.
    ///
    /// # Errors
    ///
    /// [`StorageError::Refused`] with [`StorageRefusal::ContentTypeInvalid`].
    pub fn new(text: &str) -> Result<Self, StorageError> {
        let refused = StorageError::Refused(StorageRefusal::ContentTypeInvalid);
        if text.is_empty() || text.len() > MAX_CONTENT_TYPE_BYTES {
            return Err(refused);
        }
        let mut parts = text.split(';');
        let media = parts.next().unwrap_or("").trim_matches([' ', '\t']);
        let Some((kind, subtype)) = media.split_once('/') else {
            return Err(refused);
        };
        if !is_token(kind) || !is_token(subtype) {
            return Err(refused);
        }
        for parameter in parts {
            let parameter = parameter.trim_matches([' ', '\t']);
            let Some((name, value)) = parameter.split_once('=') else {
                return Err(refused);
            };
            if !is_token(name) || !(is_token(value) || is_quoted_string(value)) {
                return Err(refused);
            }
        }
        Ok(Self(text.to_owned()))
    }

    /// The media type as given.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The bounds an application configured for its object store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageBounds {
    max_object_bytes: u64,
}

impl Default for StorageBounds {
    fn default() -> Self {
        Self {
            max_object_bytes: DEFAULT_MAX_OBJECT_BYTES,
        }
    }
}

impl StorageBounds {
    /// The documented defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the object ceiling. Refused at zero or above [`MAX_OBJECT_BYTES_CAP`].
    ///
    /// # Errors
    ///
    /// [`StorageError::Refused`] with [`StorageRefusal::BoundOutOfRange`].
    pub fn with_max_object_bytes(mut self, bytes: u64) -> Result<Self, StorageError> {
        if bytes == 0 || bytes > MAX_OBJECT_BYTES_CAP {
            return Err(StorageError::Refused(StorageRefusal::BoundOutOfRange));
        }
        self.max_object_bytes = bytes;
        Ok(self)
    }

    /// The object ceiling.
    #[must_use]
    pub const fn max_object_bytes(&self) -> u64 {
        self.max_object_bytes
    }

    /// Refuses `len` bytes above the ceiling.
    ///
    /// # Errors
    ///
    /// [`StorageError::Refused`] with [`StorageRefusal::ObjectTooLarge`].
    pub fn check(&self, len: u64) -> Result<(), StorageError> {
        if len > self.max_object_bytes {
            Err(StorageError::Refused(StorageRefusal::ObjectTooLarge))
        } else {
            Ok(())
        }
    }
}

/// What `head` returns: the size and the content type, never the bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectMeta {
    /// The object's size in bytes.
    pub size: u64,
    /// The content type recorded at `put`, if any.
    pub content_type: Option<ContentType>,
}

/// What `get` returns.
#[derive(Clone, PartialEq, Eq)]
pub struct Object {
    /// The bytes.
    pub bytes: Vec<u8>,
    /// The content type recorded at `put`, if any.
    pub content_type: Option<ContentType>,
}

impl fmt::Debug for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Object")
            .field("bytes", &self.bytes.len())
            .field("content_type", &self.content_type)
            .finish()
    }
}

/// What `delete` returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deleted {
    /// The object existed and is gone.
    Deleted,
    /// No object had that key.
    Absent,
}

/// Something that stores objects by key.
pub trait ObjectStore: Send + Sync {
    /// Stores `bytes` under `key`, replacing whatever was there (last-writer-wins). The bound is
    /// checked before a byte is written.
    fn put(
        &self,
        key: &ObjectKey,
        bytes: Vec<u8>,
        content_type: Option<ContentType>,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// The object, or `None` when absent.
    fn get(
        &self,
        key: &ObjectKey,
    ) -> impl Future<Output = Result<Option<Object>, StorageError>> + Send;

    /// The object's metadata, or `None` when absent.
    fn head(
        &self,
        key: &ObjectKey,
    ) -> impl Future<Output = Result<Option<ObjectMeta>, StorageError>> + Send;

    /// Removes the object; `Deleted::Absent` when there was none.
    fn delete(&self, key: &ObjectKey)
    -> impl Future<Output = Result<Deleted, StorageError>> + Send;

    /// Proves the backend is reachable and writable, for Boot (FR-060). The substitute answers
    /// at once.
    fn probe(&self) -> impl Future<Output = Result<(), StorageError>> + Send {
        async { Ok(()) }
    }
}

impl<T> ObjectStore for Arc<T>
where
    T: ObjectStore + ?Sized,
{
    fn put(
        &self,
        key: &ObjectKey,
        bytes: Vec<u8>,
        content_type: Option<ContentType>,
    ) -> impl Future<Output = Result<(), StorageError>> + Send {
        (**self).put(key, bytes, content_type)
    }

    fn get(
        &self,
        key: &ObjectKey,
    ) -> impl Future<Output = Result<Option<Object>, StorageError>> + Send {
        (**self).get(key)
    }

    fn head(
        &self,
        key: &ObjectKey,
    ) -> impl Future<Output = Result<Option<ObjectMeta>, StorageError>> + Send {
        (**self).head(key)
    }

    fn delete(
        &self,
        key: &ObjectKey,
    ) -> impl Future<Output = Result<Deleted, StorageError>> + Send {
        (**self).delete(key)
    }

    fn probe(&self) -> impl Future<Output = Result<(), StorageError>> + Send {
        (**self).probe()
    }
}

/// The storage counter (FR-083): `renvor_storage_operations_total{backend, op, outcome}`.
#[derive(Clone, Debug)]
pub struct StorageMetrics {
    operations: Counter,
}

impl StorageMetrics {
    /// Registers the family, or returns the existing one.
    ///
    /// # Errors
    ///
    /// [`MetricsError`] when a family of the same name is registered with another shape.
    pub fn register(registry: &Registry) -> Result<Self, MetricsError> {
        Ok(Self {
            operations: registry.counter(
                "renvor_storage_operations_total",
                "Object store operations by op and closed outcome.",
                &["backend", "op", "outcome"],
            )?,
        })
    }

    /// Counts one operation.
    pub fn record(&self, backend: &str, op: &str, outcome: Result<(), StorageError>) {
        let outcome = match outcome {
            Ok(()) => "ok",
            Err(error) => error.as_str(),
        };
        self.operations
            .increment(&[("backend", backend), ("op", op), ("outcome", outcome)], 1);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ContentType, MAX_KEY_BYTES, MAX_OBJECT_BYTES_CAP, Object, ObjectKey, StorageBounds,
        StorageError, StorageRefusal,
    };

    #[test]
    fn keys_follow_every_rule() {
        for ok in [
            "a",
            "a/b/c.txt",
            "uploads/2026/09/report.pdf",
            "with space/inside",
            "unicode/ünïcödé.txt",
            "CONTROL",
            "console.log",
            "com10/x",
        ] {
            assert!(ObjectKey::new(ok).is_ok(), "an accepted key was refused");
        }
        let longest = "k".repeat(MAX_KEY_BYTES);
        assert!(
            ObjectKey::new(&longest).is_ok(),
            "the boundary is inclusive"
        );
        let refused = [
            "",
            "/a",
            "a/",
            "a//b",
            ".",
            "..",
            "a/../b",
            "a/./b",
            "../etc/passwd",
            "a\\b",
            "a\0b",
            "a\nb",
            "a\x7fb",
            "c:/x",
            "a*b",
            "a?b",
            "a\"b",
            "a<b",
            "a>b",
            "a|b",
            "a.",
            "a ",
            "dir/a.",
            "CON",
            "con",
            "Con.txt",
            "dir/NUL",
            "COM1.log",
            "lpt9",
            "AUX.tar.gz",
        ];
        for (index, bad) in refused.into_iter().enumerate() {
            assert_eq!(
                ObjectKey::new(bad).unwrap_err(),
                StorageError::Refused(StorageRefusal::KeyInvalid),
                "rejected key case {index} was accepted"
            );
        }
        assert!(ObjectKey::new(&"k".repeat(MAX_KEY_BYTES + 1)).is_err());
        assert_ne!(
            ObjectKey::new("Case").unwrap(),
            ObjectKey::new("case").unwrap(),
            "keys are bytes"
        );
        assert_eq!(
            ObjectKey::new("a/b/c")
                .unwrap()
                .segments()
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn content_types_follow_the_media_type_grammar() {
        for ok in [
            "text/plain",
            "application/json",
            "text/plain; charset=utf-8",
            "text/plain;charset=utf-8",
            "multipart/form-data; boundary=\"a b\"",
            "application/vnd.api+json",
        ] {
            assert!(
                ContentType::new(ok).is_ok(),
                "an accepted media type was refused"
            );
        }
        for (index, bad) in [
            "",
            "text",
            "text/",
            "/plain",
            "text plain",
            "text/plain; charset",
            "text/plain; =utf-8",
            "text/plain; charset=\"utf\n8\"",
            "text/plain\r\nX: y",
            "text/pl√§in",
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                ContentType::new(bad).unwrap_err(),
                StorageError::Refused(StorageRefusal::ContentTypeInvalid),
                "rejected media-type case {index} was accepted"
            );
        }
        assert!(ContentType::new(&format!("text/{}", "p".repeat(300))).is_err());
    }

    #[test]
    fn bounds_are_capped_and_checked_inclusively() {
        let bounds = StorageBounds::new().with_max_object_bytes(10).unwrap();
        assert!(bounds.check(10).is_ok());
        assert_eq!(
            bounds.check(11).unwrap_err(),
            StorageError::Refused(StorageRefusal::ObjectTooLarge)
        );
        assert!(StorageBounds::new().with_max_object_bytes(0).is_err());
        assert!(
            StorageBounds::new()
                .with_max_object_bytes(MAX_OBJECT_BYTES_CAP)
                .is_ok()
        );
        assert!(
            StorageBounds::new()
                .with_max_object_bytes(MAX_OBJECT_BYTES_CAP + 1)
                .is_err()
        );
    }

    #[test]
    fn debug_shows_shapes_not_contents() {
        let key = ObjectKey::new("users/hunter2CanaryDoNotLeak/avatar.png").unwrap();
        let rendered = format!("{key:?}");
        assert!(!rendered.contains("hunter2"), "the key was rendered");
        assert!(rendered.contains("3 segments"));
        let object = Object {
            bytes: b"hunter2CanaryDoNotLeak".to_vec(),
            content_type: None,
        };
        let rendered = format!("{object:?}");
        assert!(!rendered.contains("hunter2"), "the payload was rendered");
        assert!(rendered.contains("bytes: 22"));
    }
}
