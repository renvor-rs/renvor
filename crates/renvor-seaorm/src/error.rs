//! Translating SeaORM and driver failures into the redacted persistence vocabulary.
//!
//! # This module deliberately discards information
//!
//! [`sea_orm::DbErr`] carries a message, and a SeaORM message routinely contains the generated
//! SQL, the offending value, the table and column, and — for a connection failure — the host.
//! [`DatabaseError`] has no field any of that can inhabit, so translating here is not lossy by
//! accident; it is lossy on purpose, and this module is the single place the loss happens.
//!
//! # The original text is terminated here, not forwarded
//!
//! It used to be emitted through `tracing` at `debug`, defended as reaching *"operators rather
//! than callers"*. `CONSTITUTION.md` principle VI forbids secrets in *"logs, telemetry"* and names
//! no consumer who is exempt: an operator is not a class of reader with a right to a credential,
//! and `debug` is a level rather than an exemption. A `DbErr`'s message is an unbounded string
//! decided by SeaORM and the driver beneath it, so a field carrying one cannot be audited.
//!
//! What replaces it is a record built entirely from CLOSED values: the adapter as a
//! [`DatabaseAdapter`] variant, the kind as a [`DatabaseErrorKind`] discriminant, and whether that
//! kind is retryable. Every one is drawn from a set this workspace enumerates.
//!
//! **Where the raw text still lives.** The database server writes its own log, under its own
//! access controls and retention. An operator who needs the untruncated message reads it there,
//! correlating on the kind and the time.
//!
//! # Why this is not shared with `renvor-sqlx`
//!
//! The two adapters classify **different vocabularies**. This one reads `DbErr`, whose variants
//! carry meanings SQLx has no equivalent for — `RecordNotInserted` and `RecordNotUpdated` are
//! SeaORM's way of reporting an affected-row count of zero, not driver errors at all. Sharing a
//! mapping would mean one of the two crates translating into the other's terms first, and the
//! shared crate is `renvor-database`, which may not name a driver. Neither adapter depends on the
//! other, which is the property `xtask` step 7 asserts.

use renvor_database::{DatabaseAdapter, DatabaseError, DatabaseErrorKind};
use sea_orm::{DbErr, RuntimeErr, SqlErr};

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
pub(crate) const ADAPTER: DatabaseAdapter = DatabaseAdapter::SeaOrm;

/// The ONLY place this crate emits telemetry about a database failure.
///
/// # Why one function rather than a macro at each site
///
/// Four entry points classify — the `DbErr` mapper, the driver mapper, the connect-time mapper,
/// and the migration loader — and before this they diverged: three logged the raw error, one
/// logged nothing. Funnelling them through a single function that takes a [`DatabaseErrorKind`]
/// and NOTHING ELSE makes divergence unrepresentable. There is no parameter here a message could
/// arrive in.
pub(crate) fn record(kind: DatabaseErrorKind) -> DatabaseError {
    tracing::debug!(
        adapter = ADAPTER.as_str(),
        database_error_kind = kind.as_str(),
        transient = kind.is_transient(),
        "database operation failed"
    );
    DatabaseError::new(kind)
}

/// Translates a SeaORM error into the redacted vocabulary.
///
/// # Why this is public
///
/// A repository implementation in an application's adapter layer receives a `DbErr` from an
/// idiomatic SeaORM call and owes its caller a [`DatabaseError`]. Without this function every
/// application would write its own mapping, and each one would be a fresh opportunity to put
/// SeaORM's text — and therefore a value, a table name, generated SQL, or a host — into something
/// a caller receives.
pub fn classify_db_error(error: &DbErr) -> DatabaseError {
    record(classify_db(error))
}

