//! The closed, redacted persistence error vocabulary.
//!
//! # Redaction is structural, not disciplined
//!
//! [`renvor_error::ApiErrorCode::detail`] returns `&'static str` so that *"there is no runtime data
//! that can inhabit the type"*. This module applies the same construction to persistence: a
//! [`DatabaseError`] holds a [`DatabaseErrorKind`] and **nothing else**. There is no `message`
//! field, no `source` chain reaching a driver, and no `sql` field.
//!
//! That is deliberate and it costs something: a driver's own diagnostic is discarded at the
//! boundary. It is discarded because the alternative is a type that *can* carry a connection
//! string, and a guarantee that rests on nobody ever putting one there is not a guarantee. An
//! operator who needs the driver's text gets it from tracing, which is a different consumer with
//! different rights — the same split `renvor-error` already makes for HTTP problems.
//!
//! [`renvor_error::ApiErrorCode::detail`]: https://docs.rs/renvor-error

use core::fmt;

/// Every way a persistence operation can fail, as a closed set.
///
/// # Closed, and why that is not a limitation
///
/// A driver has hundreds of error codes. Publishing them would make the driver's vocabulary part of
/// Renvor's public contract, so swapping a driver would be a breaking change for every caller that
/// matched on one. What a *caller* can act on is much smaller: retry, fix the input, fix the
/// deployment, or give up. These variants are that set.
///
/// # There is deliberately no `category()` here
///
/// This enum had one, projecting each kind onto `renvor_core::ErrorCategory`. Its table named
/// five kinds and sent the other seventeen through `_ => ErrorCategory::Internal`.
///
/// `contracts/error-taxonomy.md` C-E1 defines `Internal` as *"resolution work budget
/// exhausted — a defect in the kernel"* and says plainly: *"If an author ever sees `Internal`,
/// the kernel is wrong — not their graph."* A violated unique key, an absent row, and an
/// edited migration are none of those. The projection made the taxonomy's one unambiguous
/// category the routine answer for ordinary database outcomes, and no other category was
/// closer — so the correct fix was to stop projecting, not to re-aim it.
///
/// **[`DatabaseErrorKind`] is the persistence domain's own programmatically matchable
/// classification**, which is what C-E1 requires of an error: a category a caller can match
/// on rather than a message it has to parse. C-E1's `ErrorCategory` table governs **kernel**
/// errors. Two vocabularies with no lossy bridge between them is the honest arrangement; the
/// bridge was the defect.
///
/// Removing it also removed this crate's `renvor-core` dependency, which nothing else used.
/// `xtask` step 7 asserts the absence, with a control, so the coupling cannot return quietly.
///
/// The removal is enforced rather than described:
///
/// ```compile_fail
/// use renvor_database::DatabaseErrorKind;
///
/// let _ = DatabaseErrorKind::UniqueViolation.category();
/// ```
///
/// A `compile_fail` block passes when compilation fails for **any** reason. This one compiles,
/// and differs only in the method:
///
/// ```
/// use renvor_database::DatabaseErrorKind;
///
/// assert_eq!(DatabaseErrorKind::UniqueViolation.as_str(), "unique_violation");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum DatabaseErrorKind {
    /// The pool could not hand out a connection within its acquire deadline.
    ///
    /// Distinct from [`DatabaseErrorKind::ConnectFailed`]: the pool is reachable and simply full,
    /// which is a capacity signal rather than an availability one.
    AcquireTimeout,
    /// A new connection could not be established.
    ConnectFailed,
    /// The operation was cancelled before it completed.
    Cancelled,
    /// A bounded wait elapsed.
    DeadlineExceeded,
    /// The statement was rejected by the database.
    ///
    /// Carries no SQL and no driver text. An application that needs to distinguish *which*
    /// constraint failed declares that in its own domain error, where the vocabulary is its own.
    StatementRejected,
    /// A uniqueness constraint was violated.
    ///
    /// Separated from [`DatabaseErrorKind::StatementRejected`] because it is the one rejection an
    /// application routinely turns into a *caller-correctable* refusal rather than a defect.
    UniqueViolation,
    /// A foreign-key constraint was violated.
    ForeignKeyViolation,
    /// A not-null constraint was violated.
    ///
    /// Both drivers already distinguish this from a generic rejection — `sqlx-postgres` from
    /// SQLSTATE `23502`, `sqlx-mysql` from error number `1048` — so folding it into
    /// [`DatabaseErrorKind::StatementRejected`] would discard information the driver had already
    /// recovered.
    NotNullViolation,
    /// A check constraint was violated.
    ///
    /// # MySQL reports this outside the integrity class
    ///
    /// PostgreSQL gives check violations SQLSTATE `23514`, inside class 23. MySQL reports error
    /// `3819` with SQLSTATE **`HY000`**, the general-error class. Both were measured against the
    /// pinned images. That asymmetry is why classification reads the driver's error *number* rather
    /// than SQLSTATE — SQLSTATE cannot see this condition on MySQL at all.
    CheckViolation,
    /// The transaction lost a concurrency conflict and may be retried.
    ///
    /// Covers a serialization failure and a deadlock, which both engines report as SQLSTATE `40001`
    /// — PostgreSQL adds `40P01` for a deadlock specifically. Measured: a crossed-lock deadlock on
    /// `mysql:8.4.11` returns `ERROR 1213 (40001)` to exactly one of the two sessions while the
    /// other commits.
    ///
    /// **A lock-wait timeout is deliberately NOT this kind.** MySQL reports that as `1205` with
    /// SQLSTATE `HY000`: the server resolves a deadlock instantly by choosing a victim, whereas a
    /// lock-wait timeout means something held a lock for the whole timeout, which usually wants an
    /// operator rather than an automatic retry.
    TransactionConflict,
    /// The row a query required was absent.
    NotFound,
    /// A transaction could not be committed.
    CommitFailed,
    /// A transaction could not be rolled back.
    ///
    /// **A defect, not a refusal.** A failed rollback means the connection's state is unknown, so
    /// it is discarded rather than returned to the pool.
    RollbackFailed,
    /// A migration's recorded checksum does not match the migration now on disk.
    MigrationChecksumMismatch,
    /// A previous migration run failed part-way and left the schema in an unknown state.
    MigrationDirty,
    /// A rollback was requested for a migration declared irreversible.
    MigrationIrreversible,
    /// A migration failed to apply.
    MigrationFailed,
    /// The migration lock could not be taken within its deadline.
    ///
    /// Renvor bounds this. The driver does not: `sqlx-mysql` issues `GET_LOCK(?, -1)`, an infinite
    /// wait, and an unbounded wait is prohibited by constitution principle VI.
    MigrationLockTimeout,
    /// The readiness check did not pass.
    NotReady,
    /// A value could not be represented in, or read back from, the database's type system.
    TypeMismatch,
    /// The pool is closed.
    PoolClosed,
    /// An unclassified failure. **A defect.**
    Unclassified,
}

