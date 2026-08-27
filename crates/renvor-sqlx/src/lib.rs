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

pub mod auth;
pub mod error;
pub mod migrate;
pub mod page;
pub mod provider;
pub mod seed;

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
        let opening = PoolOptions::<DB>::new()
            .max_connections(settings.max_connections())
            .min_connections(settings.min_connections())
            .acquire_timeout(settings.acquire_timeout())
            .idle_timeout(Some(settings.idle_timeout()))
            .max_lifetime(Some(settings.max_lifetime()))
            .connect(dsn.expose());

        // THE CONNECT DEADLINE IS APPLIED HERE BECAUSE `sqlx` HAS NOWHERE TO PUT IT.
        //
        // `PoolOptions` exposes `acquire_timeout`, `idle_timeout` and `max_lifetime`, and no
        // separate bound on establishing a connection. So `PoolSettings::connect_timeout` was
        // accepted, validated against `MAX_TIMEOUT`, and then passed to nothing: an operator who
        // set two seconds expecting a fast boot failure against an unreachable host waited out
        // `acquire_timeout` instead. A bound that is accepted and ignored is the silent fallback
        // principle IV prohibits, and it is worse than not offering the setting.
        let pool = match tokio::time::timeout(settings.connect_timeout(), opening).await {
            Ok(Ok(pool)) => pool,
            Ok(Err(error)) => return Err(error::classify_connect_error(&error)),
            // `ConnectFailed`, not `DeadlineExceeded`: what happened is that a connection could not
            // be established inside its bound, and the operator's next step is the host and the
            // credentials rather than the deadline.
            Err(_) => return Err(DatabaseError::new(DatabaseErrorKind::ConnectFailed)),
        };

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

    /// Checks out a pooled connection for work that needs no transaction.
    ///
    /// # When to prefer [`Database::begin`]
    ///
    /// Whenever two statements must succeed or fail together. This method exists for the reads and
    /// single statements that genuinely do not need a transaction, so that a repository written
    /// against [`SqlxExecutor`] can serve both without a second implementation.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::AcquireTimeout`] when the pool is full, or
    /// [`DatabaseErrorKind::ConnectFailed`] when a new connection cannot be established.
    pub async fn acquire(&self) -> Result<SqlxConnection<DB>, DatabaseError> {
        let connection = self
            .pool
            .acquire()
            .await
            .map_err(|error| error::classify_error(&error))?;
        Ok(SqlxConnection {
            connection,
            kind: self.kind,
        })
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

/// An open transaction, owning its pooled connection for the whole of its life.
///
/// # Why this owns a `PoolConnection` instead of a `sqlx::Transaction`
///
/// Dropping a future does not cancel the statement it started. The client stops waiting; the
/// server keeps working. `sqlx-core`'s return path then calls `Connection::ping()` **with no
/// deadline**, while still holding the guard that keeps `Pool::size()` incremented:
///
/// ```text
/// sqlx-core-0.9.0/src/pool/connection.rs
///     // this is simply a band-aid as SQLx-next connections should be able
///     // to recover from cancellations
///     if let Err(error) = self.raw.ping().await {
/// ```
///
/// On MySQL that ping can block forever. `MySqlTransactionManager::start_rollback` pushes an extra
/// `Waiting::Result` and resets `stream.sequence_id = 0` mid-conversation, after which
/// `wait_until_ready()` loops on `recv_packet().await` for a packet the server has already sent.
/// The task never finishes, `size` never decrements, and because the pool still counts the lost
/// connection against `max_connections` it will not open a replacement. One slot is gone for the
/// life of the process. Measured at 6 occurrences in 12 trials on MySQL 9.7.2 and 2 in 12 on MySQL
/// 8.4.11. PostgreSQL survives it only because its protocol resynchronises at `ReadyForQuery` —
/// but even there a returned connection stays pinned for as long as the abandoned statement runs,
/// which measured at 9.6s for an abandoned `pg_sleep(10)`.
///
/// Owning the connection lets Renvor decide what happens on an abnormal end. See [`Drop`].
///
/// [`Drop`]: #impl-Drop-for-SqlxUnitOfWork<'c,+DB>
pub struct SqlxUnitOfWork<'c, DB: sqlx::Database> {
    /// `None` once [`UnitOfWork::commit`] or [`UnitOfWork::rollback`] has taken the connection.
    ///
    /// This is what lets `Drop` tell an abnormal end from a normal one: a connection still present
    /// here means neither method ran, which means the future was cancelled.
    connection: Option<sqlx::pool::PoolConnection<DB>>,
    kind: DatabaseKind,
    borrow: core::marker::PhantomData<&'c ()>,
}

