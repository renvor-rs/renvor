//! Transport-independent persistence ports for the Renvor framework.
//!
//! # This crate names no database
//!
//! There is no SQLx type here, no Axum type, no `Row`, and no connection. What is here is the
//! **shape** an application service depends on: a transaction it opens and closes explicitly, a
//! bounded pool it configures, a migration contract it can assert against, and a page it can
//! resume. `renvor-sqlx` is where a driver appears, and the dependency points from that adapter
//! inward to these ports — never the reverse.
//!
//! `xtask` step 7 asserts the absence of every driver crate from this crate's graph, with a
//! positive control proving the same query finds them where they *are* selected. An architecture
//! rule that is only written down is a rule somebody edits around.
//!
//! # The application declares its own repositories
//!
//! Renvor deliberately does **not** publish a blanket `Repository<T>` with `find`, `insert`,
//! `update`, and `delete`. That would be an ORM boundary, and constitution principle III forbids
//! creating an ORM. An application declares repository traits in **its own** domain language:
//!
//! ```rust,ignore
//! // In the application layer. No SQLx, no Axum, no `renvor_sqlx`.
//! pub trait PostRepository {
//!     fn create(&mut self, draft: NewPost)
//!         -> impl Future<Output = Result<Post, DatabaseError>> + Send;
//! }
//! ```
//!
//! and implements them, in an adapter module, for the concrete unit of work `renvor-sqlx` hands
//! back. The application service is generic over [`Database`] and never learns which driver ran.
//!
//! # Transactions are explicit in both directions
//!
//! [`Database::begin`] is the only way to start one, and [`UnitOfWork::commit`] is the only way to
//! finish one successfully. `commit` takes `self` by value, so using a unit of work after
//! committing it is a **compile** error rather than a runtime one.
//!
//! Dropping a unit of work without committing never writes. What an adapter must guarantee about
//! the connection it was holding — including that a cancelled transaction may not cost the pool
//! capacity — is stated as a contract on [`UnitOfWork`] rather than inherited from whichever driver
//! happens to be underneath.

use core::future::Future;

// The closed-enum declaration macro. Private module; `#[macro_export]` publishes the macro
// itself at the crate root, which is where `closed_named_enum!` is documented.
mod closed_enum;

pub mod error;
pub mod migrate;
pub mod page;
pub mod pool;
pub mod seed;
pub mod startup;

pub use error::{DatabaseError, DatabaseErrorKind};
pub use migrate::{
    MigrationOutcome, MigrationPolicy, MigrationReport, MigrationSettings, MigrationStep,
    Reversibility,
};
pub use page::{Keyset, KeysetError, OrderBy, OrderTerm, Page, SortAllowlist};
pub use pool::{ConnectionString, PoolSettings};
/// Re-exported from `renvor-validation` rather than redefined.
///
/// A sort direction is already part of contract C-15's collection-read vocabulary. Declaring a
/// second one here would give the workspace two `Direction` enums that agree by coincidence, and
/// the conversion between them would be the thing that eventually disagreed.
pub use renvor_validation::collection::Direction;
pub use seed::{
    APPLIED, CREATE_LEDGER, Idempotence, SeedDeclaration, SeedReport, SeedScope, SqlSeed,
};
pub use startup::{DatabaseAdapter, StartupDiagnostic, StartupPhase};

crate::closed_named_enum! {
    /// Which database an adapter is talking to.
    ///
    /// # Safe to report, unlike everything else about a connection
    ///
    /// A diagnostic needs to say *something*. The database **kind** identifies the driver without
    /// naming a host, a user, or a password, so it is the one connection fact that may appear in
    /// an error. See [`ConnectionString`] for why the rest may not.
    ///
    /// `as_str` returns the stable name, as it appears in a project manifest and in CLI output.
    ///
    /// # Declared rather than written out
    ///
    /// Through [`closed_named_enum`], for the same reason as
    /// [`DatabaseAdapter`]: this type is a rendered field of [`StartupDiagnostic`], so a variant
    /// that existed but was absent from `ALL` would be invisible to the exhaustive redaction
    /// enumeration that reads `ALL`.
    pub enum DatabaseKind {
        /// PostgreSQL.
        Postgres => "postgres",
        /// MySQL.
        MySql => "mysql",
    }
}

