//! Bounded connection settings and the redacted connection string.

use core::fmt;
use core::time::Duration;

use crate::error::{DatabaseError, DatabaseErrorKind};

/// A database connection string that cannot be printed.
///
/// # Why a newtype and not a `String`
///
/// A connection string routinely carries a password. Constitution principle VI requires secrets to
/// stay out of *"repositories, generated manifests, logs, telemetry, URLs, browser bundles, desktop
/// resources, examples, fixtures, or snapshots"* — which is every place a `String` reaches the
/// moment somebody writes `tracing::info!(?config)`.
///
/// `Debug` and `Display` are implemented **by hand** to print a fixed marker. Deriving `Debug`
/// would have printed the secret, so the derive is deliberately absent and this doc comment is why.
#[derive(Clone, PartialEq, Eq)]
pub struct ConnectionString(String);

/// What every redacted rendering prints.
const REDACTED: &str = "<redacted>";

impl ConnectionString {
    /// Wraps a connection string.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The underlying value, for the one caller that must open a connection with it.
    ///
    /// Named `expose` rather than `as_str` so that reaching for it is a visible decision at the
    /// call site. A reviewer greps one word to find every place the secret is unwrapped.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// The scheme, which is safe to report because it names a driver rather than a credential.
    ///
    /// Returns `None` when the value has no `://`, which is itself worth reporting as a
    /// configuration failure rather than guessing.
    #[must_use]
    pub fn scheme(&self) -> Option<&str> {
        self.0.split_once("://").map(|(scheme, _)| scheme)
    }
}

impl fmt::Debug for ConnectionString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ConnectionString(")?;
        f.write_str(REDACTED)?;
        f.write_str(")")
    }
}

impl fmt::Display for ConnectionString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

/// Bounded pool settings.
///
/// # Every bound is finite, and none of them is optional
///
/// Constitution principle VI requires *"work, bodies, uploads, queues, queries, pagination,
/// retries, and concurrency"* to be bounded. A pool with an unbounded acquire wait converts a
/// database outage into a hung process, which is the failure mode a readiness check exists to
/// avoid — so `Option<Duration>` is deliberately not the type here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoolSettings {
    max_connections: u32,
    min_connections: u32,
    acquire_timeout: Duration,
    connect_timeout: Duration,
    idle_timeout: Duration,
    max_lifetime: Duration,
    drain_timeout: Duration,
}

/// The largest pool this crate will construct.
///
/// A ceiling rather than a suggestion. A four-figure pool against one database is a
/// misconfiguration that presents as database-side connection exhaustion, and the corrective
/// message is far cheaper here than there.
pub const MAX_POOL_CONNECTIONS: u32 = 1024;

/// The longest bound this crate will accept for any timeout.
///
/// Ten minutes is longer than any healthy database operation and short enough that an operator who
/// typed a wrong unit finds out.
pub const MAX_TIMEOUT: Duration = Duration::from_secs(600);

impl Default for PoolSettings {
    /// Conservative, finite, and small enough to run on a laptop.
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 0,
            acquire_timeout: Duration::from_secs(10),
            connect_timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
            drain_timeout: Duration::from_secs(10),
        }
    }
}

impl PoolSettings {
    /// The maximum number of connections.
    #[must_use]
    pub const fn max_connections(&self) -> u32 {
        self.max_connections
    }

    /// The minimum number of connections kept open.
    #[must_use]
    pub const fn min_connections(&self) -> u32 {
        self.min_connections
    }

    /// How long a caller may wait for a connection from the pool.
    #[must_use]
    pub const fn acquire_timeout(&self) -> Duration {
        self.acquire_timeout
    }

    /// How long establishing a new connection may take.
    #[must_use]
    pub const fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    /// How long an idle connection is kept.
    #[must_use]
    pub const fn idle_timeout(&self) -> Duration {
        self.idle_timeout
    }

    /// The longest a connection lives before being replaced.
    #[must_use]
    pub const fn max_lifetime(&self) -> Duration {
        self.max_lifetime
    }

    /// How long shutdown waits for checked-out connections to come back.
    ///
    /// # This is not `acquire_timeout`, and conflating them is a real mistake
    ///
    /// `acquire_timeout` bounds how long a **caller** waits to be given a connection.
    /// `drain_timeout` bounds how long **shutdown** waits for connections to be given back. They
    /// answer different questions and a deployment routinely wants different numbers: a short
    /// acquire deadline keeps request latency bounded, while a longer drain deadline lets
    /// in-flight work finish.
    ///
    /// Exceeding it is an **error**, not a successful shutdown — constitution principle IV.
    #[must_use]
    pub const fn drain_timeout(&self) -> Duration {
        self.drain_timeout
    }

    /// Sets the maximum connection count.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::Unclassified`] when the value is zero or above
    /// [`MAX_POOL_CONNECTIONS`]. A zero-connection pool is a pool that can never serve a request,
    /// and refusing it at construction is cheaper than diagnosing it at the first acquire.
    pub fn with_max_connections(mut self, value: u32) -> Result<Self, DatabaseError> {
        if value == 0 || value > MAX_POOL_CONNECTIONS {
            return Err(DatabaseError::new(DatabaseErrorKind::Unclassified));
        }
        self.max_connections = value;
        if self.min_connections > value {
            self.min_connections = value;
        }
        Ok(self)
    }

