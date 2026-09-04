//! The Valkey adapter: RESP over `redis` 1.6.0 with native roots, bounded reconnection, and no
//! fallback (ADR-0033).
//!
//! # The credential is a field, never part of an address
//!
//! [`ValkeySettings`](crate::valkey::ValkeySettings) takes a
//! [`ValkeyEndpoint`](crate::valkey::ValkeyEndpoint) (host, port, database, TLS or not) and,
//! apart from it, an optional [`ValkeyCredentials`](crate::valkey::ValkeyCredentials) whose
//! password is a `renvor_config::Secret`. There is no URL form: constitution VI says a secret
//! enters no URL, and a `redis://:password@host` string is a URL with a secret in it whatever
//! type wraps it. The password is exposed exactly once, into the driver's connection settings,
//! and no `Debug`, `Display`, error, or event in this module renders it. The redis crate's own
//! error type can carry the server's reply text and the address it tried, so every `RedisError`
//! is mapped to a closed [`CacheError`] or [`CacheBootError`] **by category** and dropped.
//!
//! # TLS by default; plaintext is a double opt-in (C-C7)
//!
//! [`ValkeyEndpoint::tls`](crate::valkey::ValkeyEndpoint::tls) is the constructor an
//! application reaches for. A [`ValkeyEndpoint::plaintext`](crate::valkey::ValkeyEndpoint::plaintext)
//! endpoint is accepted only when the host is loopback **and**
//! [`ValkeySettings::with_allow_insecure_loopback`](crate::valkey::ValkeySettings::with_allow_insecure_loopback)
//! was set — both, so a development server on `127.0.0.1` works and a plaintext session to
//! anything else is refused at the settings boundary, before a socket exists, as
//! [`CacheBootError::PlaintextRefused`]. The same rule the SMTP adapter applies (FR-047),
//! applied here because the threat model names a plaintext RESP session to a non-loopback host
//! as the failure (TB-5).
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

use redis::IntoConnectionInfo as _;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use renvor_config::Secret;
use renvor_core::error::BoxedCause;
use renvor_core::health::{Readiness, ReadinessContributor};
use renvor_core::provider::ProviderId;
use renvor_core::provider::registry::{CapabilityId, InitContext, Provider, ProviderFuture};

use crate::port::{
    Cache, CacheBounds, CacheError, CacheKey, CacheMetrics, CacheValue, Deleted, Namespace,
    Refusal, Stored, Ttl,
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
        Self::new()
    }
}

impl ReconnectBounds {
    /// The documented defaults: 6 attempts, 100 ms → 5 s, 5 s connect timeout.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            attempts: 6,
            min_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(5),
        }
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

/// The most bytes a host may carry: a DNS name's limit.
pub const MAX_HOST_BYTES: usize = 253;
/// The most bytes a username may carry.
pub const MAX_USERNAME_BYTES: usize = 256;

/// `[a-z0-9.-]{1,253}` with no leading or trailing dot, or an IP literal.
fn valid_host(host: &str) -> bool {
    if host.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    let bytes = host.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_HOST_BYTES
        && !host.starts_with('.')
        && !host.ends_with('.')
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
}

/// True for `localhost`, `127.0.0.0/8`, and `::1`.
fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

/// Where the server is and whether the session is encrypted. **No credential lives here.**
#[derive(Clone, PartialEq, Eq)]
pub struct ValkeyEndpoint {
    host: String,
    port: u16,
    database: u8,
    tls: bool,
}

impl core::fmt::Debug for ValkeyEndpoint {
    /// Encryption and database only. The host and port are an operator's address.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ValkeyEndpoint")
            .field("tls", &self.tls)
            .field("database", &self.database)
            .finish_non_exhaustive()
    }
}

impl ValkeyEndpoint {
    fn new(host: &str, port: u16, tls: bool) -> Result<Self, CacheError> {
        if !valid_host(host) || port == 0 {
            return Err(CacheError::Refused(Refusal::EndpointInvalid));
        }
        Ok(Self {
            host: host.to_owned(),
            port,
            database: 0,
            tls,
        })
    }

