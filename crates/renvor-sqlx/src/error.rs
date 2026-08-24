//! Translating driver failures into the redacted persistence vocabulary.
//!
//! # This module deliberately discards information
//!
//! `sqlx::Error` carries a driver message, and a driver message routinely contains the offending
//! value, the table and column, and — for a connection failure — the host. `DatabaseError` has no
//! field any of that can inhabit, so translating here is not lossy by accident; it is lossy on
//! purpose, and this module is the single place the loss happens.
//!
//! The driver's own text is not thrown away entirely. It is emitted through `tracing` at `debug`,
//! which reaches operators rather than callers — the same split `renvor-error` already makes
//! between a Problem Details document and a telemetry span.

use renvor_database::{DatabaseError, DatabaseErrorKind};
use sqlx::error::ErrorKind;

/// Translates a driver error, recording the original for operators only.
///
/// # Why this is public
///
/// A repository implementation in an application's adapter layer receives a `sqlx::Error` and owes
/// its caller a [`DatabaseError`]. Without this function every application would write its own
/// mapping, and each one would be a fresh opportunity to put the driver's text — and therefore a
/// value, a table name, or a host — into something a caller receives.
///
/// # Why the original is logged rather than returned
///
/// A caller that received the driver's text would be receiving a string this crate cannot bound,
/// cannot redact, and does not control the shape of. An operator reading a span is a different
/// consumer with different rights.
pub fn classify_error(error: &sqlx::Error) -> DatabaseError {
    // The driver's text goes to telemetry. It never reaches the returned value.
    tracing::debug!(driver_error = %error, "database operation failed");
    DatabaseError::new(classify(error))
}

/// Classifies a failure to **establish** a connection.
///
/// # Why the general mapper is wrong here
///
/// `connect` documents [`DatabaseErrorKind::ConnectFailed`] for *"when the connection could not be
/// established"*. A server that refuses the handshake — a wrong password, an unknown database, a
/// connection limit reached — reports that as [`sqlx::Error::Database`], which the general mapper
/// classifies by SQLSTATE and lands on `StatementRejected`. That names a statement the caller never
/// sent, and sends an operator looking for SQL when the answer is a credential.
///
/// The distinction is the CALL SITE, not the SQLSTATE: mid-session an authorization error really is
/// a rejected statement, so this is deliberately not folded into [`classify_error`].
pub fn classify_connect_error(error: &sqlx::Error) -> DatabaseError {
    match error {
        sqlx::Error::Database(_) => DatabaseError::new(DatabaseErrorKind::ConnectFailed),
        other => classify_error(other),
    }
}

/// The classification, with no reference to the driver's text.
fn classify(error: &sqlx::Error) -> DatabaseErrorKind {
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

/// Classifies a migration failure.
///
/// # `Dirty` is separated from `MigrationFailed`, and the reason is operational
///
/// A failed migration can be retried after the cause is fixed. A **dirty** schema cannot: SQLx's
/// own source records that *"for MySQL we cannot really isolate migrations due to implicit commits
/// caused by table modification"*, so an interrupted MySQL migration leaves half its DDL applied.
/// Reporting both as one kind would tell an operator to retry something that needs manual repair.
fn migrate_kind(error: &sqlx::migrate::MigrateError) -> DatabaseErrorKind {
    match error {
        sqlx::migrate::MigrateError::VersionMismatch(_) => {
            DatabaseErrorKind::MigrationChecksumMismatch
        }
        sqlx::migrate::MigrateError::Dirty(_) => DatabaseErrorKind::MigrationDirty,
        sqlx::migrate::MigrateError::VersionNotPresent(_)
        | sqlx::migrate::MigrateError::VersionMissing(_) => {
            DatabaseErrorKind::MigrationChecksumMismatch
        }
        _ => DatabaseErrorKind::MigrationFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pool_timeout_is_a_capacity_signal_not_an_availability_one() {
        assert_eq!(
            classify(&sqlx::Error::PoolTimedOut),
            DatabaseErrorKind::AcquireTimeout
        );
    }

    #[test]
    fn a_closed_pool_reports_as_shutting_down_rather_than_as_a_defect() {
        let kind = classify(&sqlx::Error::PoolClosed);
        assert_eq!(kind, DatabaseErrorKind::PoolClosed);
        assert_eq!(
            kind.category(),
            renvor_database::DatabaseError::new(kind).kind().category()
        );
    }

    #[test]
    fn a_missing_row_is_not_found_rather_than_unclassified() {
        assert_eq!(
            classify(&sqlx::Error::RowNotFound),
            DatabaseErrorKind::NotFound
        );
    }

    /// The redaction guarantee, asserted against a driver error carrying a canary.
    ///
    /// This is the property the whole module exists for: whatever the driver said, the value that
    /// leaves this function cannot repeat it.
    #[test]
    fn a_driver_message_never_survives_translation() {
        const CANARY: &str = "password=hunter2 host=db.internal";
        let error = sqlx::Error::Protocol(CANARY.to_owned());
        let translated = classify_error(&error);
        assert!(!translated.to_string().contains("hunter2"));
        assert!(!translated.description().contains("hunter2"));
        assert!(!format!("{translated:?}").contains("hunter2"));
        assert!(!format!("{translated:?}").contains("db.internal"));
    }

    #[test]
    fn an_io_failure_is_a_connection_failure_not_an_internal_defect() {
        let error = sqlx::Error::Io(std::io::Error::other("connection refused to db.internal"));
        let translated = classify_error(&error);
        assert_eq!(translated.kind(), DatabaseErrorKind::ConnectFailed);
        assert!(!format!("{translated:?}").contains("db.internal"));
    }

    #[test]
    fn a_checksum_mismatch_is_distinguished_from_a_dirty_schema() {
        assert_eq!(
            migrate_kind(&sqlx::migrate::MigrateError::VersionMismatch(3)),
            DatabaseErrorKind::MigrationChecksumMismatch
        );
        assert_eq!(
            migrate_kind(&sqlx::migrate::MigrateError::Dirty(3)),
            DatabaseErrorKind::MigrationDirty
        );
    }
}
