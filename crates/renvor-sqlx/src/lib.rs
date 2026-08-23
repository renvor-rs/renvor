//! The direct-SQLx adapter for Renvor's persistence ports.
//!
//! # Selecting a database
//!
//! Neither database is a default feature. `db-postgres` and `db-mysql` each resolve exactly one
//! driver, and selecting one resolves **none** of the other — asserted by `xtask` step 7 with a
//! positive control, not left to a comment.
//!
//! ```toml
//! renvor-sqlx = { version = "0.0.0", features = ["db-postgres"] }
//! ```
//!
//! # What this crate is allowed to do that `renvor-database` is not
//!
//! Everything driver-specific. SQL text, connection options, driver types, and `sqlx` itself live
//! here. An application service depends on [`renvor_database::Database`] and never names a type
//! from this crate — which is what makes the same contract suite executable against both rows of
//! the compatibility matrix.
//!
//! # Query construction
//!
//! Use `sqlx::query` and `sqlx::query_as` — the **function** forms. The checked `query!` macros are
//! deliberately not enabled: they require either a live database at compile time or a committed
//! offline cache, and a framework compiling against two backends would need two caches kept in
//! step.

pub mod error;
pub mod migrate;

use core::time::Duration;

use renvor_database::{
    ConnectionString, Database, DatabaseError, DatabaseErrorKind, DatabaseKind, PoolSettings,
    UnitOfWork,
};
use sqlx::pool::PoolOptions;

pub use error::classify_error;
pub use migrate::Migrations;

/// A bounded SQLx pool implementing [`renvor_database::Database`].
///
/// Generic over the driver so that PostgreSQL and MySQL share one implementation. Two copies of
/// this logic would be two places for the contract to drift.
#[derive(Debug)]
pub struct SqlxDatabase<DB: sqlx::Database> {
    pool: sqlx::Pool<DB>,
    kind: DatabaseKind,
    close_timeout: Duration,
}

impl<DB: sqlx::Database> SqlxDatabase<DB> {
    /// Connects, applying every bound in `settings`.
    ///
    /// # The connection is established here, not lazily
    ///
    /// `PoolOptions::connect` opens the first connection before returning, so a wrong password or
    /// an unreachable host is a **boot** failure rather than a surprise at the first request.
    /// Principle IV requires required dependencies to be validated before readiness, and a lazy
    /// pool would report ready having verified nothing.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::ConnectFailed`] when the connection could not be established, redacted
    /// of the URL, host, user, and password.
    pub async fn connect(
        dsn: &ConnectionString,
        settings: &PoolSettings,
        kind: DatabaseKind,
    ) -> Result<Self, DatabaseError> {
        let pool = PoolOptions::<DB>::new()
            .max_connections(settings.max_connections())
            .min_connections(settings.min_connections())
            .acquire_timeout(settings.acquire_timeout())
            .idle_timeout(Some(settings.idle_timeout()))
            .max_lifetime(Some(settings.max_lifetime()))
            .connect(dsn.expose())
            .await
            .map_err(|error| error::classify_error(&error))?;

        Ok(Self {
            pool,
            kind,
            close_timeout: settings.drain_timeout(),
        })
    }

    /// The underlying pool.
    ///
    /// # Why this is public
    ///
    /// Constitution principle I: *"Public APIs SHOULD make the correct path easy without preventing
    /// direct access to the underlying package when the boundary permits it."* A repository
    /// implementation in an adapter module needs a real `sqlx::Pool` to run a real query, and
    /// hiding it would force every application to reimplement this crate.
    ///
    /// It is on the **adapter** side of the boundary. An application service that reaches for it
    /// has crossed a line the dependency rule draws, and `xtask` step 7 is where that shows up.
    #[must_use]
    pub const fn pool(&self) -> &sqlx::Pool<DB> {
        &self.pool
    }

    /// Pool statistics, for tests and for observability.
    ///
    /// # Connection release is measured, not asserted
    ///
    /// SC-002 requires commit, drop, and cancellation each to return the connection to the pool.
    /// A test that merely called them would prove nothing; a test that reads this proves it.
    #[must_use]
    pub fn connections(&self) -> PoolStatus {
        PoolStatus {
            size: self.pool.size(),
            idle: self.pool.num_idle(),
        }
    }
}

/// A snapshot of pool occupancy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolStatus {
    size: u32,
    idle: usize,
}

impl PoolStatus {
    /// Connections the pool currently holds, idle or in use.
    #[must_use]
    pub const fn size(self) -> u32 {
        self.size
    }

    /// Connections available for a caller to take.
    #[must_use]
    pub const fn idle(self) -> usize {
        self.idle
    }

    /// Connections currently checked out.
    #[must_use]
    pub const fn in_use(self) -> usize {
        (self.size as usize).saturating_sub(self.idle)
    }
}

/// An open SQLx transaction.
///
/// # Dropping this does not synchronously roll back
///
/// `sqlx-core`'s `Drop for Transaction` calls `start_rollback`, which *queues* a rollback; the
/// flush happens inside a **spawned** task when the connection returns to the pool. So `Drop`
/// returns before the rollback reaches the database.
///
/// What that does and does not mean, stated rather than softened:
///
/// - **No uncommitted row is ever visible to another connection.** That holds from the first
///   statement, independently of the rollback, and it is the property Renvor asserts.
/// - **A test that drops a transaction and immediately inspects server-side transaction state may
///   race.** Only [`UnitOfWork::rollback`] is synchronous.
#[derive(Debug)]
pub struct SqlxUnitOfWork<'c, DB: sqlx::Database> {
    inner: sqlx::Transaction<'c, DB>,
}

