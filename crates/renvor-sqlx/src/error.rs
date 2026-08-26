//! Translating driver failures into the redacted persistence vocabulary.
//!
//! # This module deliberately discards information
//!
//! `sqlx::Error` carries a driver message, and a driver message routinely contains the offending
//! value, the table and column, and — for a connection failure — the host. `DatabaseError` has no
//! field any of that can inhabit, so translating here is not lossy by accident; it is lossy on
//! purpose, and this module is the single place the loss happens.
//!
//! # The driver's text is terminated here, not forwarded
//!
//! It used to be emitted through `tracing` at `debug`, defended as reaching *"operators rather
//! than callers"*. `CONSTITUTION.md` principle VI forbids secrets in *"logs, telemetry"* and names
//! no consumer who is exempt: an operator is not a class of reader with a right to a credential,
//! and `debug` is a level rather than an exemption. A driver message is an unbounded third-party
//! string, so a field carrying one cannot be audited — its contents are decided upstream.
//!
//! What replaces it is a record built entirely from CLOSED values: the adapter as a
//! [`DatabaseAdapter`] variant, the kind as a [`DatabaseErrorKind`] discriminant, and whether that
//! kind is retryable. Every one of those is drawn from a set this workspace enumerates, so no
//! caller-, server-, or value-derived text can inhabit them.
//!
//! **Where the raw text still lives.** The database server writes its own log, under the server's
//! own access controls and retention. An operator who needs the untruncated message reads it
//! there, correlating on the kind and the time. That is a deliberate trade: Renvor's telemetry
//! becomes safe to ship anywhere, and the unsafe detail stays where it is already protected.

use renvor_database::{DatabaseAdapter, DatabaseError, DatabaseErrorKind};
use sqlx::error::ErrorKind;

/// This crate's identity, in telemetry and in startup diagnostics.
///
/// A [`DatabaseAdapter`] rather than a name: both consumers' adapter fields are a closed enum
/// precisely so that no value derived from configuration can reach them. A `&'static str` here
/// would have been one `Box::leak` away from rendering a DSN.
///
/// Declared once for the whole crate rather than per module. `provider.rs` used to carry its own
/// copy, which is a divergence vector of exactly the kind this module was corrected for: two
/// constants naming the same adapter can disagree, and then a startup diagnostic and a telemetry
/// record would attribute the same failure to different crates.
pub(crate) const ADAPTER: DatabaseAdapter = DatabaseAdapter::Sqlx;

/// The ONLY place this crate emits telemetry about a database failure.
///
/// # Why one function rather than a macro at each site
///
/// Three public entry points classify — the general mapper, the connect-time mapper, and the
/// migration loader — and before this they diverged: two logged the raw error, one logged nothing.
/// Funnelling them through a single function that takes a [`DatabaseErrorKind`] and NOTHING ELSE
/// makes divergence unrepresentable. There is no parameter here a driver message could arrive in.
pub(crate) fn record(kind: DatabaseErrorKind) -> DatabaseError {
    tracing::debug!(
        adapter = ADAPTER.as_str(),
        database_error_kind = kind.as_str(),
        transient = kind.is_transient(),
        "database operation failed"
    );
    DatabaseError::new(kind)
}

/// Translates a driver error into the redacted vocabulary.
///
/// # Why this is public
///
/// A repository implementation in an application's adapter layer receives a `sqlx::Error` and owes
/// its caller a [`DatabaseError`]. Without this function every application would write its own
/// mapping, and each one would be a fresh opportunity to put the driver's text — and therefore a
/// value, a table name, or a host — into something a caller receives.
///
/// # Why the original is neither returned nor logged
///
/// A caller that received the driver's text would be receiving a string this crate cannot bound,
/// cannot redact, and does not control the shape of. Neither can a telemetry sink, which is why
/// the earlier version of this function — which logged it at `debug` — was a leak with a level
/// attached rather than a compromise. See this module's own note on where the raw text does live.
pub fn classify_error(error: &sqlx::Error) -> DatabaseError {
    record(classify(error))
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
        // Through `record` rather than `DatabaseError::new`: this arm used to be the one path that
        // emitted no telemetry at all, so a refused handshake was invisible where every other
        // failure was recorded.
        sqlx::Error::Database(_) => record(DatabaseErrorKind::ConnectFailed),
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
            ErrorKind::NotNullViolation => DatabaseErrorKind::NotNullViolation,
            ErrorKind::CheckViolation => DatabaseErrorKind::CheckViolation,
            _ => conflict_or_rejected(inner.as_ref()),
        },
        sqlx::Error::Migrate(inner) => migrate_kind(inner),
        _ => DatabaseErrorKind::Unclassified,
    }
}

/// Separates a lost concurrency conflict from an ordinary rejection, by SQLSTATE.
///
/// # Why this one condition is keyed on SQLSTATE when the others are not
///
/// The constraint violations above are keyed on the driver's `kind()`, which reads the server's
/// error **number**, because SQLSTATE cannot distinguish them on MySQL: it reports unique,
/// foreign-key and not-null all as the generic `23000`, and a check violation as `HY000`, outside
/// the integrity class entirely.
///
/// A lost conflict is the mirror image. Neither driver offers an `ErrorKind` for it — the string
/// `40001` appears nowhere in `sqlx-postgres` or `sqlx-mysql` — so `kind()` returns
/// `ErrorKind::Other` and carries no information. SQLSTATE, however, agrees across both engines
/// here: PostgreSQL raises `40001` for a serialization failure and `40P01` for a deadlock, and
/// MySQL raises `40001` for a deadlock. Measured against `mysql:8.4.11`: a crossed-lock deadlock
/// returns `ERROR 1213 (40001)` to exactly one of two sessions while the other commits.
///
/// # What is deliberately excluded
///
/// **MySQL's lock-wait timeout** — error `1205`, SQLSTATE `HY000` — is not folded in. The server
/// resolves a deadlock instantly by choosing a victim, so retrying is the correct response. A
/// lock-wait timeout means something held a lock for the full `innodb_lock_wait_timeout` (50s on
/// the pinned image); retrying that automatically re-queues behind the same holder.
///
/// **PostgreSQL's exclusion violation** (`ErrorKind::ExclusionViolation`) also lands here and stays
/// `StatementRejected`. It has no MySQL equivalent, so promoting it to a kind of its own would put
/// a PostgreSQL-only concept into a vocabulary shared by both engines.
fn conflict_or_rejected(inner: &dyn sqlx::error::DatabaseError) -> DatabaseErrorKind {
    match inner.code().as_deref() {
        // 40001 serialization_failure (both engines) | 40P01 deadlock_detected (PostgreSQL).
        Some("40001" | "40P01") => DatabaseErrorKind::TransactionConflict,
        _ => DatabaseErrorKind::StatementRejected,
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

    /// A closed pool is its own kind, distinguishable from every other refusal.
    ///
    /// This test used to compare `kind.category()` against the same value reached through
    /// `DatabaseError`, which asserted that a projection agreed with itself. The projection was
    /// removed in Phase 008 — see `DatabaseErrorKind`'s own note on why `Internal` was the wrong
    /// answer for a database outcome — and what remains is the claim the name makes: shutdown is
    /// not folded into `StatementRejected` or `Unclassified`.
    #[test]
    fn a_closed_pool_reports_as_shutting_down_rather_than_as_a_defect() {
        let kind = classify(&sqlx::Error::PoolClosed);
        assert_eq!(kind, DatabaseErrorKind::PoolClosed);
        assert_ne!(kind, DatabaseErrorKind::Unclassified);
        assert_ne!(kind, DatabaseErrorKind::StatementRejected);
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
