//! Translating SeaORM and driver failures into the redacted persistence vocabulary.
//!
//! # This module deliberately discards information
//!
//! [`sea_orm::DbErr`] carries a message, and a SeaORM message routinely contains the generated
//! SQL, the offending value, the table and column, and — for a connection failure — the host.
//! [`DatabaseError`] has no field any of that can inhabit, so translating here is not lossy by
//! accident; it is lossy on purpose, and this module is the single place the loss happens.
//!
//! The original text is not thrown away entirely. It is emitted through `tracing` at `debug`,
//! which reaches operators rather than callers.
//!
//! # Why this is not shared with `renvor-sqlx`
//!
//! The two adapters classify **different vocabularies**. This one reads `DbErr`, whose variants
//! carry meanings SQLx has no equivalent for — `RecordNotInserted` and `RecordNotUpdated` are
//! SeaORM's way of reporting an affected-row count of zero, not driver errors at all. Sharing a
//! mapping would mean one of the two crates translating into the other's terms first, and the
//! shared crate is `renvor-database`, which may not name a driver. Neither adapter depends on the
//! other, which is the property `xtask` step 7 asserts.

use renvor_database::{DatabaseError, DatabaseErrorKind};
use sea_orm::{DbErr, RuntimeErr, SqlErr};

/// Translates a SeaORM error, recording the original for operators only.
///
/// # Why this is public
///
/// A repository implementation in an application's adapter layer receives a `DbErr` from an
/// idiomatic SeaORM call and owes its caller a [`DatabaseError`]. Without this function every
/// application would write its own mapping, and each one would be a fresh opportunity to put
/// SeaORM's text — and therefore a value, a table name, generated SQL, or a host — into something
/// a caller receives.
pub fn classify_db_error(error: &DbErr) -> DatabaseError {
    // The original text goes to telemetry. It never reaches the returned value.
    tracing::debug!(driver_error = %error, "database operation failed");
    DatabaseError::new(classify_db(error))
}

/// Translates a driver error raised by this adapter's own connection handling.
pub fn classify_error(error: &sqlx::Error) -> DatabaseError {
    tracing::debug!(driver_error = %error, "database operation failed");
    DatabaseError::new(classify_sqlx(error))
}

/// The classification, with no reference to any message text.
fn classify_db(error: &DbErr) -> DatabaseErrorKind {
    // SeaORM's OWN portable classifier runs first. `sql_err` reads the driver's constraint code
    // rather than matching on a message, which is what makes it usable across both engines —
    // PostgreSQL and MySQL word a unique violation completely differently.
    if let Some(sql) = error.sql_err() {
        return match sql {
            SqlErr::UniqueConstraintViolation(_) => DatabaseErrorKind::UniqueViolation,
            SqlErr::ForeignKeyConstraintViolation(_) => DatabaseErrorKind::ForeignKeyViolation,
            // `SqlErr` is `#[non_exhaustive]`. A variant added upstream is a constraint violation
            // this mapping does not yet name — `StatementRejected` is the honest answer, and is
            // deliberately NOT `Unclassified`: SeaORM has already told us it is a constraint.
            _ => DatabaseErrorKind::StatementRejected,
        };
    }
    match error {
        DbErr::ConnectionAcquire(_) => DatabaseErrorKind::AcquireTimeout,
        DbErr::Conn(inner) => runtime_kind(inner, DatabaseErrorKind::ConnectFailed),
        DbErr::Exec(inner) | DbErr::Query(inner) => {
            runtime_kind(inner, DatabaseErrorKind::StatementRejected)
        }
        DbErr::RecordNotFound(_) => DatabaseErrorKind::NotFound,
        // NOT `NotFound`. These two mean "the statement ran and affected no rows", which is a
        // concurrency outcome an optimistic-locking caller must distinguish from "no such row".
        DbErr::RecordNotInserted | DbErr::RecordNotUpdated => DatabaseErrorKind::StatementRejected,
        DbErr::Type(_) | DbErr::Json(_) | DbErr::TryIntoErr { .. } | DbErr::ConvertFromU64(_) => {
            DatabaseErrorKind::TypeMismatch
        }
        DbErr::Migration(_) => DatabaseErrorKind::MigrationFailed,
        _ => DatabaseErrorKind::Unclassified,
    }
}

/// Unwraps a [`RuntimeErr`] to the driver error underneath, when there is one.
fn runtime_kind(error: &RuntimeErr, fallback: DatabaseErrorKind) -> DatabaseErrorKind {
    match error {
        // Gated because SeaORM gates the variant: `SqlxError` exists only under `sqlx-dep`,
        // which no driver feature turns on. With neither driver selected — the default, and the
        // configuration `cargo package` verifies — there is no SQLx error to unwrap.
        #[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
        RuntimeErr::SqlxError(inner) => classify_sqlx(inner),
        _ => fallback,
    }
}

/// The driver-level classification.
fn classify_sqlx(error: &sqlx::Error) -> DatabaseErrorKind {
    use sqlx::error::ErrorKind;
    match error {
        sqlx::Error::PoolTimedOut => DatabaseErrorKind::AcquireTimeout,
        sqlx::Error::PoolClosed => DatabaseErrorKind::PoolClosed,
        sqlx::Error::RowNotFound => DatabaseErrorKind::NotFound,
        sqlx::Error::Io(_) | sqlx::Error::Tls(_) => DatabaseErrorKind::ConnectFailed,
        sqlx::Error::Configuration(_) | sqlx::Error::ConfigFile(_) => {
            DatabaseErrorKind::ConnectFailed
        }
        sqlx::Error::BeginFailed => DatabaseErrorKind::StatementRejected,
        sqlx::Error::Decode(_)
        | sqlx::Error::Encode(_)
        | sqlx::Error::ColumnDecode { .. }
        | sqlx::Error::TypeNotFound { .. }
        | sqlx::Error::ColumnNotFound(_)
        | sqlx::Error::ColumnIndexOutOfBounds { .. } => DatabaseErrorKind::TypeMismatch,
        sqlx::Error::Database(inner) => match inner.kind() {
            ErrorKind::UniqueViolation => DatabaseErrorKind::UniqueViolation,
            ErrorKind::ForeignKeyViolation => DatabaseErrorKind::ForeignKeyViolation,
            _ => DatabaseErrorKind::StatementRejected,
        },
        sqlx::Error::Migrate(inner) => migrate_kind(inner),
        _ => DatabaseErrorKind::Unclassified,
    }
}

/// The migration-specific classification.
fn migrate_kind(error: &sqlx::migrate::MigrateError) -> DatabaseErrorKind {
    use sqlx::migrate::MigrateError;
    match error {
        MigrateError::VersionMismatch(_) | MigrateError::VersionNotPresent(_) => {
            DatabaseErrorKind::MigrationChecksumMismatch
        }
        MigrateError::Dirty(_) => DatabaseErrorKind::MigrationDirty,
        MigrateError::VersionMissing(_) => DatabaseErrorKind::MigrationIrreversible,
        _ => DatabaseErrorKind::MigrationFailed,
    }
}