impl<DB: sqlx::Database> core::fmt::Debug for SqlxUnitOfWork<'_, DB> {
    /// Prints whether the transaction is still open, and nothing about the connection.
    ///
    /// The same reasoning as [`renvor_database::ConnectionString`]: a derived `Debug` would print
    /// whatever the driver's own `Debug` prints, which is not a guarantee anyone controls.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SqlxUnitOfWork")
            .field("kind", &self.kind)
            .field("open", &self.connection.is_some())
            .finish()
    }
}

/// Opens a transaction. Both databases accept it unqualified.
const BEGIN: &str = "BEGIN";
/// Commits the open transaction.
const COMMIT: &str = "COMMIT";
/// Rolls back the open transaction.
const ROLLBACK: &str = "ROLLBACK";

impl<DB: sqlx::Database> SqlxUnitOfWork<'_, DB> {
    /// The underlying connection, for a repository implementation to execute against.
    ///
    /// On the adapter side of the boundary, for the reason [`SqlxDatabase::pool`] gives.
    ///
    /// # Panics
    ///
    /// Never in practice. [`UnitOfWork::commit`] and [`UnitOfWork::rollback`] both consume `self`,
    /// so no caller holds a value whose connection has been taken.
    pub fn inner(&mut self) -> &mut sqlx::pool::PoolConnection<DB> {
        self.connection
            .as_mut()
            .expect("BUG: the connection is taken only by `commit`/`rollback`, which consume self")
    }
}

impl<DB: sqlx::Database> UnitOfWork for SqlxUnitOfWork<'_, DB>
where
    for<'e> &'e mut DB::Connection: sqlx::Executor<'e, Database = DB>,
    <DB as sqlx::Database>::Arguments: sqlx::IntoArguments<DB>,
{
    async fn commit(mut self) -> Result<(), DatabaseError> {
        // Taking the connection marks this end as normal, so `Drop` will not discard it.
        let mut connection = self
            .connection
            .take()
            .expect("BUG: `commit` consumes self, so the connection is present");
        match sqlx::Executor::execute(&mut *connection, COMMIT).await {
            Ok(_) => Ok(()),
            Err(error) => {
                let _ = error::classify_error(&error);
                // A failed commit leaves the transaction's fate unknown, so the connection is
                // discarded rather than returned. `detach` frees the pool slot synchronously, so
                // a failing commit cannot cost capacity either.
                drop(connection.detach());
                Err(DatabaseError::new(DatabaseErrorKind::CommitFailed))
            }
        }
    }

    async fn rollback(mut self) -> Result<(), DatabaseError> {
        let mut connection = self
            .connection
            .take()
            .expect("BUG: `rollback` consumes self, so the connection is present");
        match sqlx::Executor::execute(&mut *connection, ROLLBACK).await {
            Ok(_) => Ok(()),
            Err(error) => {
                let _ = error::classify_error(&error);
                drop(connection.detach());
                Err(DatabaseError::new(DatabaseErrorKind::RollbackFailed))
            }
        }
    }
}

impl<'c, DB: sqlx::Database> Drop for SqlxUnitOfWork<'c, DB> {
    /// Discards the connection when the transaction ended without `commit` or `rollback`.
    ///
    /// # Why discard rather than return
    ///
    /// Reaching here means the future was dropped mid-flight. The connection's protocol state is
    /// therefore unknown, and handing it back to `sqlx-core` risks the unbounded `ping()` this
    /// type's documentation describes. [`sqlx::pool::PoolConnection::detach`] releases the pool's
    /// size guard **synchronously**, so the pool may open a replacement immediately; dropping the
    /// detached connection closes its socket, and both databases roll back an open transaction
    /// when the client disconnects.
    ///
    /// This runs only on the abnormal path. `commit` and `rollback` take the connection first, so
    /// a successful transaction still returns a healthy connection to the pool for reuse — which
    /// `commit_reuses_a_pooled_connection` asserts, so that this can never quietly become
    /// "reconnect every time".
    ///
    /// Nothing is spawned and nothing is awaited: `detach` is synchronous and the raw connection
    /// has no `Drop` of its own, so shutdown cannot hang here.
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            drop(connection.detach());
        }
    }
}

/// A pooled connection held outside any transaction.
///
/// # Why this exists alongside [`SqlxUnitOfWork`]
///
/// So that a repository written once against [`SqlxExecutor`] runs both inside a transaction and
/// outside one. Without it every repository would need two implementations, and the second would
/// drift from the first.
pub struct SqlxConnection<DB: sqlx::Database> {
    connection: sqlx::pool::PoolConnection<DB>,
    kind: DatabaseKind,
}

