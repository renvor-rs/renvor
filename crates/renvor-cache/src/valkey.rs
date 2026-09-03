//! The Valkey adapter: RESP over `redis` 1.6.0 with native roots, bounded reconnection, and no
//! fallback (ADR-0033).
//!
//! # The credential travels once, into the client, and nowhere else
//!
//! The URL in `ValkeySettings` is a `renvor_config::Secret`; it is exposed exactly once, to build the client, and
//! no `Debug`, `Display`, error, or event in this module renders it. The redis crate's own error
//! type can carry the server's reply text and the address it tried, so every `RedisError` is
//! mapped to a closed [`CacheError`] or [`CacheBootError`] **by category** and dropped.
//!
//! # Reconnection is bounded and configured, and an operation during it fails loudly
//!
//! `ConnectionManager` re-establishes a dropped connection with an exponential backoff whose
//! retry count, delay floor and ceiling, exponent, and timeouts are all set here from
//! `ReconnectBounds`. An operation attempted while the connection is down returns
//! [`CacheError::Unavailable`] — it does not queue, and there is no in-memory stand-in (FR-021).
//!
//! # The process-level crypto provider
//!
//! `redis` builds its TLS configuration with `rustls::ClientConfig::builder()`, which uses the
//! process-level provider, installing one from crate features when exactly one is enabled. This
//! workspace enables exactly one (`ring`), and `xtask` step 7 asserts it stays that way. An
//! application whose own dependencies add a second provider must call
//! `rustls::crypto::CryptoProvider::install_default` in `main` before Boot, or the TLS path
//! panics inside the client — stated in the capabilities contract rather than discovered.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use renvor_config::Secret;
use renvor_core::error::BoxedCause;
use renvor_core::health::{Readiness, ReadinessContributor};
use renvor_core::provider::ProviderId;
use renvor_core::provider::registry::{CapabilityId, InitContext, Provider, ProviderFuture};

use crate::port::{
    Cache, CacheBounds, CacheError, CacheKey, CacheValue, Deleted, Namespace, Refusal, Stored, Ttl,
};
use crate::provider::{CacheBootError, CacheReadiness, cache_capability};

/// The most reconnection attempts the manager may make before giving up on a dropped connection.
pub const MAX_RECONNECT_ATTEMPTS: usize = 100;
/// The longest reconnection delay.
pub const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(60);
/// The longest connection timeout.
pub const MAX_CONNECT_TIMEOUT: Duration = Duration::from_secs(60);

/// The reconnection bounds handed to the connection manager, each with a default and a cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconnectBounds {
    attempts: usize,
    min_delay: Duration,
    max_delay: Duration,
    connect_timeout: Duration,
}

impl Default for ReconnectBounds {
    fn default() -> Self {
        Self {
            attempts: 6,
            min_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(5),
        }
    }
}

impl ReconnectBounds {
    /// The documented defaults: 6 attempts, 100 ms → 5 s, 5 s connect timeout.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces every bound, refusing any outside its cap.
    ///
    /// # Errors
    ///
    /// [`CacheError::Refused`] with [`Refusal::BoundOutOfRange`].
    pub fn with(
        attempts: usize,
        min_delay: Duration,
        max_delay: Duration,
        connect_timeout: Duration,
    ) -> Result<Self, CacheError> {
        let valid = (1..=MAX_RECONNECT_ATTEMPTS).contains(&attempts)
            && !min_delay.is_zero()
            && min_delay <= max_delay
            && max_delay <= MAX_RECONNECT_DELAY
            && !connect_timeout.is_zero()
            && connect_timeout <= MAX_CONNECT_TIMEOUT;
        if !valid {
            return Err(CacheError::Refused(Refusal::BoundOutOfRange));
        }
        Ok(Self {
            attempts,
            min_delay,
            max_delay,
            connect_timeout,
        })
    }

    /// The connection timeout, which Boot also uses as its outer bound.
    #[must_use]
    pub const fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }
}

/// Everything the adapter needs. The URL is the only secret and is wrapped.
#[derive(Debug)]
pub struct ValkeySettings {
    url: Secret<String>,
    namespace: Namespace,
    bounds: CacheBounds,
    reconnect: ReconnectBounds,
}

impl ValkeySettings {
    /// Builds settings. `url` is a `redis://` or `rediss://` URL carrying the credential; it is
    /// a [`Secret`] so that it can be neither printed nor serialised.
    #[must_use]
    pub fn new(url: Secret<String>, namespace: Namespace, bounds: CacheBounds) -> Self {
        Self {
            url,
            namespace,
            bounds,
            reconnect: ReconnectBounds::default(),
        }
    }

