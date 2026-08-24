//! The SeaORM adapter for Renvor.
//!
//! # What selecting SeaORM changes, and what it does not
//!
//! It changes the **programming model**: an application writes `Entity::find().all(&uow)` instead
//! of hand-written SQL. It does **not** change the driver family. SeaORM is built on SQLx, so a
//! SeaORM project resolves SQLx transitively, and this crate depends on SQLx directly. That is
//! stated here rather than left to be discovered in a lockfile.
//!
//! Direct-SQLx application APIs are **not** part of this surface. `renvor-sqlx` and this crate are
//! siblings; neither depends on the other, and choosing one does not hand you the other's types.
//!
//! # Why Renvor owns the connection instead of using `sea_orm::DatabaseTransaction`
//!
//! SeaORM's own transaction type cannot satisfy the cancellation contract Phase 006 established
//! (`renvor_database::UnitOfWork`), and the reason is structural rather than a bug to wait out:
//!
//! ```text
//! sea_orm::DatabaseTransaction { conn: Arc<Mutex<InnerConnection>>, .. }
//!
//! impl Drop for DatabaseTransaction {
//!     fn drop(&mut self) {
//!         self.start_rollback().expect("Fail to rollback transaction");
//!     }                     //  ^^^^^^ panics when `try_lock` fails
//! }
//! ```
//!
//! Because the connection is behind an `Arc`, drop can only `try_lock`, and a failed try-lock has
//! nowhere to go but `expect`. It then hands the connection to SQLx's queued-rollback path, which
//! ADR-0017 measured **leaking pool capacity permanently on MySQL**.
//!
//! [`SeaOrmUnitOfWork`] is uniquely owned, so its `Drop` reaches the connection through
//! [`tokio::sync::Mutex::get_mut`] — no lock, no `try_lock`, and no failure mode to `expect` on.
//! It then `detach`es, which frees the pool slot synchronously. See ADR-0021.
//!
//! The application still writes idiomatic SeaORM, because [`SeaOrmUnitOfWork`] implements
//! [`sea_orm::ConnectionTrait`]. Every SeaORM query method takes `&impl ConnectionTrait`.

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod error;
pub mod migrate;
pub mod provider;
pub mod version;

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use renvor_database::Database;
use renvor_database::{
    ConnectionString, DatabaseError, DatabaseErrorKind, DatabaseKind, PoolSettings, UnitOfWork,
};
use sqlx::pool::PoolOptions;

/// A configured, bounded connection to one database, presented through SeaORM.
#[cfg_attr(
    not(any(feature = "db-postgres", feature = "db-mysql")),
    allow(
        dead_code,
        reason = "with neither driver selected — the DEFAULT, since neither is a default feature, \
              and the configuration `cargo package` verifies — the per-driver impls that read \
              these do not exist. Phase 006 shipped a crate that failed to build in exactly this \
              configuration; the annotation is how that is not repeated"
    )
)]
pub struct SeaOrmDatabase<DB: sqlx::Database> {
    pool: sqlx::Pool<DB>,
    kind: DatabaseKind,
    close_timeout: core::time::Duration,
}

impl<DB: sqlx::Database> core::fmt::Debug for SeaOrmDatabase<DB> {
    /// Prints the database kind and pool counters, and nothing that could carry a credential.
    ///
    /// A derived `Debug` would print whatever `sqlx::Pool`'s own `Debug` prints, which includes
    /// connect options — and therefore a host, a user, and potentially a password. FR-009 is met
    /// structurally here rather than by hoping the driver stays discreet.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SeaOrmDatabase")
            .field("kind", &self.kind)
            .field("size", &self.pool.size())
            .field("idle", &self.pool.num_idle())
            .finish()
    }
}

/// Pool counters, for tests and for observability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolStatus {
    size: u32,
    idle: usize,
}

impl PoolStatus {
    /// Connections the pool currently owns, idle or checked out.
    #[must_use]
    pub const fn size(self) -> u32 {
        self.size
    }

    /// Connections available for immediate checkout.
    #[must_use]
    pub const fn idle(self) -> usize {
        self.idle
    }
}