impl<DB: sqlx::Database> core::fmt::Debug for SqlxConnection<DB> {
    /// Prints the kind and nothing about the connection, for [`SqlxUnitOfWork`]'s reason.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SqlxConnection")
            .field("kind", &self.kind)
            .finish()
    }
}

/// Hands out the driver connection underneath an executor.
///
/// # Why this is separate from [`renvor_database::Executor`]
///
/// The port may not name a driver type — that is what makes an application's repository trait
/// mechanically unable to reach one. This trait lives in the adapter, where a driver is already
/// visible, and is what an adapter-side repository implementation is actually generic over.
pub trait SqlxExecutor {
    /// The driver this executor speaks.
    type Database: sqlx::Database;

    /// The underlying connection, for a repository to execute against.
    fn connection(&mut self) -> &mut <Self::Database as sqlx::Database>::Connection;
}

impl<DB: sqlx::Database> SqlxExecutor for SqlxConnection<DB> {
    type Database = DB;

    fn connection(&mut self) -> &mut DB::Connection {
        &mut self.connection
    }
}

impl<DB: sqlx::Database> SqlxExecutor for SqlxUnitOfWork<'_, DB> {
    type Database = DB;

    fn connection(&mut self) -> &mut DB::Connection {
        self.inner()
    }
}

impl<DB: sqlx::Database> renvor_database::Executor for SqlxConnection<DB> {
    fn kind(&self) -> DatabaseKind {
        self.kind
    }

    /// Always `false`. A pooled connection outside a transaction is exactly what this type is.
    fn in_transaction(&self) -> bool {
        false
    }
}

impl<DB: sqlx::Database> renvor_database::Executor for SqlxUnitOfWork<'_, DB> {
    fn kind(&self) -> DatabaseKind {
        self.kind
    }

    /// Always `true`. This type only exists between a `begin` and its commit or rollback.
    fn in_transaction(&self) -> bool {
        true
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
    for<'e> &'e mut DB::Connection: sqlx::Executor<'e, Database = DB>,
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
        // Acquire the connection ourselves rather than calling `Pool::begin`, which would hand
        // ownership to a `sqlx::Transaction` and leave no way to discard the connection when a
        // future is cancelled. See `SqlxUnitOfWork` for what that costs.
        let mut connection = self
            .pool
            .acquire()
            .await
            .map_err(|error| error::classify_error(&error))?;
        // `Executor::execute` with a bare string uses the TEXT protocol. `sqlx::query`
        // would prepare the statement, and MySQL's prepared-statement protocol does not
        // accept `BEGIN` — this is the same call shape `sqlx`'s own transaction manager uses.
        if let Err(error) = sqlx::Executor::execute(&mut *connection, BEGIN).await {
            let _ = error::classify_error(&error);
            // The transaction never opened, so this connection is discarded rather than returned:
            // its protocol state is unknown for exactly the same reason.
            drop(connection.detach());
            return Err(DatabaseError::new(DatabaseErrorKind::StatementRejected));
        }
        Ok(SqlxUnitOfWork {
            connection: Some(connection),
            kind: self.kind,
            borrow: core::marker::PhantomData,
        })
    }

    async fn check(&self) -> Result<(), DatabaseError> {
        // A REAL ROUND TRIP. Reporting ready without one would let readiness precede the database's
        // ability to serve, which principle IV forbids. Acquiring first makes an exhausted pool
        // report as an acquire timeout rather than as an unready database — two different faults
        // with two different corrections.
        let mut guard = self
            .pool
            .acquire()
            .await
            .map_err(|error| error::classify_error(&error))?;

        // ON THE GUARD, NOT ON THE POOL, and the difference is a deadlock.
        //
        // This ran `.execute(&self.pool)` while still holding `guard`, so it acquired a SECOND
        // connection. `PoolSettings::with_max_connections` accepts `1` — a documented, supported
        // choice for a sidecar bounding load on a shared database — and with it, the second
        // acquire waits out the whole `acquire_timeout` and returns `PoolTimedOut`. `check()` maps
        // that to `NotReady`, so Boot is refused identically on every restart, reporting that the
        // database is not ready when the database is fine.
        //
        // Acquiring first is still deliberate: it makes an exhausted pool report as an acquire
        // timeout rather than as an unready database. Running the probe on the connection already
        // held keeps that property and costs one connection instead of two.
        sqlx::Executor::execute(&mut *guard, READINESS_PROBE)
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