impl DatabaseErrorKind {
    /// Every kind, in the order the published contract lists them.
    ///
    /// The length assertion below moves with this array, so an edit that adds a variant without
    /// updating the contract still fails loudly — the same mechanism `renvor-error`'s registry and
    /// `renvor-core`'s `ErrorCategory::ALL` already use.
    pub const ALL: [Self; 22] = [
        Self::AcquireTimeout,
        Self::ConnectFailed,
        Self::Cancelled,
        Self::DeadlineExceeded,
        Self::StatementRejected,
        Self::UniqueViolation,
        Self::ForeignKeyViolation,
        Self::NotNullViolation,
        Self::CheckViolation,
        Self::TransactionConflict,
        Self::NotFound,
        Self::CommitFailed,
        Self::RollbackFailed,
        Self::MigrationChecksumMismatch,
        Self::MigrationDirty,
        Self::MigrationIrreversible,
        Self::MigrationFailed,
        Self::MigrationLockTimeout,
        Self::NotReady,
        Self::TypeMismatch,
        Self::PoolClosed,
        Self::Unclassified,
    ];

    /// The stable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AcquireTimeout => "acquire_timeout",
            Self::ConnectFailed => "connect_failed",
            Self::Cancelled => "cancelled",
            Self::DeadlineExceeded => "deadline_exceeded",
            Self::StatementRejected => "statement_rejected",
            Self::UniqueViolation => "unique_violation",
            Self::ForeignKeyViolation => "foreign_key_violation",
            Self::NotNullViolation => "not_null_violation",
            Self::CheckViolation => "check_violation",
            Self::TransactionConflict => "transaction_conflict",
            Self::NotFound => "not_found",
            Self::CommitFailed => "commit_failed",
            Self::RollbackFailed => "rollback_failed",
            Self::MigrationChecksumMismatch => "migration_checksum_mismatch",
            Self::MigrationDirty => "migration_dirty",
            Self::MigrationIrreversible => "migration_irreversible",
            Self::MigrationFailed => "migration_failed",
            Self::MigrationLockTimeout => "migration_lock_timeout",
            Self::NotReady => "not_ready",
            Self::TypeMismatch => "type_mismatch",
            Self::PoolClosed => "pool_closed",
            Self::Unclassified => "unclassified",
        }
    }

    /// The safe, occurrence-independent description.
    ///
    /// Returns `&'static str` for the reason [`crate::error`] gives: a `String` would make it
    /// *possible* to interpolate a connection string, and the guarantee would then rest on nobody
    /// ever doing so.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::AcquireTimeout => {
                "No connection became available within the configured acquire deadline."
            }
            Self::ConnectFailed => "A connection to the database could not be established.",
            Self::Cancelled => "The operation was cancelled before it completed.",
            Self::DeadlineExceeded => "The operation exceeded its configured deadline.",
            Self::StatementRejected => "The database rejected the statement.",
            Self::UniqueViolation => "A value already exists that must be unique.",
            Self::ForeignKeyViolation => "A referenced row does not exist.",
            Self::NotNullViolation => "A required value was absent.",
            Self::CheckViolation => "A value failed a constraint the schema declares.",
            Self::TransactionConflict => {
                "The transaction conflicted with another and was not committed."
            }
            Self::NotFound => "The requested row does not exist.",
            Self::CommitFailed => "The transaction could not be committed.",
            Self::RollbackFailed => "The transaction could not be rolled back.",
            Self::MigrationChecksumMismatch => {
                "An applied migration's content has changed since it was applied."
            }
            Self::MigrationDirty => "A previous migration run did not complete.",
            Self::MigrationIrreversible => "This migration declares no rollback.",
            Self::MigrationFailed => "A migration could not be applied.",
            Self::MigrationLockTimeout => {
                "The migration lock was not acquired within its deadline."
            }
            Self::NotReady => "The database did not pass its readiness check.",
            Self::TypeMismatch => "A value could not be represented in the database's type system.",
            Self::PoolClosed => "The connection pool is closed.",
            Self::Unclassified => "The operation could not be completed.",
        }
    }

    /// Whether retrying the same operation unchanged could plausibly succeed.
    ///
    /// # This is advice, not a promise
    ///
    /// It says the failure is *transient in kind*, not that a retry will work. Retries remain
    /// bounded and observable at the call site, per constitution principle IV.
    #[must_use]
    pub const fn is_transient(self) -> bool {
        matches!(
            self,
            Self::AcquireTimeout
                | Self::ConnectFailed
                | Self::DeadlineExceeded
                | Self::MigrationLockTimeout
                | Self::NotReady
                // The server itself says so: MySQL's text for 1213 is literally "try restarting
                // transaction". A lost conflict is the one rejection that is transient BY
                // CONSTRUCTION — nothing about the statement was wrong.
                | Self::TransactionConflict
        )
    }
}