impl<DB: sqlx::Database> SeaOrmDatabase<DB> {
    /// Opens the pool, establishing the first connection before returning.
    ///
    /// Every [`PoolSettings`] field is mapped onto the pool here, including the connect deadline
    /// SQLx has nowhere to put — see the inline note.
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

        // The connect deadline is applied here for the reason `renvor-sqlx` records: `PoolOptions`
        // has no separate bound on ESTABLISHING a connection, so a `connect_timeout` that was
        // accepted and validated would otherwise be passed to nothing.
        let pool = match tokio::time::timeout(settings.connect_timeout(), opening).await {
            Ok(Ok(pool)) => pool,
            Ok(Err(error)) => return Err(error::classify_error(&error)),
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
    /// On the **adapter** side of the boundary, for the reason `renvor-sqlx` records: an
    /// application's own adapter module may legitimately need it, and an application *service*
    /// that reaches for it has crossed the line the dependency rule draws.
    #[must_use]
    pub const fn pool(&self) -> &sqlx::Pool<DB> {
        &self.pool
    }

    /// Pool statistics, for tests and for observability.
    #[must_use]
    pub fn connections(&self) -> PoolStatus {
        PoolStatus {
            size: self.pool.size(),
            idle: self.pool.num_idle(),
        }
    }
}

/// A SeaORM-backed PostgreSQL database.
///
/// Named so that application code and tests do not spell a SQLx type to say which engine they
/// mean — the alias is the boundary FR-004 asserts, stated as a name rather than a convention.
#[cfg(feature = "db-postgres")]
pub type PostgresDatabase = SeaOrmDatabase<sqlx::Postgres>;

/// A SeaORM-backed MySQL database.
#[cfg(feature = "db-mysql")]
pub type MySqlDatabase = SeaOrmDatabase<sqlx::MySql>;

/// An open transaction, owned by the application service that began it.
///
/// It is both a [`renvor_database::UnitOfWork`] — so a service can commit or roll back — and a
/// [`sea_orm::ConnectionTrait`] — so every SeaORM query method accepts it directly:
///
/// ```ignore
/// let uow = database.begin().await?;
/// let posts = post::Entity::find().all(&uow).await?;   // idiomatic SeaORM
/// uow.commit().await?;                                  // explicit Renvor boundary
/// ```
///
/// # Nesting is unrepresentable
///
/// There is no `begin` on this type. Calling [`Database::begin`] again yields a **separate**
/// session on a **different** connection, which is asserted rather than assumed.
pub struct SeaOrmUnitOfWork<'c, DB: sqlx::Database> {
    /// `None` once `commit` or `rollback` has taken the connection.
    ///
    /// This is what lets `Drop` tell an abnormal end from a normal one: a connection still present
    /// means neither method ran, which means the future was cancelled.
    ///
    /// The `Mutex` exists because [`sea_orm::ConnectionTrait`] takes `&self` while SQLx needs
    /// `&mut`. It is **not** behind an `Arc` — see this crate's module documentation for why that
    /// single difference from SeaORM's own type removes a panic.
    connection: tokio::sync::Mutex<Option<sqlx::pool::PoolConnection<DB>>>,
    kind: DatabaseKind,
    borrow: core::marker::PhantomData<&'c ()>,
}

impl<DB: sqlx::Database> core::fmt::Debug for SeaOrmUnitOfWork<'_, DB> {
    /// Prints whether the transaction is still open, and nothing about the connection.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SeaOrmUnitOfWork")
            .field("kind", &self.kind)
            // `try_lock` rather than `lock`: `Debug` is not async, and a formatter that could
            // block would be a deadlock waiting for a log line.
            .field(
                "open",
                &self
                    .connection
                    .try_lock()
                    .map_or("in use", |guard| if guard.is_some() { "yes" } else { "no" }),
            )
            .finish()
    }
}