    /// A TLS session to `host:port`, verified against the native root store. The default.
    ///
    /// # Errors
    ///
    /// [`CacheError::Refused`] with [`Refusal::EndpointInvalid`] for a host that is not a
    /// lowercase DNS name or an IP literal, or a port of zero.
    pub fn tls(host: &str, port: u16) -> Result<Self, CacheError> {
        Self::new(host, port, true)
    }

    /// A plaintext session to `host:port`. Accepted at Boot **only** when the host is loopback
    /// and the settings say [`ValkeySettings::with_allow_insecure_loopback`] (C-C7).
    ///
    /// # Errors
    ///
    /// As [`Self::tls`].
    pub fn plaintext(host: &str, port: u16) -> Result<Self, CacheError> {
        Self::new(host, port, false)
    }

    /// Selects a logical database (`SELECT`); `0` by default.
    #[must_use]
    pub const fn with_database(mut self, database: u8) -> Self {
        self.database = database;
        self
    }

    /// The host.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// The logical database.
    #[must_use]
    pub const fn database(&self) -> u8 {
        self.database
    }

    /// Whether the session is TLS.
    #[must_use]
    pub const fn is_tls(&self) -> bool {
        self.tls
    }

    /// Whether the host is a loopback address or `localhost`.
    #[must_use]
    pub fn is_loopback(&self) -> bool {
        is_loopback(&self.host)
    }

    fn address(&self) -> redis::ConnectionAddr {
        if self.tls {
            redis::ConnectionAddr::TcpTls {
                host: self.host.clone(),
                port: self.port,
                insecure: false,
                tls_params: None,
            }
        } else {
            redis::ConnectionAddr::Tcp(self.host.clone(), self.port)
        }
    }
}

/// What the client authenticates with: an optional ACL username and a password.
pub struct ValkeyCredentials {
    username: Option<String>,
    password: Secret<String>,
}

impl core::fmt::Debug for ValkeyCredentials {
    /// Whether a username is set. Never the username, never the password.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ValkeyCredentials")
            .field("username", &self.username.is_some())
            .finish_non_exhaustive()
    }
}

impl ValkeyCredentials {
    /// A password for the default user (`AUTH <password>`).
    #[must_use]
    pub const fn password(password: Secret<String>) -> Self {
        Self {
            username: None,
            password,
        }
    }

    /// An ACL username to authenticate as (`AUTH <username> <password>`).
    ///
    /// # Errors
    ///
    /// [`CacheError::Refused`] with [`Refusal::CredentialInvalid`] for an empty username, one over
    /// [`MAX_USERNAME_BYTES`], or one holding a control character or whitespace.
    pub fn with_username(mut self, username: &str) -> Result<Self, CacheError> {
        let valid = !username.is_empty()
            && username.len() <= MAX_USERNAME_BYTES
            && !username
                .chars()
                .any(|character| character.is_control() || character.is_whitespace());
        if !valid {
            return Err(CacheError::Refused(Refusal::CredentialInvalid));
        }
        self.username = Some(username.to_owned());
        Ok(self)
    }
}

/// Everything the adapter needs. The password inside the credentials is the only secret.
#[derive(Debug)]
pub struct ValkeySettings {
    endpoint: ValkeyEndpoint,
    credentials: Option<ValkeyCredentials>,
    namespace: Namespace,
    bounds: CacheBounds,
    reconnect: ReconnectBounds,
    allow_insecure_loopback: bool,
}

impl ValkeySettings {
    /// Builds settings. Nothing is connected and nothing is checked against the network here;
    /// [`Self::validate`] is the settings rule, and `connect` applies it first.
    #[must_use]
    pub const fn new(
        endpoint: ValkeyEndpoint,
        credentials: Option<ValkeyCredentials>,
        namespace: Namespace,
        bounds: CacheBounds,
    ) -> Self {
        Self {
            endpoint,
            credentials,
            namespace,
            bounds,
            reconnect: ReconnectBounds::new(),
            allow_insecure_loopback: false,
        }
    }

    /// Replaces the reconnection bounds.
    #[must_use]
    pub const fn with_reconnect(mut self, reconnect: ReconnectBounds) -> Self {
        self.reconnect = reconnect;
        self
    }

