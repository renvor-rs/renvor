//! The cache port: four operations, bounded inputs, a closed error.
//!
//! # A value that exists is a value that can be stored
//!
//! Every bound this port enforces is enforced **at construction of the argument**, not inside the
//! implementation: [`CacheKey::new`] refuses a bad key, [`Ttl::within`] refuses a bad lifetime, and
//! [`CacheValue::within`] refuses an oversized value, each against the [`CacheBounds`] the
//! application configured. So the trait's methods take types that have already passed, and an
//! implementation — the memory substitute, the Valkey adapter, an author's own — cannot forget a
//! check, because there is no unchecked input for it to receive.
//!
//! # Four operations, deliberately
//!
//! `get`, `set`, `set_if_absent`, `delete`. No `increment`, no `expire`, no scan, no pipeline. A
//! cache port that grows into a data-structure server becomes a second database with weaker
//! durability, and the constitution's rule against silently substituting one storage for another
//! starts to bite at the port rather than at the adapter. `set_if_absent` is the one primitive
//! that gives an application a single-writer guarantee (FR-094); everything else composes from the
//! four.
//!
//! # A miss is not an error
//!
//! `get` returns `Ok(None)` for an absent or expired key. `Err` means the cache **failed** — the
//! server did not answer, the operation timed out — and there is no mode in which a failure is
//! reported as a miss (FR-021). An author who wants miss-on-failure writes that, visibly.

use core::fmt;
use core::future::Future;
use std::sync::Arc;
use std::time::Duration;

/// The most bytes a key may carry, after the namespace prefix is applied.
pub const MAX_KEY_BYTES: usize = 512;

/// The default ceiling on a value's size.
pub const DEFAULT_MAX_VALUE_BYTES: usize = 1024 * 1024;

/// The hard cap on the configurable value-size ceiling.
pub const MAX_VALUE_BYTES_CAP: usize = 8 * 1024 * 1024;

/// The shortest lifetime a value may be given.
pub const MIN_TTL: Duration = Duration::from_secs(1);

/// The default ceiling on a lifetime.
pub const DEFAULT_MAX_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// The hard cap on the configurable lifetime ceiling.
pub const MAX_TTL_CAP: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// The default per-operation timeout.
pub const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(2);

/// The hard cap on the per-operation timeout.
pub const OPERATION_TIMEOUT_CAP: Duration = Duration::from_secs(30);

/// The most bytes a namespace may carry.
pub const MAX_NAMESPACE_BYTES: usize = 64;

/// Why an input was refused before any I/O.
///
/// Closed and fieldless: the reason names the **bound**, never the value that broke it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Refusal {
    /// The key was empty.
    KeyEmpty,
    /// The key, with its namespace, exceeded [`MAX_KEY_BYTES`].
    KeyTooLong,
    /// The key contained a control character (the C0 range `0x00`–`0x1F`, `0x7F`, or the C1
    /// range `U+0080`–`U+009F`).
    KeyHasControlCharacter,
    /// The key contained whitespace — any character with the Unicode White_Space property,
    /// `U+00A0` and `U+3000` included, not only the ASCII blanks.
    KeyHasWhitespace,
    /// The value exceeded the configured ceiling.
    ValueTooLarge,
    /// The lifetime was below [`MIN_TTL`].
    TtlTooShort,
    /// The lifetime exceeded the configured ceiling.
    TtlTooLong,
    /// The namespace was empty, too long, or not `[a-z0-9_.-]`.
    NamespaceInvalid,
    /// A configured bound exceeded its hard cap or fell below its floor.
    BoundOutOfRange,
    /// An endpoint's host was not a lowercase DNS name or an IP literal, or its port was zero.
    EndpointInvalid,
    /// A credential had the wrong shape: an empty username, or one holding a control character
    /// or whitespace. The value itself is never named.
    CredentialInvalid,
}