    /// Sets the minimum connection count.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::Unclassified`] when the value exceeds the maximum. A minimum above the
    /// maximum is not a preference the pool can express, so it is refused rather than clamped —
    /// `contracts/collection-reads.md` establishes that Renvor refuses rather than clamps.
    pub fn with_min_connections(mut self, value: u32) -> Result<Self, DatabaseError> {
        if value > self.max_connections {
            return Err(DatabaseError::new(DatabaseErrorKind::Unclassified));
        }
        self.min_connections = value;
        Ok(self)
    }

    /// Sets the acquire deadline.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::Unclassified`] for zero or for a value above [`MAX_TIMEOUT`].
    pub fn with_acquire_timeout(mut self, value: Duration) -> Result<Self, DatabaseError> {
        self.acquire_timeout = checked(value)?;
        Ok(self)
    }

    /// Sets the connect deadline.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::Unclassified`] for zero or for a value above [`MAX_TIMEOUT`].
    pub fn with_connect_timeout(mut self, value: Duration) -> Result<Self, DatabaseError> {
        self.connect_timeout = checked(value)?;
        Ok(self)
    }

    /// Sets the idle timeout.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::Unclassified`] for zero or for a value above [`MAX_TIMEOUT`].
    pub fn with_idle_timeout(mut self, value: Duration) -> Result<Self, DatabaseError> {
        self.idle_timeout = checked(value)?;
        Ok(self)
    }

    /// Sets the maximum connection lifetime.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::Unclassified`] for zero or for a value above [`MAX_TIMEOUT`].
    pub fn with_max_lifetime(mut self, value: Duration) -> Result<Self, DatabaseError> {
        self.max_lifetime = checked(value)?;
        Ok(self)
    }

    /// Sets the shutdown drain deadline.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::Unclassified`] for zero or for a value above [`MAX_TIMEOUT`]. An
    /// unbounded drain would turn a stuck connection into a process that never exits.
    pub fn with_drain_timeout(mut self, value: Duration) -> Result<Self, DatabaseError> {
        self.drain_timeout = checked(value)?;
        Ok(self)
    }
}

/// Refuses a zero or over-long duration.
fn checked(value: Duration) -> Result<Duration, DatabaseError> {
    if value.is_zero() || value > MAX_TIMEOUT {
        return Err(DatabaseError::new(DatabaseErrorKind::Unclassified));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connection_string_never_prints_its_password() {
        let dsn = ConnectionString::new("postgres://app:hunter2@db.internal:5432/app");
        assert!(!format!("{dsn}").contains("hunter2"));
        assert!(!format!("{dsn:?}").contains("hunter2"));
        assert!(!format!("{dsn:?}").contains("db.internal"));
        assert_eq!(dsn.expose(), "postgres://app:hunter2@db.internal:5432/app");
    }

    #[test]
    fn the_scheme_is_reportable_because_it_names_a_driver_not_a_credential() {
        let dsn = ConnectionString::new("mysql://app:secret@host/app");
        assert_eq!(dsn.scheme(), Some("mysql"));
        assert_eq!(ConnectionString::new("nonsense").scheme(), None);
    }

    #[test]
    fn defaults_are_finite() {
        let settings = PoolSettings::default();
        assert!(!settings.acquire_timeout().is_zero());
        assert!(!settings.connect_timeout().is_zero());
        assert!(!settings.drain_timeout().is_zero());
        assert!(settings.max_connections() > 0);
    }

    #[test]
    fn the_drain_bound_is_independent_of_the_acquire_bound() {
        let settings = PoolSettings::default()
            .with_acquire_timeout(Duration::from_secs(2))
            .expect("ok")
            .with_drain_timeout(Duration::from_secs(30))
            .expect("ok");
        assert_eq!(settings.acquire_timeout(), Duration::from_secs(2));
        assert_eq!(settings.drain_timeout(), Duration::from_secs(30));
    }

    #[test]
    fn an_unbounded_drain_cannot_be_expressed() {
        assert!(
            PoolSettings::default()
                .with_drain_timeout(Duration::ZERO)
                .is_err()
        );
    }

    #[test]
    fn a_zero_pool_is_refused_rather_than_accepted_as_a_pool_that_serves_nothing() {
        assert!(PoolSettings::default().with_max_connections(0).is_err());
    }

    #[test]
    fn a_pool_above_the_ceiling_is_refused() {
        assert!(
            PoolSettings::default()
                .with_max_connections(MAX_POOL_CONNECTIONS + 1)
                .is_err()
        );
        assert!(
            PoolSettings::default()
                .with_max_connections(MAX_POOL_CONNECTIONS)
                .is_ok()
        );
    }

    #[test]
    fn a_minimum_above_the_maximum_is_refused_not_clamped() {
        let settings = PoolSettings::default().with_max_connections(4).expect("ok");
        assert!(settings.with_min_connections(5).is_err());
    }

    #[test]
    fn lowering_the_maximum_lowers_a_now_impossible_minimum() {
        let settings = PoolSettings::default()
            .with_max_connections(10)
            .expect("ok")
            .with_min_connections(8)
            .expect("ok")
            .with_max_connections(4)
            .expect("ok");
        assert_eq!(settings.max_connections(), 4);
        assert_eq!(settings.min_connections(), 4);
    }

    #[test]
    fn an_unbounded_wait_cannot_be_expressed() {
        assert!(
            PoolSettings::default()
                .with_acquire_timeout(Duration::ZERO)
                .is_err()
        );
        assert!(
            PoolSettings::default()
                .with_acquire_timeout(MAX_TIMEOUT + Duration::from_secs(1))
                .is_err()
        );
    }
}