    /// Permits a plaintext session **to a loopback host only**. Off by default (C-C7).
    #[must_use]
    pub const fn with_allow_insecure_loopback(mut self, allow: bool) -> Self {
        self.allow_insecure_loopback = allow;
        self
    }

    /// The settings rule, applied before any socket: a plaintext endpoint is accepted only when
    /// its host is loopback **and** the opt-in was given.
    ///
    /// # Errors
    ///
    /// [`CacheBootError::PlaintextRefused`].
    pub fn validate(&self) -> Result<(), CacheBootError> {
        let plaintext_permitted = self.allow_insecure_loopback && self.endpoint.is_loopback();
        if !self.endpoint.tls && !plaintext_permitted {
            return Err(CacheBootError::PlaintextRefused);
        }
        Ok(())
    }

    /// The endpoint.
    #[must_use]
    pub const fn endpoint(&self) -> &ValkeyEndpoint {
        &self.endpoint
    }

    /// The operation bounds.
    #[must_use]
    pub const fn bounds(&self) -> &CacheBounds {
        &self.bounds
    }

    /// The driver's connection settings: the address, the database, and the credential exposed
    /// **once**, here.
    fn connection_info(&self) -> Result<redis::ConnectionInfo, CacheBootError> {
        let mut redis_settings =
            redis::RedisConnectionInfo::default().set_db(i64::from(self.endpoint.database));
        if let Some(credentials) = &self.credentials {
            if let Some(username) = &credentials.username {
                redis_settings = redis_settings.set_username(username);
            }
            redis_settings = redis_settings.set_password(credentials.password.expose());
        }
        self.endpoint
            .address()
            .into_connection_info()
            .map(|info| info.set_redis_settings(redis_settings))
            .map_err(|_| CacheBootError::InvalidAddress)
    }
}

/// A connected Valkey cache.
pub struct ValkeyCache {
    manager: ConnectionManager,
    namespace: Namespace,
    bounds: CacheBounds,
    metrics: Option<CacheMetrics>,
}

/// The backend label in metrics.
pub const BACKEND: &str = "valkey";

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
        // The settings rule first, before any socket: a plaintext session that the rule refuses
        // never reaches the transport probe below (C-C7).
        settings.validate()?;
        let client = redis::Client::open(settings.connection_info()?)
            .map_err(|_| CacheBootError::InvalidAddress)?;

        // The transport probe first: a bounded plain TCP connect to the address. The driver
        // reports its own connect bound and its response bound as the same timeout, so without
        // this a port that is slow to refuse would surface as `Unanswered` — Windows retries a
        // SYN to a closed loopback port for longer than the bound and did exactly that on the
        // pull-request platform legs. A refusal, a dropped SYN, or no route is `Unreachable`
        // everywhere; only an address that accepts goes on to the driver.
        reach(
            client.get_connection_info().addr(),
            settings.reconnect.connect_timeout,
        )
        .await?;

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
            metrics: None,
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
/// A bounded plain TCP connect to the address the driver would dial, classified as
/// `Unreachable` on refusal, on no route, or on the bound elapsing. A socket address is left to
/// the driver, as is any address form this probe does not know.
async fn reach(address: &redis::ConnectionAddr, bound: Duration) -> Result<(), CacheBootError> {
    let (host, port) = match address {
        redis::ConnectionAddr::Tcp(host, port) => (host.as_str(), *port),
        redis::ConnectionAddr::TcpTls { host, port, .. } => (host.as_str(), *port),
        _ => return Ok(()),
    };
    match tokio::time::timeout(bound, tokio::net::TcpStream::connect((host, port))).await {
        Ok(Ok(stream)) => {
            drop(stream);
            Ok(())
        }
        // A refusal, no route, or the bound elapsing: all `Unreachable`.
        Ok(Err(_)) | Err(_) => Err(CacheBootError::Unreachable),
    }
}

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

impl ValkeyCache {
    /// Counts a read as a hit, a miss, or an error.
    fn record_read(&self, outcome: &Result<Option<CacheValue>, CacheError>) {
        if let Some(metrics) = &self.metrics {
            match outcome {
                Ok(Some(_)) => metrics.hit(BACKEND),
                Ok(None) => metrics.miss(BACKEND),
                Err(error) => metrics.error(BACKEND, *error),
            }
        }
    }