impl DatabaseKind {
    /// Parses a kind name.
    ///
    /// Returns `None` rather than defaulting. A misspelled database name that silently became
    /// PostgreSQL is the class of silent fallback constitution principle IV prohibits.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "postgres" => Some(Self::Postgres),
            "mysql" => Some(Self::MySql),
            _ => None,
        }
    }

    /// Quotes an identifier the way this database expects.
    ///
    /// # The input is always a compile-time constant
    ///
    /// Every caller passes a `&'static str` drawn from a [`SortAllowlist`]. This function exists
    /// because the *syntax* differs per database, not because the *value* is ever untrusted.
    ///
    /// The doubling below is defence in depth rather than the primary control: an allowlisted
    /// constant contains no quote character, and if one ever did, doubling is the escape both
    /// databases define.
    #[must_use]
    pub fn quote_identifier(self, identifier: &str) -> String {
        match self {
            Self::Postgres => format!("\"{}\"", identifier.replace('"', "\"\"")),
            Self::MySql => format!("`{}`", identifier.replace('`', "``")),
        }
    }

    /// The placeholder for the `index`-th bound parameter, one-based.
    ///
    /// PostgreSQL numbers its placeholders and MySQL does not, which is the difference that makes
    /// a shared statement builder need this rather than a constant.
    #[must_use]
    pub fn placeholder(self, index: usize) -> String {
        match self {
            Self::Postgres => format!("${index}"),
            Self::MySql => "?".to_owned(),
        }
    }
}

/// Somewhere a statement can run — a pooled connection, or an open transaction.
///
/// # Why this is not a `Repository<T>` trait
///
/// A blanket `Repository<T>` with `find`/`insert`/`update`/`delete` would be an ORM boundary, and
/// constitution principle III forbids building one. What a repository actually needs from Renvor is
/// narrower than that: somewhere to run its own SQL, and knowledge of which dialect to write. Each
/// application declares its **own** repository trait in its own domain language and takes an
/// `&mut impl Executor`. Renvor supplies the transaction machinery, not the vocabulary.
///
/// # Why the same repository runs inside and outside a transaction
///
/// Both a pooled connection and a [`UnitOfWork`] implement this, so a repository written once
/// against `&mut impl Executor` runs either way. Without that, every repository would need two
/// implementations, and the second would drift.
///
/// # What it deliberately does not expose
///
/// No statement type, no row type, no driver handle. Those live in the adapter, where a driver is
/// already visible. That is what makes FR-001 mechanically checkable rather than a promise: an
/// application trait bounded on this **cannot** name a SQLx type, because it was never handed one.
pub trait Executor: Send {
    /// Which database this executor speaks to.
    ///
    /// A repository needs this to choose dialect-specific SQL from its own closed allowlist — the
    /// one place a caller-visible difference between the two backends is legitimate.
    fn kind(&self) -> DatabaseKind;

    /// Whether this executor is inside an explicit transaction.
    ///
    /// Lets a repository refuse an operation that is only safe transactionally, rather than
    /// discovering the absence of a transaction from a partially applied write.
    fn in_transaction(&self) -> bool;
}

/// An open transaction, owned by the application service that began it.
///
/// # There is no commit-on-drop path
///
/// Only [`UnitOfWork::commit`] commits. A unit of work that goes out of scope for **any** reason —
/// an early return, a `?`, a panic, a cancelled future — does not write.
///
/// # Nesting is not supported, and cannot be attempted
///
/// There is no `begin` on this trait, so a nested transaction is not merely refused at runtime — it
/// is unrepresentable. Calling [`Database::begin`] again while a unit of work is live yields a
/// **different connection** carrying a **separate** transaction, not a nested one; that distinction
/// is asserted, because a nested begin quietly flattened into the outer transaction is the failure
/// mode this rules out.
///
/// SQLx supports savepoints. Renvor does not expose them in Phase 006, because a savepoint API
/// whose failure semantics are untested is worse than none. Recorded as a limitation with a target
/// phase rather than half-shipped.
///
/// # What an adapter must guarantee when one is dropped
///
/// Dropping a future does not cancel the statement it started: the client stops waiting, and the
/// server keeps working. An adapter therefore cannot assume a dropped unit of work leaves a
/// reusable connection behind, and these are the properties it must provide instead:
///
/// - **No uncommitted row is ever visible to another connection.** This holds from the first
///   statement onward and does not depend on any rollback having run.
/// - **Dropping must not cost the pool capacity.** Whatever an adapter does with the connection —
///   return it, discard it, replace it — the pool must regain its full configured capacity within
///   a bounded, deterministic time. A connection that can never be handed out again, while still
///   counted against the configured maximum, is a defect and not a slow path.
/// - **Cleanup must be bounded.** No unbounded wait, and nothing that can make shutdown hang.
/// - **A committed or explicitly rolled-back unit of work leaves a reusable connection.** The
///   capacity guarantee above must not be met by discarding every connection unconditionally.
///
/// The server-side transaction may be cleaned up asynchronously, so a test that drops a unit of
/// work and immediately inspects server-side transaction state may still see it open. Renvor
/// asserts the properties above and documents this one, rather than claiming a synchronous
/// rollback no adapter performs.
pub trait UnitOfWork: Send {
    /// Commits.
    ///
    /// Takes `self` by value, so a use after commit does not compile.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::CommitFailed`] when the database refused the commit.
    fn commit(self) -> impl Future<Output = Result<(), DatabaseError>> + Send;