/// Opens a transaction. Both databases accept it unqualified.
#[cfg_attr(
    not(any(feature = "db-postgres", feature = "db-mysql")),
    allow(dead_code, reason = "issued only by the feature-gated impls")
)]
const BEGIN: &str = "BEGIN";
/// Commits the open transaction.
#[cfg_attr(
    not(any(feature = "db-postgres", feature = "db-mysql")),
    allow(dead_code, reason = "issued only by the feature-gated impls")
)]
const COMMIT: &str = "COMMIT";
/// Rolls back the open transaction.
#[cfg_attr(
    not(any(feature = "db-postgres", feature = "db-mysql")),
    allow(dead_code, reason = "issued only by the feature-gated impls")
)]
const ROLLBACK: &str = "ROLLBACK";

impl<DB: sqlx::Database> SeaOrmUnitOfWork<'_, DB> {
    /// Takes the connection out, marking this end as normal so `Drop` will not discard it.
    fn take(&mut self) -> Option<sqlx::pool::PoolConnection<DB>> {
        // `get_mut` rather than `lock().await`: this is called from `commit`/`rollback`, which
        // consume `self`, so no other reference can exist and no locking is needed.
        self.connection.get_mut().take()
    }
}

impl<DB: sqlx::Database> UnitOfWork for SeaOrmUnitOfWork<'_, DB>
where
    for<'e> &'e mut DB::Connection: sqlx::Executor<'e, Database = DB>,
{
    async fn commit(mut self) -> Result<(), DatabaseError> {
        let Some(mut connection) = self.take() else {
            return Err(DatabaseError::new(DatabaseErrorKind::CommitFailed));
        };
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
        let Some(mut connection) = self.take() else {
            return Err(DatabaseError::new(DatabaseErrorKind::RollbackFailed));
        };
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

impl<DB: sqlx::Database> Drop for SeaOrmUnitOfWork<'_, DB> {
    /// Discards the connection when the transaction ended without `commit` or `rollback`.
    ///
    /// # This is the method SeaORM's own transaction cannot write
    ///
    /// `Mutex::get_mut` takes `&mut self`, which `Drop` already has, and therefore **cannot fail**
    /// — there is no lock to contend for when the compiler has proved unique access. SeaORM's
    /// `DatabaseTransaction` keeps its connection in an `Arc<Mutex<_>>`, so its drop can only
    /// `try_lock`, and its failure path is `expect(..)` — a panic reachable from `Drop`, which
    /// during an unwind aborts the process.
    ///
    /// `detach` releases the pool's size guard **synchronously**, so the pool may open a
    /// replacement immediately; dropping the detached connection closes its socket, and both
    /// databases roll back an open transaction when the client disconnects.
    ///
    /// This runs only on the abnormal path — `commit` and `rollback` take the connection first, so
    /// a successful transaction still returns a healthy connection for reuse.
    ///
    /// Nothing is spawned and nothing is awaited, so shutdown cannot hang here.
    fn drop(&mut self) {
        if let Some(connection) = self.connection.get_mut().take() {
            drop(connection.detach());
        }
    }
}

/// A pooled connection held outside any transaction.
///
/// Exists so that a repository written against [`sea_orm::ConnectionTrait`] serves both
/// transactional and non-transactional callers without a second implementation.
#[cfg_attr(
    not(any(feature = "db-postgres", feature = "db-mysql")),
    allow(
        dead_code,
        reason = "read only by the feature-gated `ConnectionTrait` impls"
    )
)]
pub struct SeaOrmConnection<DB: sqlx::Database> {
    connection: tokio::sync::Mutex<sqlx::pool::PoolConnection<DB>>,
    kind: DatabaseKind,
}

impl<DB: sqlx::Database> core::fmt::Debug for SeaOrmConnection<DB> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SeaOrmConnection")
            .field("kind", &self.kind)
            .finish()
    }
}

impl<DB: sqlx::Database> renvor_database::Executor for SeaOrmUnitOfWork<'_, DB> {
    fn kind(&self) -> DatabaseKind {
        self.kind
    }

    /// Always `true`. This type exists only inside a transaction.
    fn in_transaction(&self) -> bool {
        true
    }
}

impl<DB: sqlx::Database> renvor_database::Executor for SeaOrmConnection<DB> {
    fn kind(&self) -> DatabaseKind {
        self.kind
    }