    /// Replaces the reconnection bounds.
    #[must_use]
    pub const fn with_reconnect(mut self, reconnect: ReconnectBounds) -> Self {
        self.reconnect = reconnect;
        self
    }

    /// The operation bounds.
    #[must_use]
    pub const fn bounds(&self) -> &CacheBounds {
        &self.bounds
    }
}

/// A connected Valkey cache.
pub struct ValkeyCache {
    manager: ConnectionManager,
    namespace: Namespace,
    bounds: CacheBounds,
}

impl core::fmt::Debug for ValkeyCache {
    /// Namespace and bounds only. The manager holds the address and the credential.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ValkeyCache")
            .field("namespace", &self.namespace)
            .field("bounds", &self.bounds)
            .finish_non_exhaustive()
    }
}

impl ValkeyCache {
    /// Connects, proves the server answers an authenticated `PING`, and returns the cache.
    ///
    /// # Two connections at Boot, deliberately
    ///
    /// The first is a **single, bounded probe** with no retry: the connection manager retries its
    /// initial connect under its reconnection policy, which turns a refused credential into a
    /// timeout and reports the wrong category. The probe classifies the failure exactly —
    /// refused credential, unreachable, unanswered — and only then is the managed connection
    /// opened, against a server that has just answered.
    ///
    /// Every step is bounded: each connection by `reconnect.connect_timeout`, the ping by the
    /// operation timeout.
    ///
    /// # Errors
    ///
    /// A [`CacheBootError`] naming the phase and the category — never the address or the
    /// credential.
    pub async fn connect(settings: &ValkeySettings) -> Result<Self, CacheBootError> {
        let client = redis::Client::open(settings.url.expose().as_str())
            .map_err(|_| CacheBootError::InvalidAddress)?;

        // The probe: one attempt, one classification.
        let probe_config = redis::AsyncConnectionConfig::new()
            .set_connection_timeout(Some(settings.reconnect.connect_timeout))
            .set_response_timeout(Some(settings.bounds.operation_timeout()));
        let mut probe = match tokio::time::timeout(
            settings.reconnect.connect_timeout,
            client.get_multiplexed_async_connection_with_config(&probe_config),
        )
        .await
        {
            Ok(Ok(connection)) => connection,
            Ok(Err(error)) => return Err(boot_category(&error)),
            Err(_elapsed) => return Err(CacheBootError::Unreachable),
        };
        Self::ping(&mut probe, settings.bounds.operation_timeout()).await?;
        drop(probe);

        // The managed connection, with the bounded reconnection policy.
        let config = ConnectionManagerConfig::new()
            .set_number_of_retries(settings.reconnect.attempts)
            .set_min_delay(settings.reconnect.min_delay)
            .set_max_delay(settings.reconnect.max_delay)
            .set_connection_timeout(Some(settings.reconnect.connect_timeout))
            .set_response_timeout(Some(settings.bounds.operation_timeout()));
        let manager = match tokio::time::timeout(
            settings.reconnect.connect_timeout,
            ConnectionManager::new_with_config(client, config),
        )
        .await
        {
            Ok(Ok(manager)) => manager,
            Ok(Err(error)) => return Err(boot_category(&error)),
            Err(_elapsed) => return Err(CacheBootError::Unreachable),
        };
        Ok(Self {
            manager,
            namespace: settings.namespace.clone(),
            bounds: settings.bounds,
        })
    }

    /// The authenticated readiness probe: `PING` must answer `PONG` within `timeout`.
    async fn ping(
        connection: &mut impl redis::aio::ConnectionLike,
        timeout: Duration,
    ) -> Result<(), CacheBootError> {
        let answer: Result<String, redis::RedisError> =
            match tokio::time::timeout(timeout, redis::cmd("PING").query_async(connection)).await {
                Ok(answer) => answer,
                Err(_elapsed) => return Err(CacheBootError::Unanswered),
            };
        match answer {
            Ok(pong) if pong == "PONG" => Ok(()),
            Ok(_) => Err(CacheBootError::Unanswered),
            Err(error) => Err(boot_category(&error)),
        }
    }

    /// The bounds this cache validates against.
    #[must_use]
    pub const fn bounds(&self) -> &CacheBounds {
        &self.bounds
    }

    /// The namespace keys are stored under.
    #[must_use]
    pub const fn namespace(&self) -> &Namespace {
        &self.namespace
    }