impl Refusal {
    /// A stable label for a metric or a structured field.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KeyEmpty => "key_empty",
            Self::KeyTooLong => "key_too_long",
            Self::KeyHasControlCharacter => "key_has_control_character",
            Self::KeyHasWhitespace => "key_has_whitespace",
            Self::ValueTooLarge => "value_too_large",
            Self::TtlTooShort => "ttl_too_short",
            Self::TtlTooLong => "ttl_too_long",
            Self::NamespaceInvalid => "namespace_invalid",
            Self::BoundOutOfRange => "bound_out_of_range",
            Self::EndpointInvalid => "endpoint_invalid",
            Self::CredentialInvalid => "credential_invalid",
        }
    }
}

/// Why a cache operation failed.
///
/// **Closed, and no variant carries text** (FR-020). A server's reply, a driver's message, and a
/// socket's error string are all written by somebody else and would otherwise travel wherever this
/// error travels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum CacheError {
    /// The backend did not answer, refused the connection, or dropped it.
    #[error("the cache is unavailable")]
    Unavailable,
    /// The operation ran past its configured timeout.
    #[error("the cache operation timed out")]
    TimedOut,
    /// An input was refused before any I/O. The reason names the bound.
    #[error("the cache refused an input: {}", .0.as_str())]
    Refused(Refusal),
    /// The substitute's entry capacity was reached and nothing expired could be evicted.
    #[error("the cache is at capacity")]
    Capacity,
}

impl CacheError {
    /// A stable label for a metric or a structured field.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::TimedOut => "timed_out",
            Self::Refused(_) => "refused",
            Self::Capacity => "capacity",
        }
    }
}

/// The bounds an application configured, each with a default and a hard cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheBounds {
    max_value_bytes: usize,
    max_ttl: Duration,
    operation_timeout: Duration,
}

impl Default for CacheBounds {
    fn default() -> Self {
        Self {
            max_value_bytes: DEFAULT_MAX_VALUE_BYTES,
            max_ttl: DEFAULT_MAX_TTL,
            operation_timeout: DEFAULT_OPERATION_TIMEOUT,
        }
    }
}

impl CacheBounds {
    /// The documented defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the value-size ceiling. Refused above [`MAX_VALUE_BYTES_CAP`] or at zero.
    ///
    /// # Errors
    ///
    /// [`CacheError::Refused`] with [`Refusal::BoundOutOfRange`].
    pub fn with_max_value_bytes(mut self, bytes: usize) -> Result<Self, CacheError> {
        if bytes == 0 || bytes > MAX_VALUE_BYTES_CAP {
            return Err(CacheError::Refused(Refusal::BoundOutOfRange));
        }
        self.max_value_bytes = bytes;
        Ok(self)
    }

    /// Replaces the lifetime ceiling. Refused above [`MAX_TTL_CAP`] or below [`MIN_TTL`].
    ///
    /// # Errors
    ///
    /// [`CacheError::Refused`] with [`Refusal::BoundOutOfRange`].
    pub fn with_max_ttl(mut self, max_ttl: Duration) -> Result<Self, CacheError> {
        if max_ttl < MIN_TTL || max_ttl > MAX_TTL_CAP {
            return Err(CacheError::Refused(Refusal::BoundOutOfRange));
        }
        self.max_ttl = max_ttl;
        Ok(self)
    }

    /// Replaces the per-operation timeout. Refused above [`OPERATION_TIMEOUT_CAP`] or at zero.
    ///
    /// # Errors
    ///
    /// [`CacheError::Refused`] with [`Refusal::BoundOutOfRange`].
    pub fn with_operation_timeout(mut self, timeout: Duration) -> Result<Self, CacheError> {
        if timeout.is_zero() || timeout > OPERATION_TIMEOUT_CAP {
            return Err(CacheError::Refused(Refusal::BoundOutOfRange));
        }
        self.operation_timeout = timeout;
        Ok(self)
    }

    /// The value-size ceiling.
    #[must_use]
    pub const fn max_value_bytes(&self) -> usize {
        self.max_value_bytes
    }

    /// The lifetime ceiling.
    #[must_use]
    pub const fn max_ttl(&self) -> Duration {
        self.max_ttl
    }

    /// The per-operation timeout.
    #[must_use]
    pub const fn operation_timeout(&self) -> Duration {
        self.operation_timeout
    }
}

/// The prefix every key is stored under, so two applications sharing one server cannot collide.
///
/// Applied by the **implementation**, never by the caller (FR-014): an author writes `user:42`
/// and the adapter stores `shop:user:42`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Namespace(String);