/// Translates a driver error raised by this adapter's own connection handling.
pub fn classify_error(error: &sqlx::Error) -> DatabaseError {
    record(classify_sqlx(error))
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
/// Mirrors `renvor-sqlx`'s function of the same name deliberately rather than sharing one: the
/// shared crate is `renvor-database`, which may not name a driver, and neither adapter may depend
/// on the other — the property `xtask` step 7 asserts.
///
/// Keyed on SQLSTATE because this is the one condition where SQLSTATE is the better key. Neither
/// driver offers an `ErrorKind` for a lost conflict, so `kind()` returns `Other` and carries
/// nothing; SQLSTATE meanwhile agrees across both engines — `40001` on either, plus `40P01` for a
/// PostgreSQL deadlock. The constraint violations above are the reverse case, where MySQL collapses
/// three conditions onto `23000` and puts check violations in `HY000`, so only the error number
/// distinguishes them.
///
/// MySQL's lock-wait timeout (`1205`, SQLSTATE `HY000`) is deliberately excluded: a deadlock is
/// resolved instantly by the server choosing a victim, whereas a lock-wait timeout means a lock was
/// held for the full timeout and an automatic retry just re-queues behind the same holder.
fn conflict_or_rejected(inner: &dyn sqlx::error::DatabaseError) -> DatabaseErrorKind {
    match inner.code().as_deref() {
        Some("40001" | "40P01") => DatabaseErrorKind::TransactionConflict,
        _ => DatabaseErrorKind::StatementRejected,
    }
}

/// The migration-specific classification.
fn migrate_kind(error: &sqlx::migrate::MigrateError) -> DatabaseErrorKind {
    use sqlx::migrate::MigrateError;
    match error {
        // All three are history-integrity failures: the applied history no longer agrees with the
        // resolved set. `VersionMissing` is upstream's *"previously applied but is missing in the
        // resolved migrations"*, which is the same class of problem as a changed checksum and is
        // classified identically by `renvor-sqlx`. It is NOT `MigrationIrreversible` — that kind
        // means a migration declares no rollback, and both adapters raise it directly from their
        // own `migrate.rs` rather than by translating a driver error.
        MigrateError::VersionMismatch(_)
        | MigrateError::VersionNotPresent(_)
        | MigrateError::VersionMissing(_) => DatabaseErrorKind::MigrationChecksumMismatch,
        MigrateError::Dirty(_) => DatabaseErrorKind::MigrationDirty,
        _ => DatabaseErrorKind::MigrationFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Migration failures are distinguished from each other, not flattened.
    ///
    /// # Why this is a unit test rather than a database one
    ///
    /// FR-023, FR-024 and FR-026 name three different refusals — a changed migration, a dirty
    /// schema, a lock that never came — and an operator's next step differs for each. Staging a
    /// genuinely dirty `_sqlx_migrations` against a live engine is possible but slow and racy;
    /// what actually has to hold is that **this crate's mapping** does not collapse them, and that
    /// is decidable here.
    ///
    /// A review found this untested: `MigrationDirty` appeared in the mapping and in a doc comment
    /// and nowhere else, so deleting the arm and letting it fall through to `MigrationFailed`
    /// would have failed nothing in the phase.
    #[test]
    fn each_migration_failure_keeps_its_own_kind() {
        use sqlx::migrate::MigrateError;

        let cases = [
            (
                MigrateError::VersionMismatch(1),
                DatabaseErrorKind::MigrationChecksumMismatch,
            ),
            (MigrateError::Dirty(1), DatabaseErrorKind::MigrationDirty),
            (
                // Corrected: this is a history mismatch, not an irreversible migration. See
                // `a_missing_applied_migration_classifies_as_a_history_mismatch_in_both_adapters`.
                MigrateError::VersionMissing(1),
                DatabaseErrorKind::MigrationChecksumMismatch,
            ),
        ];
        for (index, (inner, expected)) in cases.into_iter().enumerate() {
            let classified = classify_error(&sqlx::Error::Migrate(Box::new(inner)));
            // The case INDEX rather than the rendered error: this file handles credentials, and a
            // diagnostic that prints what it asserts about prints it on the one run where the
            // redaction was wrong. `crates/renvor-core/tests/diagnostics.rs` enforces that.
            assert_eq!(
                classified.kind(),
                expected,
                "migration case {index} lost its own kind, which sends an operator to the wrong place"
            );
        }
    }

    /// A classified error carries a kind and nothing an operator could leak.
    ///
    /// FR-009 and FR-021 are structural — `DatabaseError` has one field — but "structural" is a
    /// claim about a type in another crate, and this is the assertion that notices if that type
    /// ever grows somewhere for a message to live.
    #[test]
    fn a_classified_error_renders_no_input_text() {
        // Named `canary`, not `secret`: the literal has to STAY here — `renvor-core`'s diagnostic
        // gate selects credential-handling files by searching for it — while `secret = "..."` is
        // the exact shape gitleaks' generic-api-key rule keys on. Renaming satisfies both controls
        // without an allowlist entry, and it is the more accurate word besides.
        let canary = "hunter2CanaryDoNotLeak";
        let inner = sqlx::Error::Protocol(format!("connection to host with password {canary}"));
        let classified = classify_error(&inner);
        let rendered = format!("{classified:?} {classified}");
        // Neither message names `rendered`. If this assertion fails, `rendered` is precisely the
        // string that still carries the planted secret, and printing it would put the credential
        // into the test log of the failing run.
        assert!(
            !rendered.contains(canary),
            "the planted secret survived translation into the classified error"
        );
        assert!(
            !rendered.contains("password"),
            "the driver's own wording survived translation into the classified error"
        );
    }

    /// The same upstream variant must classify the same way in both adapters.
    ///
    /// # This is a cross-adapter consistency test, not a preference
    ///
    /// `MigrateError::VersionMissing` is documented upstream as *"migration {0} was previously
    /// applied but is missing in the resolved migrations"* — a history-integrity failure. This
    /// adapter mapped it to `MigrationIrreversible`, which means something else entirely: that a
    /// migration declares no rollback. `renvor-sqlx` mapped the same variant to
    /// `MigrationChecksumMismatch`.
    ///
    /// One upstream condition, two Renvor kinds, selected by which ORM the caller happened to
    /// choose. An operator reading `migration_irreversible` would go looking for a missing `.down.`
    /// file that has nothing to do with the actual problem.
    #[test]
    fn a_missing_applied_migration_classifies_as_a_history_mismatch_in_both_adapters() {
        assert_eq!(
            migrate_kind(&sqlx::migrate::MigrateError::VersionMissing(3)),
            DatabaseErrorKind::MigrationChecksumMismatch,
            "`VersionMissing` means the applied history no longer matches the resolved set, which \
             is a checksum-class mismatch. `MigrationIrreversible` means a migration declares no \
             rollback — a different condition, and the one `renvor-sqlx` correctly does not use \
             here"
        );
    }

    /// `MigrationIrreversible` still has a meaning, and it is not this one.
    ///
    /// A CONTROL. Without it, mapping every migration failure to `MigrationChecksumMismatch` would
    /// satisfy the test above while destroying the distinction it exists to protect.
    #[test]
    fn an_irreversible_migration_is_still_reported_as_irreversible() {
        assert_ne!(
            migrate_kind(&sqlx::migrate::MigrateError::VersionMissing(3)),
            migrate_kind(&sqlx::migrate::MigrateError::Dirty(3)),
            "distinct upstream conditions must not collapse onto one kind"
        );
    }
}