    /// Runs one command under the operation timeout, mapping every failure to a closed category.
    async fn run<T: redis::FromRedisValue>(&self, command: redis::Cmd) -> Result<T, CacheError> {
        let mut connection = self.manager.clone();
        match tokio::time::timeout(
            self.bounds.operation_timeout(),
            command.query_async::<T>(&mut connection),
        )
        .await
        {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(operation_category(&error)),
            Err(_elapsed) => Err(CacheError::TimedOut),
        }
    }
}

/// Maps a driver error at Boot to a closed category. The error is **dropped**, never rendered.
fn boot_category(error: &redis::RedisError) -> CacheBootError {
    if error.kind() == redis::ErrorKind::AuthenticationFailed {
        CacheBootError::CredentialRefused
    } else if error.is_timeout() {
        CacheBootError::Unanswered
    } else {
        CacheBootError::Unreachable
    }
}

/// Maps a driver error during an operation to a closed category. The error is **dropped**.
fn operation_category(error: &redis::RedisError) -> CacheError {
    if error.is_timeout() {
        CacheError::TimedOut
    } else {
        CacheError::Unavailable
    }
}

impl Cache for ValkeyCache {
    async fn get(&self, key: &CacheKey) -> Result<Option<CacheValue>, CacheError> {
        let qualified = self.namespace.qualify(key)?;
        let bytes: Option<Vec<u8>> = self.run(redis::cmd("GET").arg(qualified).clone()).await?;
        match bytes {
            // A value the server returned that exceeds this process's bound is refused rather
            // than handed over: the bound is on what this application holds in memory, and
            // another writer with a larger bound must not raise it.
            Some(bytes) => Ok(Some(CacheValue::within(bytes, &self.bounds)?)),
            None => Ok(None),
        }
    }

    async fn set(&self, key: &CacheKey, value: CacheValue, ttl: Ttl) -> Result<(), CacheError> {
        let qualified = self.namespace.qualify(key)?;
        let _: String = self
            .run(
                redis::cmd("SET")
                    .arg(qualified)
                    .arg(value.as_bytes())
                    .arg("EX")
                    .arg(ttl.whole_seconds())
                    .clone(),
            )
            .await?;
        Ok(())
    }

    async fn set_if_absent(
        &self,
        key: &CacheKey,
        value: CacheValue,
        ttl: Ttl,
    ) -> Result<Stored, CacheError> {
        let qualified = self.namespace.qualify(key)?;
        // `SET … EX … NX` answers `OK` when it stored and nil when the key existed. One command,
        // so the check and the write are one server-side step — the single-writer guarantee.
        let answer: Option<String> = self
            .run(
                redis::cmd("SET")
                    .arg(qualified)
                    .arg(value.as_bytes())
                    .arg("EX")
                    .arg(ttl.whole_seconds())
                    .arg("NX")
                    .clone(),
            )
            .await?;
        Ok(match answer {
            Some(_) => Stored::Stored,
            None => Stored::AlreadyPresent,
        })
    }

    async fn delete(&self, key: &CacheKey) -> Result<Deleted, CacheError> {
        let qualified = self.namespace.qualify(key)?;
        let removed: i64 = self.run(redis::cmd("DEL").arg(qualified).clone()).await?;
        Ok(if removed > 0 {
            Deleted::Removed
        } else {
            Deleted::Absent
        })
    }
}

/// The Valkey cache as a kernel provider: connects at Boot, fails startup if the server does not
/// answer, publishes an `Arc<ValkeyCache>`, and contributes readiness.
pub struct ValkeyProvider {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    settings: ValkeySettings,
    cache: OnceLock<Arc<ValkeyCache>>,
    ready: Arc<AtomicBool>,
}

impl core::fmt::Debug for ValkeyProvider {
    /// Identity and readiness. Not the settings, whose `Debug` would be safe — the URL is a
    /// `Secret` — but the fewer paths a provider offers to its credential the fewer to audit.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ValkeyProvider")
            .field("id", &self.id)
            .field("booted", &self.cache.get().is_some())
            .finish()
    }
}