impl<'c, DB: sqlx::Database> SqlxUnitOfWork<'c, DB> {
    /// The underlying transaction, for a repository implementation to execute against.
    ///
    /// On the adapter side of the boundary, for the reason [`SqlxDatabase::pool`] gives.
    pub fn inner(&mut self) -> &mut sqlx::Transaction<'c, DB> {
        &mut self.inner
    }
}

impl<DB: sqlx::Database> UnitOfWork for SqlxUnitOfWork<'_, DB> {
    async fn commit(self) -> Result<(), DatabaseError> {
        self.inner.commit().await.map_err(|error| {
            let _ = error::classify_error(&error);
            DatabaseError::new(DatabaseErrorKind::CommitFailed)
        })
    }

    async fn rollback(self) -> Result<(), DatabaseError> {
        self.inner.rollback().await.map_err(|error| {
            let _ = error::classify_error(&error);
            DatabaseError::new(DatabaseErrorKind::RollbackFailed)
        })
    }
}

/// The readiness statement.
///
/// A constant with no interpolation and no bound parameters, and the smallest round-trip both
/// databases accept. It is a constant rather than a parameter so that no caller can make readiness
/// run something else.
const READINESS_PROBE: &str = "SELECT 1";

impl<DB: sqlx::Database> Database for SqlxDatabase<DB>
where
    for<'p> &'p sqlx::Pool<DB>: sqlx::Executor<'p, Database = DB>,
    <DB as sqlx::Database>::Arguments: sqlx::IntoArguments<DB>,
{
    type UnitOfWork<'c>
        = SqlxUnitOfWork<'c, DB>
    where
        Self: 'c;

    fn kind(&self) -> DatabaseKind {
        self.kind
    }

    async fn begin(&self) -> Result<Self::UnitOfWork<'_>, DatabaseError> {
        let inner = self
            .pool
            .begin()
            .await
            .map_err(|error| error::classify_error(&error))?;
        Ok(SqlxUnitOfWork { inner })
    }

    async fn check(&self) -> Result<(), DatabaseError> {
        // A REAL ROUND TRIP. Reporting ready without one would let readiness precede the database's
        // ability to serve, which principle IV forbids. Acquiring first makes an exhausted pool
        // report as an acquire timeout rather than as an unready database — two different faults
        // with two different corrections.
        let _guard = self
            .pool
            .acquire()
            .await
            .map_err(|error| error::classify_error(&error))?;

        sqlx::query(READINESS_PROBE)
            .execute(&self.pool)
            .await
            .map_err(|error| {
                let _ = error::classify_error(&error);
                DatabaseError::new(DatabaseErrorKind::NotReady)
            })?;
        Ok(())
    }

    async fn close(&self) -> Result<(), DatabaseError> {
        // A FORCED CLOSE IS AN ERROR, NOT A SUCCESSFUL SHUTDOWN. Principle IV: "Timeouts and forced
        // termination are visible errors, never successful shutdowns."
        match tokio::time::timeout(self.close_timeout, self.pool.close()).await {
            Ok(()) => Ok(()),
            Err(_) => Err(DatabaseError::new(DatabaseErrorKind::DeadlineExceeded)),
        }
    }
}

/// A bounded PostgreSQL database.
#[cfg(feature = "db-postgres")]
pub type PostgresDatabase = SqlxDatabase<sqlx::Postgres>;

/// A bounded MySQL database.
#[cfg(feature = "db-mysql")]
pub type MySqlDatabase = SqlxDatabase<sqlx::MySql>;

/// Connects to PostgreSQL.
///
/// # Errors
///
/// [`DatabaseErrorKind::ConnectFailed`], redacted.
#[cfg(feature = "db-postgres")]
pub async fn connect_postgres(
    dsn: &ConnectionString,
    settings: &PoolSettings,
) -> Result<PostgresDatabase, DatabaseError> {
    SqlxDatabase::connect(dsn, settings, DatabaseKind::Postgres).await
}

/// Connects to MySQL.
///
/// # A note on authentication that is not obvious
///
/// This crate does **not** enable `sqlx/mysql-rsa`, because that feature resolves the `rsa` crate,
/// which carries RUSTSEC-2023-0071 with no patch available. Without RSA key exchange, MySQL's
/// `caching_sha2_password` cannot complete a **first** authentication over a plaintext channel.
/// Use TLS, or a user whose password is already in the server's cache.
///
/// # Errors
///
/// [`DatabaseErrorKind::ConnectFailed`], redacted.
#[cfg(feature = "db-mysql")]
pub async fn connect_mysql(
    dsn: &ConnectionString,
    settings: &PoolSettings,
) -> Result<MySqlDatabase, DatabaseError> {
    SqlxDatabase::connect(dsn, settings, DatabaseKind::MySql).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_status_reports_occupancy_without_underflowing() {
        let status = PoolStatus { size: 3, idle: 1 };
        assert_eq!(status.in_use(), 2);
        // An idle count above the size cannot arise, but saturating rather than panicking is the
        // behaviour a metrics path needs.
        let odd = PoolStatus { size: 1, idle: 5 };
        assert_eq!(odd.in_use(), 0);
    }
}