impl Namespace {
    /// Validates a namespace: 1–64 bytes of `[a-z0-9_.-]`.
    ///
    /// # Errors
    ///
    /// [`CacheError::Refused`] with [`Refusal::NamespaceInvalid`].
    pub fn new(namespace: &str) -> Result<Self, CacheError> {
        let bytes = namespace.as_bytes();
        let valid = !bytes.is_empty()
            && bytes.len() <= MAX_NAMESPACE_BYTES
            && bytes.iter().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'_' | b'.' | b'-')
            });
        if !valid {
            return Err(CacheError::Refused(Refusal::NamespaceInvalid));
        }
        Ok(Self(namespace.to_owned()))
    }

    /// The namespace text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The stored form of `key` under this namespace: `<namespace>:<key>`.
    ///
    /// # Errors
    ///
    /// [`CacheError::Refused`] with [`Refusal::KeyTooLong`] when the prefixed key exceeds
    /// [`MAX_KEY_BYTES`] — the bound is on what is **stored**, so the namespace counts.
    pub fn qualify(&self, key: &CacheKey) -> Result<String, CacheError> {
        let qualified = format!("{}:{}", self.0, key.as_str());
        if qualified.len() > MAX_KEY_BYTES {
            return Err(CacheError::Refused(Refusal::KeyTooLong));
        }
        Ok(qualified)
    }
}

impl fmt::Debug for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Namespace({})", self.0)
    }
}

/// A validated key: non-empty, at most [`MAX_KEY_BYTES`], no control character, no whitespace —
/// both as Unicode properties, so a no-break space is whitespace and a C1 control is a control.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CacheKey(String);

impl CacheKey {
    /// Validates a key.
    ///
    /// # Errors
    ///
    /// [`CacheError::Refused`] naming the first rule the key failed.
    pub fn new(key: &str) -> Result<Self, CacheError> {
        if key.is_empty() {
            return Err(CacheError::Refused(Refusal::KeyEmpty));
        }
        if key.len() > MAX_KEY_BYTES {
            return Err(CacheError::Refused(Refusal::KeyTooLong));
        }
        // Checked per character, not per byte: the rules are Unicode properties. A byte-wise
        // `is_ascii_whitespace` accepted a no-break space (U+00A0) and every other non-ASCII
        // blank, which is the same formatting hazard as `a b` with a different encoding.
        for character in key.chars() {
            if character.is_control() {
                return Err(CacheError::Refused(Refusal::KeyHasControlCharacter));
            }
            if character.is_whitespace() {
                return Err(CacheError::Refused(Refusal::KeyHasWhitespace));
            }
        }
        Ok(Self(key.to_owned()))
    }

    /// The key text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Renders the **length**, never the key: a key is routinely built from an identifier — an
/// account, a session — and a `Debug` that printed it would put that identifier into any
/// diagnostic that formats the key.
impl fmt::Debug for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CacheKey")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// A validated value: at most the configured ceiling.
#[derive(Clone, PartialEq, Eq)]
pub struct CacheValue(Vec<u8>);

impl CacheValue {
    /// Validates a value against `bounds`.
    ///
    /// # Errors
    ///
    /// [`CacheError::Refused`] with [`Refusal::ValueTooLarge`].
    pub fn within(bytes: impl Into<Vec<u8>>, bounds: &CacheBounds) -> Result<Self, CacheError> {
        let bytes = bytes.into();
        if bytes.len() > bounds.max_value_bytes() {
            return Err(CacheError::Refused(Refusal::ValueTooLarge));
        }
        Ok(Self(bytes))
    }

    /// The bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Takes the bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    /// How many bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the value is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Renders the **length**, never the bytes (FR-009).
impl fmt::Debug for CacheValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CacheValue")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// A validated lifetime: at least [`MIN_TTL`], at most the configured ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ttl(Duration);

impl Ttl {
    /// Validates a lifetime against `bounds`.
    ///
    /// # Errors
    ///
    /// [`CacheError::Refused`] with [`Refusal::TtlTooShort`] or [`Refusal::TtlTooLong`].
    pub fn within(duration: Duration, bounds: &CacheBounds) -> Result<Self, CacheError> {
        if duration < MIN_TTL {
            return Err(CacheError::Refused(Refusal::TtlTooShort));
        }
        if duration > bounds.max_ttl() {
            return Err(CacheError::Refused(Refusal::TtlTooLong));
        }
        Ok(Self(duration))
    }