    /// Always `false`. A statement here is its own implicit transaction.
    fn in_transaction(&self) -> bool {
        false
    }
}

/// Generates the per-driver halves that cannot be written generically.
///
/// # Why a macro rather than a generic implementation
///
/// [`sea_orm::QueryResult`] is built from a **concrete** row — `From<PgRow>` and `From<MySqlRow>`
/// are separate public impls, and there is no trait unifying them. The same is true of
/// `ExecResult`. A generic implementation would need a bound that SeaORM does not declare, so the
/// two drivers are generated instead of abstracted. This is the same shape `renvor-sqlx`'s
/// migration runner uses, and for the same reason.
#[cfg_attr(
    not(any(feature = "db-postgres", feature = "db-mysql")),
    allow(
        unused_macros,
        reason = "invoked once per driver feature; with none, never"
    )
)]
macro_rules! driver {
    (
        $driver:ty,
        $row:ty,
        $done:ty,
        $backend:expr,
        $connect:ident,
        $kind:expr,
        $scheme:literal,
        $feature:literal,
        $bind:ident
    ) => {
        impl SeaOrmDatabase<$driver> {
            /// Checks out a pooled connection for work that needs no transaction.
            ///
            /// # Errors
            ///
            /// [`DatabaseErrorKind::AcquireTimeout`] when the pool is full, or
            /// [`DatabaseErrorKind::ConnectFailed`] when a new connection cannot be established.
            pub async fn acquire(&self) -> Result<SeaOrmConnection<$driver>, DatabaseError> {
                let connection = self
                    .pool
                    .acquire()
                    .await
                    .map_err(|error| error::classify_error(&error))?;
                Ok(SeaOrmConnection {
                    connection: tokio::sync::Mutex::new(connection),
                    kind: self.kind,
                })
            }
        }

        /// Opens a pool against this engine.
        ///
        /// # Errors
        ///
        /// [`DatabaseErrorKind::ConnectFailed`], redacted of the URL, host, user, and password.
        #[cfg(feature = $feature)]
        pub async fn $connect(
            dsn: &ConnectionString,
            settings: &PoolSettings,
        ) -> Result<SeaOrmDatabase<$driver>, DatabaseError> {
            SeaOrmDatabase::<$driver>::connect(dsn, settings, $kind).await
        }

        impl Database for SeaOrmDatabase<$driver> {
            type UnitOfWork<'c>
                = SeaOrmUnitOfWork<'c, $driver>
            where
                Self: 'c;

            fn kind(&self) -> DatabaseKind {
                self.kind
            }

            async fn begin(&self) -> Result<Self::UnitOfWork<'_>, DatabaseError> {
                let mut connection = self
                    .pool
                    .acquire()
                    .await
                    .map_err(|error| error::classify_error(&error))?;

                // The TEXT protocol, deliberately. `sqlx::query` would PREPARE this, and MySQL's
                // prepared-statement protocol does not accept `BEGIN`. Phase 006 found this the
                // hard way; it is written down here so it is not rediscovered.
                if let Err(error) = sqlx::Executor::execute(&mut *connection, BEGIN).await {
                    let classified = error::classify_error(&error);
                    // The slot is freed synchronously rather than left to the unbounded return
                    // path, so a failed BEGIN cannot cost capacity either.
                    drop(connection.detach());
                    return Err(classified);
                }

                Ok(SeaOrmUnitOfWork {
                    connection: tokio::sync::Mutex::new(Some(connection)),
                    kind: self.kind,
                    borrow: core::marker::PhantomData,
                })
            }

            async fn check(&self) -> Result<(), DatabaseError> {
                let mut connection = self
                    .pool
                    .acquire()
                    .await
                    .map_err(|error| error::classify_error(&error))?;
                // Run on the connection already held. Acquiring a SECOND one would make the check
                // report on a connection it never used.
                sqlx::Executor::execute(&mut *connection, "SELECT 1")
                    .await
                    .map_err(|error| error::classify_error(&error))?;
                Ok(())
            }

            async fn close(&self) -> Result<(), DatabaseError> {
                match tokio::time::timeout(self.close_timeout, self.pool.close()).await {
                    Ok(()) => Ok(()),
                    // A forced close is an error, never a successful shutdown (principle IV).
                    Err(_) => Err(DatabaseError::new(DatabaseErrorKind::DeadlineExceeded)),
                }
            }
        }

        #[async_trait::async_trait]
        impl sea_orm::ConnectionTrait for SeaOrmUnitOfWork<'_, $driver> {
            fn get_database_backend(&self) -> sea_orm::DbBackend {
                $backend
            }

            async fn execute_raw(
                &self,
                stmt: sea_orm::Statement,
            ) -> Result<sea_orm::ExecResult, sea_orm::DbErr> {
                let mut guard = self.connection.lock().await;
                let connection = guard.as_mut().ok_or_else(closed)?;
                $bind(&stmt)
                    .execute(&mut **connection)
                    .await
                    .map(<$done>::into)
                    .map_err(exec_failed)
            }

            /// # Never interpolate a caller-controlled value into `sql`
            ///
            /// This is the escape hatch's last rung, and the only one with no binding. FR-037: a
            /// value from a request belongs in `Statement::from_sql_and_values`, one rung up.
            async fn execute_unprepared(
                &self,
                sql: &str,
            ) -> Result<sea_orm::ExecResult, sea_orm::DbErr> {
                // `sqlx::raw_sql` + `AssertSqlSafe` rather than a bare `&str`: SQLx 0.9 accepts
                // an unwrapped string ONLY when it is `&'static str`, precisely so that
                // caller-supplied SQL has to be marked. Owning it also detaches the query's
                // lifetime from the `&str` parameter, which does not outlive the boxed future.
                //
                // The assertion is the CALLER's, and it is the whole meaning of this method: FR-037
                // — a value from a request belongs in `Statement::from_sql_and_values`, one rung up
                // the escape hatch, where it is bound rather than concatenated.
                let statement = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()));
                let mut guard = self.connection.lock().await;
                let connection = guard.as_mut().ok_or_else(closed)?;
                // Bound to a local before mapping: as a tail expression the temporaries would
                // outlive `statement`, which the borrow checker rejects.
                let outcome = sqlx::Executor::execute(&mut **connection, statement).await;
                outcome.map(<$done>::into).map_err(exec_failed)
            }

            async fn query_one_raw(
                &self,
                stmt: sea_orm::Statement,
            ) -> Result<Option<sea_orm::QueryResult>, sea_orm::DbErr> {
                let mut guard = self.connection.lock().await;
                let connection = guard.as_mut().ok_or_else(closed)?;
                $bind(&stmt)
                    .fetch_optional(&mut **connection)
                    .await
                    .map(|row| row.map(<$row>::into))
                    .map_err(query_failed)
            }

            async fn query_all_raw(
                &self,
                stmt: sea_orm::Statement,
            ) -> Result<Vec<sea_orm::QueryResult>, sea_orm::DbErr> {
                let mut guard = self.connection.lock().await;
                let connection = guard.as_mut().ok_or_else(closed)?;
                $bind(&stmt)
                    .fetch_all(&mut **connection)
                    .await
                    .map(|rows| rows.into_iter().map(<$row>::into).collect())
                    .map_err(query_failed)
            }
        }

        #[async_trait::async_trait]
        impl sea_orm::ConnectionTrait for SeaOrmConnection<$driver> {
            fn get_database_backend(&self) -> sea_orm::DbBackend {
                $backend
            }

            async fn execute_raw(
                &self,
                stmt: sea_orm::Statement,
            ) -> Result<sea_orm::ExecResult, sea_orm::DbErr> {
                let mut connection = self.connection.lock().await;
                $bind(&stmt)
                    .execute(&mut **connection)
                    .await
                    .map(<$done>::into)
                    .map_err(exec_failed)
            }

            /// See [`SeaOrmUnitOfWork`]'s implementation: never interpolate a caller value here.
            async fn execute_unprepared(
                &self,
                sql: &str,
            ) -> Result<sea_orm::ExecResult, sea_orm::DbErr> {
                // See the unit-of-work implementation above for why this is wrapped.
                let statement = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()));
                let mut connection = self.connection.lock().await;
                // Bound to a local before mapping: as a tail expression the temporaries would
                // outlive `statement`, which the borrow checker rejects.
                let outcome = sqlx::Executor::execute(&mut **connection, statement).await;
                outcome.map(<$done>::into).map_err(exec_failed)
            }

            async fn query_one_raw(
                &self,
                stmt: sea_orm::Statement,
            ) -> Result<Option<sea_orm::QueryResult>, sea_orm::DbErr> {
                let mut connection = self.connection.lock().await;
                $bind(&stmt)
                    .fetch_optional(&mut **connection)
                    .await
                    .map(|row| row.map(<$row>::into))
                    .map_err(query_failed)
            }

            async fn query_all_raw(
                &self,
                stmt: sea_orm::Statement,
            ) -> Result<Vec<sea_orm::QueryResult>, sea_orm::DbErr> {
                let mut connection = self.connection.lock().await;
                $bind(&stmt)
                    .fetch_all(&mut **connection)
                    .await
                    .map(|rows| rows.into_iter().map(<$row>::into).collect())
                    .map_err(query_failed)
            }
        }

        /// Binds a SeaORM statement's values onto a prepared query.
        ///
        /// This is the one line SeaORM keeps `pub(crate)` (`driver::sqlx_postgres::sqlx_query`).
        /// It is reproduced through the **published** `sea-query-sqlx` crate rather than
        /// reimplemented: `SqlxValues` is the `sqlx::Arguments` implementation that knows how to
        /// bind every SeaQuery value type, and hand-writing that mapping is exactly the
        /// reimplementation constitution principle III forbids.
        fn $bind(
            stmt: &sea_orm::Statement,
        ) -> sqlx::query::Query<'_, $driver, sea_query_sqlx::SqlxValues> {
            let values = stmt
                .values
                .as_ref()
                .map_or_else(|| sea_orm::sea_query::Values(Vec::new()), Clone::clone);
            // `AssertSqlSafe` is SQLx 0.9's marker that this string is not caller-interpolated.
            // It is sound here because `stmt.sql` is built by SeaQuery, which parameterises every
            // value it is given; the values travel separately, in `SqlxValues`.
            sqlx::query_with(
                sqlx::AssertSqlSafe(stmt.sql.as_str()),
                sea_query_sqlx::SqlxValues(values),
            )
        }
    };
}

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
/// The error for a statement issued after `commit`/`rollback` took the connection.
///
/// Unreachable through the public API — both consume `self` — but `ConnectionTrait` takes `&self`
/// and cannot express that, so the case is answered rather than `unwrap`ped.
fn closed() -> sea_orm::DbErr {
    sea_orm::DbErr::Conn(sea_orm::RuntimeErr::Internal(
        "the unit of work has already been committed or rolled back".to_owned(),
    ))
}

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
/// Wraps a driver error for SeaORM's own vocabulary, on the execute path.
fn exec_failed(error: sqlx::Error) -> sea_orm::DbErr {
    sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(std::sync::Arc::new(error)))
}

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
/// Wraps a driver error for SeaORM's own vocabulary, on the query path.
fn query_failed(error: sqlx::Error) -> sea_orm::DbErr {
    sea_orm::DbErr::Query(sea_orm::RuntimeErr::SqlxError(std::sync::Arc::new(error)))
}

#[cfg(feature = "db-postgres")]
driver!(
    sqlx::Postgres,
    sqlx::postgres::PgRow,
    sqlx::postgres::PgQueryResult,
    sea_orm::DbBackend::Postgres,
    connect_postgres,
    DatabaseKind::Postgres,
    "postgres",
    "db-postgres",
    bind_postgres
);

#[cfg(feature = "db-mysql")]
driver!(
    sqlx::MySql,
    sqlx::mysql::MySqlRow,
    sqlx::mysql::MySqlQueryResult,
    sea_orm::DbBackend::MySql,
    connect_mysql,
    DatabaseKind::MySql,
    "mysql",
    "db-mysql",
    bind_mysql
);