impl ValkeyProvider {
    /// Declares the provider. Nothing is connected until Boot.
    #[must_use]
    pub fn new(id: ProviderId, settings: ValkeySettings) -> Self {
        Self {
            id,
            provides: vec![cache_capability()],
            settings,
            cache: OnceLock::new(),
            ready: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The connected cache, or `None` before Boot has reached this provider.
    #[must_use]
    pub fn cache(&self) -> Option<Arc<ValkeyCache>> {
        self.cache.get().cloned()
    }
}

impl Provider for ValkeyProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn provides(&self) -> &[CapabilityId] {
        &self.provides
    }

    fn initialise<'a>(&'a self, context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
        Box::pin(async move {
            let cache = ValkeyCache::connect(&self.settings)
                .await
                .map(Arc::new)
                .map_err(|error| Box::new(error) as BoxedCause)?;
            context
                .register_state(Arc::clone(&cache))
                .map_err(|error| Box::new(error) as BoxedCause)?;
            context.register_readiness(Arc::new(CacheReadiness::new(
                &self.id,
                Arc::clone(&self.ready),
            )));
            let _ = self.cache.set(cache);
            self.ready.store(true, Ordering::Release);
            tracing::info!(
                target: "renvor.cache",
                provider = %self.id,
                adapter = "valkey",
                "cache provider booted"
            );
            Ok(())
        })
    }

    fn stop(&self) -> ProviderFuture<'_> {
        Box::pin(async move {
            self.ready.store(false, Ordering::Release);
            // The manager drops its socket with the last clone; there is no explicit close in the
            // client API, and holding the state `Arc` is the application's choice.
            tracing::info!(
                target: "renvor.cache",
                provider = %self.id,
                adapter = "valkey",
                "cache provider stopped"
            );
            Ok(())
        })
    }
}

impl ReadinessContributor for ValkeyProvider {
    fn name(&self) -> &str {
        self.id.as_str()
    }

    /// Ready once Boot's `PING` answered and the provider has not stopped. **Not** a continuous
    /// probe, for the reason the persistence providers give: the trait is synchronous and a round
    /// trip is not, and a background prober is the unbounded orphan the kernel excludes.
    fn readiness(&self) -> Readiness {
        if self.ready.load(Ordering::Acquire) {
            Readiness::Ready
        } else {
            Readiness::NotReady
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_CONNECT_TIMEOUT, MAX_RECONNECT_ATTEMPTS, MAX_RECONNECT_DELAY, ReconnectBounds,
        ValkeySettings,
    };
    use crate::port::{CacheBounds, CacheError, Namespace, Refusal};
    use renvor_config::Secret;
    use std::time::Duration;

    #[test]
    fn reconnect_bounds_are_capped() {
        let ok = ReconnectBounds::with(
            MAX_RECONNECT_ATTEMPTS,
            Duration::from_millis(1),
            MAX_RECONNECT_DELAY,
            MAX_CONNECT_TIMEOUT,
        );
        assert!(ok.is_ok());
        for bad in [
            ReconnectBounds::with(
                0,
                Duration::from_millis(1),
                Duration::from_secs(1),
                Duration::from_secs(1),
            ),
            ReconnectBounds::with(
                MAX_RECONNECT_ATTEMPTS + 1,
                Duration::from_millis(1),
                Duration::from_secs(1),
                Duration::from_secs(1),
            ),
            ReconnectBounds::with(
                1,
                Duration::ZERO,
                Duration::from_secs(1),
                Duration::from_secs(1),
            ),
            ReconnectBounds::with(
                1,
                Duration::from_secs(2),
                Duration::from_secs(1),
                Duration::from_secs(1),
            ),
            ReconnectBounds::with(
                1,
                Duration::from_millis(1),
                MAX_RECONNECT_DELAY + Duration::from_secs(1),
                Duration::from_secs(1),
            ),
            ReconnectBounds::with(
                1,
                Duration::from_millis(1),
                Duration::from_secs(1),
                Duration::ZERO,
            ),
            ReconnectBounds::with(
                1,
                Duration::from_millis(1),
                Duration::from_secs(1),
                MAX_CONNECT_TIMEOUT + Duration::from_secs(1),
            ),
        ] {
            assert_eq!(
                bad.unwrap_err(),
                CacheError::Refused(Refusal::BoundOutOfRange)
            );
        }
    }

    #[test]
    fn settings_debug_never_renders_the_url() {
        let settings = ValkeySettings::new(
            Secret::new(
                "cache.url",
                "redis://:hunter2CanaryDoNotLeak@127.0.0.1:6379/0".to_owned(),
            ),
            Namespace::new("app").unwrap(),
            CacheBounds::new(),
        );
        let rendered = format!("{settings:?}");
        assert!(
            !rendered.contains("hunter2"),
            "the credential leaked through Debug"
        );
        assert!(
            !rendered.contains("127.0.0.1"),
            "the address leaked through Debug"
        );
        // POSITIVE CONTROL: the namespace is shown.
        assert!(rendered.contains("app"), "Debug did not show the namespace");
    }
}