    /// The lifetime.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }

    /// The lifetime in whole seconds, rounded **up**, which is the unit a RESP server takes.
    #[must_use]
    pub fn whole_seconds(self) -> u64 {
        let secs = self.0.as_secs();
        if self.0.subsec_nanos() > 0 {
            secs.saturating_add(1)
        } else {
            secs
        }
    }
}

/// What `set_if_absent` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stored {
    /// The key was absent and is now set.
    Stored,
    /// Something else holds the key; nothing was written.
    AlreadyPresent,
}

/// What `delete` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Deleted {
    /// The key existed and is gone.
    Removed,
    /// The key was not there.
    Absent,
}

/// The cache counters (FR-083): `renvor_cache_hits_total{backend}`,
/// `renvor_cache_misses_total{backend}`, and `renvor_cache_errors_total{backend, category}`.
#[derive(Clone, Debug)]
pub struct CacheMetrics {
    hits: renvor_core::observe::metrics::Counter,
    misses: renvor_core::observe::metrics::Counter,
    errors: renvor_core::observe::metrics::Counter,
}

impl CacheMetrics {
    /// Registers the three families, or returns the existing ones.
    ///
    /// # Errors
    ///
    /// [`renvor_core::observe::metrics::MetricsError`] when a family of the same name is
    /// registered with another shape.
    pub fn register(
        registry: &renvor_core::observe::metrics::Registry,
    ) -> Result<Self, renvor_core::observe::metrics::MetricsError> {
        Ok(Self {
            hits: registry.counter(
                "renvor_cache_hits_total",
                "Reads that found a value.",
                &["backend"],
            )?,
            misses: registry.counter(
                "renvor_cache_misses_total",
                "Reads that found nothing.",
                &["backend"],
            )?,
            errors: registry.counter(
                "renvor_cache_errors_total",
                "Operations that failed, by closed category.",
                &["backend", "category"],
            )?,
        })
    }

    /// Counts a read that found a value.
    pub fn hit(&self, backend: &str) {
        self.hits.increment(&[("backend", backend)], 1);
    }

    /// Counts a read that found nothing.
    pub fn miss(&self, backend: &str) {
        self.misses.increment(&[("backend", backend)], 1);
    }

    /// Counts a failed operation by its closed category.
    pub fn error(&self, backend: &str, error: CacheError) {
        self.errors
            .increment(&[("backend", backend), ("category", error.as_str())], 1);
    }
}

/// The cache port.
///
/// `Send + Sync` because a cache is shared across tasks. Native `async fn` in the trait, which
/// makes it generic-only (not `dyn`); providers are generic over the implementation, as the
/// persistence ports are.
pub trait Cache: Send + Sync {
    /// Reads `key`. `Ok(None)` is a miss or an expired entry; `Err` is a failure.
    fn get(
        &self,
        key: &CacheKey,
    ) -> impl Future<Output = Result<Option<CacheValue>, CacheError>> + Send;

    /// Writes `value` under `key` for `ttl`, replacing whatever was there.
    fn set(
        &self,
        key: &CacheKey,
        value: CacheValue,
        ttl: Ttl,
    ) -> impl Future<Output = Result<(), CacheError>> + Send;

    /// Writes `value` under `key` for `ttl` **only if** the key is absent.
    ///
    /// The single-writer primitive: concurrent callers observe exactly one [`Stored::Stored`].
    fn set_if_absent(
        &self,
        key: &CacheKey,
        value: CacheValue,
        ttl: Ttl,
    ) -> impl Future<Output = Result<Stored, CacheError>> + Send;

    /// Removes `key`.
    fn delete(&self, key: &CacheKey) -> impl Future<Output = Result<Deleted, CacheError>> + Send;
}