impl fmt::Display for DatabaseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A persistence failure.
///
/// # It has exactly one field, and that is the point
///
/// See the module documentation. There is no field a connection string, a credential, a SQL
/// statement, or a driver message can inhabit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DatabaseError {
    kind: DatabaseErrorKind,
}

impl DatabaseError {
    /// Constructs an error of this kind.
    #[must_use]
    pub const fn new(kind: DatabaseErrorKind) -> Self {
        Self { kind }
    }

    /// The kind.
    #[must_use]
    pub const fn kind(self) -> DatabaseErrorKind {
        self.kind
    }

    /// The safe description.
    #[must_use]
    pub const fn description(self) -> &'static str {
        self.kind.description()
    }
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind.description())
    }
}

impl core::error::Error for DatabaseError {}

impl From<DatabaseErrorKind> for DatabaseError {
    fn from(kind: DatabaseErrorKind) -> Self {
        Self::new(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_is_listed_exactly_once() {
        let mut seen = DatabaseErrorKind::ALL.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(
            seen.len(),
            DatabaseErrorKind::ALL.len(),
            "`ALL` contains a duplicate"
        );
    }

    #[test]
    fn every_name_is_unique() {
        let mut names: Vec<&str> = DatabaseErrorKind::ALL.iter().map(|k| k.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), DatabaseErrorKind::ALL.len());
    }

    /// The redaction guarantee, asserted rather than assumed.
    ///
    /// `Display` and `description` can only ever return the constant, so no canary can appear.
    #[test]
    fn no_description_can_carry_runtime_data() {
        const CANARY: &str = "postgres://user:hunter2@db.internal:5432/app";
        for kind in DatabaseErrorKind::ALL {
            let error = DatabaseError::new(kind);
            assert!(!error.to_string().contains("hunter2"));
            assert!(!error.description().contains(CANARY));
            // The description is a compile-time constant, so it cannot contain a password.
            assert!(!error.description().contains("://"));
        }
    }

    #[test]
    fn transient_kinds_are_the_ones_a_retry_could_help() {
        assert!(DatabaseErrorKind::AcquireTimeout.is_transient());
        assert!(!DatabaseErrorKind::UniqueViolation.is_transient());
        assert!(!DatabaseErrorKind::MigrationChecksumMismatch.is_transient());
    }
}