    /// Rolls back.
    ///
    /// Explicit rollback exists so that "undo this deliberately" and "undo this because something
    /// went wrong" are different statements in the source.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::RollbackFailed`], which is a defect rather than a refusal: the
    /// connection's state is then unknown and it is discarded rather than reused.
    fn rollback(self) -> impl Future<Output = Result<(), DatabaseError>> + Send;
}

/// A configured, bounded connection to one database.
///
/// # Implemented by adapters, depended on by services
///
/// An application service takes `&impl Database` and never learns whether it is talking to
/// PostgreSQL or MySQL. That is what makes the same contract suite executable against both.
pub trait Database: Send + Sync {
    /// The concrete transaction this database hands out.
    type UnitOfWork<'c>: UnitOfWork
    where
        Self: 'c;

    /// Which database this is.
    fn kind(&self) -> DatabaseKind;

    /// Begins a transaction.
    ///
    /// # This is the only way one starts
    ///
    /// No repository call begins a transaction implicitly. `PLAN.md` §12 requires transaction
    /// boundaries to be *"explicit in service code and tests"*, and a boundary that can appear
    /// without a line of code naming it is not explicit.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::AcquireTimeout`] when the pool had no connection within its deadline,
    /// [`DatabaseErrorKind::PoolClosed`] during shutdown, or
    /// [`DatabaseErrorKind::ConnectFailed`].
    fn begin(&self) -> impl Future<Output = Result<Self::UnitOfWork<'_>, DatabaseError>> + Send;

    /// Runs the readiness check.
    ///
    /// # Readiness is not liveness
    ///
    /// This performs a real round-trip, bounded by the configured acquire and connect deadlines.
    /// Returning `Ok` from a check that never touched the database would let readiness be reported
    /// before the database could serve a request, which principle IV forbids.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::NotReady`], or a transient kind describing why the round-trip failed.
    fn check(&self) -> impl Future<Output = Result<(), DatabaseError>> + Send;

    /// Closes the pool within a bounded drain.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::DeadlineExceeded`] when connections did not return in time. A forced
    /// close is reported as an error rather than as a successful shutdown, per principle IV:
    /// *"Timeouts and forced termination are visible errors, never successful shutdowns."*
    fn close(&self) -> impl Future<Output = Result<(), DatabaseError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_quoting_matches_each_database() {
        assert_eq!(DatabaseKind::Postgres.quote_identifier("id"), "\"id\"");
        assert_eq!(DatabaseKind::MySql.quote_identifier("id"), "`id`");
    }

    #[test]
    fn a_quote_character_in_an_identifier_is_doubled_not_passed_through() {
        assert_eq!(
            DatabaseKind::Postgres.quote_identifier("a\"b"),
            "\"a\"\"b\""
        );
        assert_eq!(DatabaseKind::MySql.quote_identifier("a`b"), "`a``b`");
    }

    /// The classic identifier-injection payload, against both quoting rules.
    ///
    /// This is defence in depth: an allowlisted column is a `&'static str` and can never be this
    /// value. The assertion exists so that a future change which widened the input type fails here
    /// rather than in production.
    #[test]
    fn an_injection_payload_cannot_escape_either_quoting_rule() {
        let payload = "id\"; DROP TABLE posts; --";
        let quoted = DatabaseKind::Postgres.quote_identifier(payload);
        assert!(quoted.starts_with('"') && quoted.ends_with('"'));
        // Every interior quote is doubled, so no odd-length run of quotes can terminate it early.
        assert_eq!(quoted.matches('"').count() % 2, 0);

        let payload = "id`; DROP TABLE posts; --";
        let quoted = DatabaseKind::MySql.quote_identifier(payload);
        assert!(quoted.starts_with('`') && quoted.ends_with('`'));
        assert_eq!(quoted.matches('`').count() % 2, 0);
    }

    #[test]
    fn placeholders_differ_per_database() {
        assert_eq!(DatabaseKind::Postgres.placeholder(1), "$1");
        assert_eq!(DatabaseKind::Postgres.placeholder(7), "$7");
        assert_eq!(DatabaseKind::MySql.placeholder(1), "?");
        assert_eq!(DatabaseKind::MySql.placeholder(7), "?");
    }

    #[test]
    fn an_unknown_database_name_is_refused_rather_than_defaulted() {
        assert_eq!(DatabaseKind::parse("sqlite"), None);
        assert_eq!(DatabaseKind::parse("Postgres"), None);
        assert_eq!(DatabaseKind::parse(""), None);
    }

    #[test]
    fn kind_names_round_trip() {
        for kind in DatabaseKind::ALL {
            assert_eq!(DatabaseKind::parse(kind.as_str()), Some(kind));
        }
    }
}