/// A shared cache is itself a cache, so one instance can be held by a provider and a service.
impl<T> Cache for Arc<T>
where
    T: Cache + ?Sized,
{
    fn get(
        &self,
        key: &CacheKey,
    ) -> impl Future<Output = Result<Option<CacheValue>, CacheError>> + Send {
        (**self).get(key)
    }

    fn set(
        &self,
        key: &CacheKey,
        value: CacheValue,
        ttl: Ttl,
    ) -> impl Future<Output = Result<(), CacheError>> + Send {
        (**self).set(key, value, ttl)
    }

    fn set_if_absent(
        &self,
        key: &CacheKey,
        value: CacheValue,
        ttl: Ttl,
    ) -> impl Future<Output = Result<Stored, CacheError>> + Send {
        (**self).set_if_absent(key, value, ttl)
    }

    fn delete(&self, key: &CacheKey) -> impl Future<Output = Result<Deleted, CacheError>> + Send {
        (**self).delete(key)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CacheBounds, CacheError, CacheKey, CacheValue, MAX_KEY_BYTES, MAX_TTL_CAP,
        MAX_VALUE_BYTES_CAP, MIN_TTL, Namespace, OPERATION_TIMEOUT_CAP, Refusal, Ttl,
    };
    use std::time::Duration;

    #[test]
    fn every_key_rule_is_refused_by_name() {
        let cases: [(&str, Refusal); 5] = [
            ("", Refusal::KeyEmpty),
            (&"k".repeat(MAX_KEY_BYTES + 1), Refusal::KeyTooLong),
            ("a\u{1}b", Refusal::KeyHasControlCharacter),
            ("a\u{7f}b", Refusal::KeyHasControlCharacter),
            ("a b", Refusal::KeyHasWhitespace),
        ];
        for (input, expected) in cases {
            assert_eq!(
                CacheKey::new(input).unwrap_err(),
                CacheError::Refused(expected),
                "case {expected:?}"
            );
        }
        // POSITIVE CONTROL: the boundary value and ordinary punctuation are accepted.
        assert!(CacheKey::new(&"k".repeat(MAX_KEY_BYTES)).is_ok());
        assert!(CacheKey::new("user:42/profile.v1").is_ok());
        // Unicode is fine: the rules are about control characters and whitespace, not ASCII.
        assert!(CacheKey::new("clé").is_ok());
    }

    #[test]
    fn unicode_whitespace_is_refused_and_unicode_letters_are_not() {
        // The whitespace rule is about whitespace, not about ASCII: a key holding a no-break
        // space (U+00A0) or an ideographic space (U+3000) is as much a formatting hazard in a
        // log line or a key list as `a b` is, and a byte-wise `is_ascii_whitespace` never sees
        // either. `char::is_whitespace` is the Unicode White_Space property.
        let cases: [(&str, Refusal); 5] = [
            ("a\u{a0}b", Refusal::KeyHasWhitespace),
            ("a\u{2003}b", Refusal::KeyHasWhitespace),
            ("a\u{3000}b", Refusal::KeyHasWhitespace),
            ("a\u{2028}b", Refusal::KeyHasWhitespace),
            // C1 controls are controls: U+0085 (NEL) is both, and the control rule is checked
            // first, as it is for the C0 range.
            ("a\u{85}b", Refusal::KeyHasControlCharacter),
        ];
        for (input, expected) in cases {
            assert_eq!(
                CacheKey::new(input).unwrap_err(),
                CacheError::Refused(expected),
                "case {expected:?}"
            );
        }
        // POSITIVE CONTROL: non-ASCII letters and symbols are keys.
        for (index, accepted) in ["clé", "日本語", "ключ:1", "emoji-🔑"]
            .into_iter()
            .enumerate()
        {
            assert!(
                CacheKey::new(accepted).is_ok(),
                "accepted-key case {index} was refused"
            );
        }
    }

    #[test]
    fn the_namespace_counts_toward_the_key_bound() {
        let namespace = Namespace::new("shop").unwrap();
        let just_fits = CacheKey::new(&"k".repeat(MAX_KEY_BYTES - 5)).unwrap();
        assert_eq!(
            namespace.qualify(&just_fits).unwrap().len(),
            MAX_KEY_BYTES,
            "shop: plus the key is exactly the bound"
        );
        let one_over = CacheKey::new(&"k".repeat(MAX_KEY_BYTES - 4)).unwrap();
        assert_eq!(
            namespace.qualify(&one_over).unwrap_err(),
            CacheError::Refused(Refusal::KeyTooLong)
        );
        assert_eq!(
            namespace.qualify(&CacheKey::new("x").unwrap()).unwrap(),
            "shop:x"
        );
    }

    #[test]
    fn namespaces_are_lowercase_identifiers() {
        for (index, bad) in ["", "Shop", "sh op", "a:b", &"n".repeat(65)]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                Namespace::new(bad).unwrap_err(),
                CacheError::Refused(Refusal::NamespaceInvalid),
                "rejected namespace case {index} was accepted"
            );
        }
        assert!(Namespace::new("shop.v2_eu-west").is_ok());
    }

    #[test]
    fn bounds_have_defaults_and_hard_caps() {
        let bounds = CacheBounds::new();
        assert_eq!(bounds.max_value_bytes(), 1024 * 1024);
        assert_eq!(bounds.max_ttl(), Duration::from_secs(7 * 24 * 60 * 60));
        assert_eq!(bounds.operation_timeout(), Duration::from_secs(2));
        assert!(bounds.with_max_value_bytes(MAX_VALUE_BYTES_CAP).is_ok());
        assert!(
            bounds
                .with_max_value_bytes(MAX_VALUE_BYTES_CAP + 1)
                .is_err()
        );
        assert!(bounds.with_max_value_bytes(0).is_err());
        assert!(bounds.with_max_ttl(MAX_TTL_CAP).is_ok());
        assert!(bounds.with_max_ttl(MAX_TTL_CAP + MIN_TTL).is_err());
        assert!(bounds.with_max_ttl(Duration::from_millis(999)).is_err());
        assert!(bounds.with_operation_timeout(OPERATION_TIMEOUT_CAP).is_ok());
        assert!(
            bounds
                .with_operation_timeout(OPERATION_TIMEOUT_CAP + MIN_TTL)
                .is_err()
        );
        assert!(bounds.with_operation_timeout(Duration::ZERO).is_err());
    }

    #[test]
    fn values_and_ttls_are_checked_against_the_configured_bounds() {
        let bounds = CacheBounds::new().with_max_value_bytes(8).unwrap();
        assert!(CacheValue::within(vec![0; 8], &bounds).is_ok());
        assert_eq!(
            CacheValue::within(vec![0; 9], &bounds).unwrap_err(),
            CacheError::Refused(Refusal::ValueTooLarge)
        );
        let bounds = CacheBounds::new()
            .with_max_ttl(Duration::from_secs(10))
            .unwrap();
        assert_eq!(
            Ttl::within(Duration::from_millis(500), &bounds).unwrap_err(),
            CacheError::Refused(Refusal::TtlTooShort)
        );
        assert_eq!(
            Ttl::within(Duration::from_secs(11), &bounds).unwrap_err(),
            CacheError::Refused(Refusal::TtlTooLong)
        );
        let ttl = Ttl::within(Duration::from_millis(1_500), &bounds).unwrap();
        assert_eq!(
            ttl.whole_seconds(),
            2,
            "a RESP server takes whole seconds; round up"
        );
        assert_eq!(
            Ttl::within(Duration::from_secs(3), &bounds)
                .unwrap()
                .whole_seconds(),
            3
        );
    }

    #[test]
    fn debug_shows_lengths_and_never_content() {
        let key = CacheKey::new("session:hunter2CanaryDoNotLeak").unwrap();
        let value =
            CacheValue::within(b"hunter2CanaryDoNotLeak".to_vec(), &CacheBounds::new()).unwrap();
        let rendered = format!("{key:?} {value:?}");
        assert!(
            !rendered.contains("hunter2"),
            "a key or value leaked through Debug"
        );
        // POSITIVE CONTROL: the lengths are there, so the redaction is targeted.
        assert!(
            rendered.contains("bytes: 30"),
            "Debug did not report the key length"
        );
        assert!(
            rendered.contains("bytes: 22"),
            "Debug did not report the value length"
        );
    }
}