    /// Counts a failed write or delete by its closed category.
    fn record<T>(&self, outcome: Result<T, CacheError>) -> Result<T, CacheError> {
        if let (Some(metrics), Err(error)) = (&self.metrics, &outcome) {
            metrics.error(BACKEND, *error);
        }
        outcome
    }

    /// Counts hits, misses, and errors in `metrics`.
    #[must_use]
    pub fn with_metrics(mut self, metrics: CacheMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    async fn raw_get(&self, key: &CacheKey) -> Result<Option<CacheValue>, CacheError> {
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

    async fn raw_set(&self, key: &CacheKey, value: CacheValue, ttl: Ttl) -> Result<(), CacheError> {
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

    async fn raw_set_if_absent(
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

    async fn raw_delete(&self, key: &CacheKey) -> Result<Deleted, CacheError> {
        let qualified = self.namespace.qualify(key)?;
        let removed: i64 = self.run(redis::cmd("DEL").arg(qualified).clone()).await?;
        Ok(if removed > 0 {
            Deleted::Removed
        } else {
            Deleted::Absent
        })
    }
}

impl Cache for ValkeyCache {
    async fn get(&self, key: &CacheKey) -> Result<Option<CacheValue>, CacheError> {
        let outcome = self.raw_get(key).await;
        self.record_read(&outcome);
        outcome
    }
    async fn set(&self, key: &CacheKey, value: CacheValue, ttl: Ttl) -> Result<(), CacheError> {
        self.record(self.raw_set(key, value, ttl).await)
    }
    async fn set_if_absent(
        &self,
        key: &CacheKey,
        value: CacheValue,
        ttl: Ttl,
    ) -> Result<Stored, CacheError> {
        self.record(self.raw_set_if_absent(key, value, ttl).await)
    }
    async fn delete(&self, key: &CacheKey) -> Result<Deleted, CacheError> {
        self.record(self.raw_delete(key).await)
    }
}

/// The Valkey cache as a kernel provider: connects at Boot, fails startup if the server does not
/// answer, publishes an `Arc<ValkeyCache>`, and contributes readiness.
pub struct ValkeyProvider {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    settings: SettingsSource,
    cache: OnceLock<Arc<ValkeyCache>>,
    ready: Arc<AtomicBool>,
}

/// Where the provider's settings come from: built in code, or read at Boot from the typed
/// section the kernel validated.
enum SettingsSource {
    Given(Box<ValkeySettings>),
    Configured(renvor_config::ConfigHandle<crate::config::CacheSection>),
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
            settings: SettingsSource::Given(Box::new(settings)),
            cache: OnceLock::new(),
            ready: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Declares a provider whose settings are the typed `[cache]` section behind `handle`
    /// (FR-011). The section was validated by the kernel before Boot; the handle is read here at
    /// Boot, which is the first moment the resolved value exists.
    #[must_use]
    pub fn from_config(
        id: ProviderId,
        handle: renvor_config::ConfigHandle<crate::config::CacheSection>,
    ) -> Self {
        Self {
            id,
            provides: vec![cache_capability()],
            settings: SettingsSource::Configured(handle),
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
            let configured;
            let settings = match &self.settings {
                SettingsSource::Given(settings) => settings.as_ref(),
                SettingsSource::Configured(handle) => {
                    configured = crate::config::settings_from_handle(handle)
                        .map_err(|error| Box::new(error) as BoxedCause)?;
                    &configured
                }
            };
            let cache = ValkeyCache::connect(settings)
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
        ValkeyCredentials, ValkeyEndpoint, ValkeySettings,
    };
    use crate::port::{CacheBounds, CacheError, Namespace, Refusal};
    use crate::provider::CacheBootError;
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

    fn settings(endpoint: ValkeyEndpoint) -> ValkeySettings {
        ValkeySettings::new(
            endpoint,
            Some(ValkeyCredentials::password(Secret::new(
                "cache.password",
                "hunter2CanaryDoNotLeak".to_owned(),
            ))),
            Namespace::new("app").unwrap(),
            CacheBounds::new(),
        )
    }

    #[test]
    fn a_plaintext_endpoint_to_a_non_loopback_host_is_refused_even_with_the_opt_in() {
        // C-C7: plaintext is a DOUBLE opt-in. The flag alone does not open a plaintext session to
        // a host that is not loopback — that would make one configuration line the difference
        // between an encrypted and a cleartext credential on the wire.
        let refused = settings(ValkeyEndpoint::plaintext("cache.internal", 6379).unwrap())
            .with_allow_insecure_loopback(true);
        assert_eq!(
            refused.validate().unwrap_err(),
            CacheBootError::PlaintextRefused
        );
        let refused = settings(ValkeyEndpoint::plaintext("192.0.2.10", 6379).unwrap())
            .with_allow_insecure_loopback(true);
        assert_eq!(
            refused.validate().unwrap_err(),
            CacheBootError::PlaintextRefused
        );
    }

    #[test]
    fn a_plaintext_endpoint_to_loopback_needs_the_opt_in() {
        for (index, host) in ["127.0.0.1", "localhost", "::1", "127.1.2.3"]
            .into_iter()
            .enumerate()
        {
            let without = settings(ValkeyEndpoint::plaintext(host, 6379).unwrap());
            assert_eq!(
                without.validate().unwrap_err(),
                CacheBootError::PlaintextRefused,
                "loopback case {index} was accepted without the opt-in"
            );
            // POSITIVE CONTROL: loopback AND the opt-in is the one accepted plaintext shape.
            let with = settings(ValkeyEndpoint::plaintext(host, 6379).unwrap())
                .with_allow_insecure_loopback(true);
            assert!(
                with.validate().is_ok(),
                "loopback case {index} was refused with the opt-in"
            );
        }
    }

    #[test]
    fn a_tls_endpoint_needs_no_opt_in_anywhere() {
        assert!(
            settings(ValkeyEndpoint::tls("cache.internal", 6379).unwrap())
                .validate()
                .is_ok()
        );
        assert!(
            settings(ValkeyEndpoint::tls("127.0.0.1", 6379).unwrap())
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn endpoints_and_credentials_are_validated_at_construction() {
        for (index, bad) in [
            ("", 6379),
            ("Cache.Internal", 6379),
            ("cache.internal", 0),
            ("cache internal", 6379),
            (".cache", 6379),
            ("cache\u{1}", 6379),
            ("user@cache.internal", 6379),
            (&"h".repeat(254), 6379),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                ValkeyEndpoint::tls(bad.0, bad.1).unwrap_err(),
                CacheError::Refused(Refusal::EndpointInvalid),
                "endpoint case {index} was accepted"
            );
        }
        assert!(ValkeyEndpoint::tls("cache-1.internal", 6379).is_ok());
        assert!(ValkeyEndpoint::tls("::1", 6379).is_ok());
        assert_eq!(
            ValkeyEndpoint::tls("h", 1)
                .unwrap()
                .with_database(3)
                .database(),
            3
        );
        let password = || Secret::new("cache.password", "x".to_owned());
        for (index, bad) in ["", "a b", "a\u{1}", &"u".repeat(257)]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                ValkeyCredentials::password(password())
                    .with_username(bad)
                    .unwrap_err(),
                CacheError::Refused(Refusal::CredentialInvalid),
                "username case {index} was accepted"
            );
        }
        assert!(
            ValkeyCredentials::password(password())
                .with_username("app-user")
                .is_ok()
        );
    }

    #[test]
    fn settings_debug_never_renders_the_credential_or_the_address() {
        let settings = settings(ValkeyEndpoint::tls("127.0.0.1", 6379).unwrap());
        let rendered = format!("{settings:?}");
        assert!(
            !rendered.contains("hunter2"),
            "the credential leaked through Debug"
        );
        assert!(
            !rendered.contains("127.0.0.1") && !rendered.contains("6379"),
            "the address leaked through Debug"
        );
        // POSITIVE CONTROL: the namespace and the encryption are shown.
        assert!(rendered.contains("app"), "Debug did not show the namespace");
        assert!(
            rendered.contains("tls: true"),
            "Debug did not show the encryption"
        );
    }
}
