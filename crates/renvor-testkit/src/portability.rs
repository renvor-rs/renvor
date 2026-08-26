//! The cross-database portability contract, executed against every engine.
//!
//! # Why differences are asserted rather than hidden
//!
//! `PLAN.md` §10.1 requires that *"identifiers, timestamps, isolation levels, upserts, pagination
//! order, JSON capabilities, and migration syntax require cross-database contract tests"*, and
//! that database-specific behaviour be *"isolated behind adapters and documented"*.
//!
//! Documented is not the same as hidden. Some of these differences **cannot** be papered over
//! without lying to the caller — MySQL's `TIMESTAMP` genuinely stops in 2038, and no adapter can
//! invent the missing years. So each assertion below states the value **each engine is expected to
//! produce**, keyed on [`renvor_database::DatabaseKind`], and fails if either engine moves.
//!
//! That is the difference between a portability guide and a portability *contract*: the guide says
//! what the engines do, and this fails the build when the guide goes stale.
//!
//! # This is the engine axis, deliberately, and here is why that is not a gap
//!
//! Every assertion here measures what the **server** does with a statement. ADR-0022 has both ORM
//! choices migrate on SQLx's engine, and neither ORM changes how MySQL resolves
//! `ON DUPLICATE KEY` or where PostgreSQL puts a NULL. So the facts are properties of the engine,
//! and the ORM axis would multiply the runs without multiplying the evidence.
//!
//! The suite is nevertheless executed from **both** adapter crates, because "both adapters can
//! reach these facts at all" is itself worth checking, and because a four-row census that skipped
//! two rows here would be a census with an exception in it.
//!
//! Error **classification** across the ORM axis is a different question, and it is measured
//! separately in each adapter's `error_classification` suite.

use core::future::Future;

use renvor_database::{Database, DatabaseErrorKind, DatabaseKind};

/// Which end of an ascending sort an engine puts NULLs at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NullPlacement {
    /// NULLs come before every value.
    First,
    /// NULLs come after every value.
    Last,
}

/// What an engine did with an upsert whose conflict landed on a unique key it did not name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnnamedKeyUpsert {
    /// Refused, with the kind Renvor classified the refusal as.
    Refused(DatabaseErrorKind),
    /// Updated a row the statement never named. Carries the identifier of the row it rewrote.
    RewroteAnotherRow(i64),
}

/// What an engine did with an identifier longer than it allows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OversizedIdentifier {
    /// Refused outright.
    Refused,
    /// Accepted, and created the object under this shortened name instead.
    Truncated(String),
}

/// The engine probes the portability contract needs.
///
/// Every method is one measurement. They are deliberately *questions about the server* rather than
/// operations on a domain: the answers differ between engines, which is the whole subject.
pub trait PortabilityFixture: Sync {
    /// The adapter's database type.
    type Database: Database;

    /// The connected database.
    fn database(&self) -> &Self::Database;

    /// Values `2`, `NULL`, `1` returned by an unqualified `ORDER BY … ASC`.
    fn nulls_ascending(&self) -> impl Future<Output = Vec<Option<i64>>> + Send;

    /// Whether a `CREATE TABLE` issued inside an explicit transaction survives its rollback.
    fn ddl_survives_rollback(&self) -> impl Future<Output = bool> + Send;

    /// The sub-second digits the engine gives a zone-naive timestamp column declared with no
    /// explicit precision, read back from `information_schema`.
    fn default_timestamp_digits(&self) -> impl Future<Output = u32> + Send;

    /// The engine's answer to an upsert that names one unique key while conflicting on another.
    fn upsert_on_unnamed_key(&self) -> impl Future<Output = UnnamedKeyUpsert> + Send;

    /// Whether a second read inside one open transaction observes another session's commit.
    fn repeated_read_sees_concurrent_commit(&self) -> impl Future<Output = bool> + Send;

    /// What the engine does with an identifier one character past its limit.
    fn oversized_identifier(&self) -> impl Future<Output = OversizedIdentifier> + Send;

    /// `{"b":1,"a":2,"a":3}` stored in the engine's **recommended** JSON type and read back.
    ///
    /// Recommended, not default: PostgreSQL offers two, and only one of them is portable. See
    /// [`json_normalisation_agrees_across_engines`].
    fn json_round_trip(&self) -> impl Future<Output = String> + Send;

    /// The same document stored in a type the contract advises **against**, or `None` if the
    /// engine has no such type.
    ///
    /// The control. Without it, an assertion that the recommended types agree could be satisfied
    /// by an engine that had only one type and no choice to get wrong.
    fn json_round_trip_unadvised(&self) -> impl Future<Output = Option<String>> + Send;
}

/// NULLs sort to opposite ends, and each engine is pinned to the end it actually uses.
///
/// # Why this is a contract and not a footnote
///
/// Contract C-15 makes a cursor resumable by ordering on a total key. An `ORDER BY` over a
/// **nullable** column is not total in the same way on both engines: the same page boundary falls
/// in a different place, so a cursor minted on one engine resumes elsewhere on the other. An
/// application that never learns this writes a pager that is correct on its development database.
pub async fn nulls_sort_to_the_documented_end<F: PortabilityFixture>(fixture: &F) {
    let observed = fixture.nulls_ascending().await;
    let placement = match observed.first() {
        Some(None) => NullPlacement::First,
        Some(Some(_)) => NullPlacement::Last,
        None => panic!("the NULL-ordering probe returned no rows, so it measured nothing"),
    };

    let expected = match fixture.database().kind() {
        DatabaseKind::Postgres => NullPlacement::Last,
        DatabaseKind::MySql => NullPlacement::First,
        // `DatabaseKind` is `#[non_exhaustive]` on purpose. A third engine must record its own
        // measured answer here rather than silently inherit one of these two.
        kind => panic!(
            "{kind:?} has never been measured against this contract. Add its answer, with the \
             measurement that produced it"
        ),
    };
    assert_eq!(
        placement,
        expected,
        "{:?} placed NULLs {placement:?} in an ascending sort, and the contract records \
         {expected:?}. Either the engine changed or the contract is now wrong; both are \
         reportable. Observed: {observed:?}",
        fixture.database().kind()
    );

    assert!(
        observed.iter().any(Option::is_none),
        "the probe lost the NULL row entirely, so its placement was never measured: {observed:?}"
    );
}

/// A migration that fails partway leaves a rolled-back schema on one engine and a changed one on
/// the other.
///
/// # The most consequential difference in this file
///
/// It decides what an operator finds after a failed migration: PostgreSQL leaves the database
/// exactly as it was, MySQL leaves every statement before the failure applied. A runner that
/// assumed the first behaviour would report a clean failure on MySQL while having half-migrated
/// the database.
pub async fn ddl_transactionality_is_engine_specific<F: PortabilityFixture>(fixture: &F) {
    let survived = fixture.ddl_survives_rollback().await;
    let expected = match fixture.database().kind() {
        // Transactional DDL: the `CREATE TABLE` is undone with everything else.
        DatabaseKind::Postgres => false,
        // An implicit commit surrounds DDL, so the rollback cannot reach it.
        DatabaseKind::MySql => true,
        // `DatabaseKind` is `#[non_exhaustive]` on purpose. A third engine must record its own
        // measured answer here rather than silently inherit one of these two.
        kind => panic!(
            "{kind:?} has never been measured against this contract. Add its answer, with the \
             measurement that produced it"
        ),
    };
    assert_eq!(
        survived,
        expected,
        "{:?} {} a table created inside a rolled-back transaction. The migration runner's \
         failure semantics are derived from this, so a change here is a change to what an \
         operator finds after a failed migration",
        fixture.database().kind(),
        if survived { "KEPT" } else { "removed" }
    );
}

/// A timestamp column declared with no precision means different things on the two engines.
pub async fn default_timestamp_precision_is_engine_specific<F: PortabilityFixture>(fixture: &F) {
    let digits = fixture.default_timestamp_digits().await;
    let expected = match fixture.database().kind() {
        DatabaseKind::Postgres => 6,
        // Seconds. A migration that writes `DATETIME` and expects microseconds silently truncates.
        DatabaseKind::MySql => 0,
        // `DatabaseKind` is `#[non_exhaustive]` on purpose. A third engine must record its own
        // measured answer here rather than silently inherit one of these two.
        kind => panic!(
            "{kind:?} has never been measured against this contract. Add its answer, with the \
             measurement that produced it"
        ),
    };
    assert_eq!(
        digits,
        expected,
        "a timestamp column declared without precision kept {digits} sub-second digits on {:?}, \
         and the portability guide states {expected}",
        fixture.database().kind()
    );
}

/// An upsert scoped to one unique key does something different when another key conflicts.
///
/// # This is the difference that can corrupt data rather than merely annoy
///
/// PostgreSQL refuses: the conflict was on a constraint the statement did not name, so it is a
/// unique violation like any other. MySQL has no way to scope `ON DUPLICATE KEY` at all, so it
/// updates whichever row conflicted — **a row the statement never mentioned**.
///
/// The contract's rule follows from this and is stated in `contracts/database-portability.md`: a
/// portable upsert targets a table with exactly one unique key. This assertion is what keeps that
/// rule attached to a measurement.
pub async fn an_unnamed_unique_key_is_not_a_portable_upsert_target<F: PortabilityFixture>(
    fixture: &F,
) {
    let outcome = fixture.upsert_on_unnamed_key().await;
    match (fixture.database().kind(), &outcome) {
        (DatabaseKind::Postgres, UnnamedKeyUpsert::Refused(kind)) => assert_eq!(
            *kind,
            DatabaseErrorKind::UniqueViolation,
            "PostgreSQL refused, correctly, but the refusal classified as {kind:?} rather than a \
             unique violation"
        ),
        (DatabaseKind::MySql, UnnamedKeyUpsert::RewroteAnotherRow(id)) => assert_eq!(
            *id, 1,
            "MySQL rewrote row {id}, and the probe was built so that the only row it could \
             reach is 1. A different row means the probe no longer measures what it claims"
        ),
        (kind, other) => panic!(
            "{kind:?} answered an upsert on an unnamed unique key with {other:?}. The contract \
             records PostgreSQL refusing and MySQL rewriting the conflicting row; an engine that \
             changed side here changes what a portable upsert may assume"
        ),
    }
}

/// A repeated read inside one transaction sees a concurrent commit on one engine and not the
/// other.
///
/// # The default isolation levels differ, and this measures the consequence
///
/// PostgreSQL defaults to `read committed`, MySQL/InnoDB to `REPEATABLE-READ`. Naming the levels
/// proves nothing — an engine could rename one tomorrow — so what is asserted is the behaviour a
/// read-modify-write actually depends on.
pub async fn repeated_reads_differ_by_default_isolation<F: PortabilityFixture>(fixture: &F) {
    let saw_it = fixture.repeated_read_sees_concurrent_commit().await;
    let expected = match fixture.database().kind() {
        // READ COMMITTED: each statement takes a fresh snapshot.
        DatabaseKind::Postgres => true,
        // REPEATABLE READ: the snapshot is fixed at the transaction's first read.
        DatabaseKind::MySql => false,
        // `DatabaseKind` is `#[non_exhaustive]` on purpose. A third engine must record its own
        // measured answer here rather than silently inherit one of these two.
        kind => panic!(
            "{kind:?} has never been measured against this contract. Add its answer, with the \
             measurement that produced it"
        ),
    };
    assert_eq!(
        saw_it,
        expected,
        "on {:?} a second read inside one transaction {} a row another session committed \
         meanwhile. A read-modify-write written against the other behaviour is a lost update",
        fixture.database().kind(),
        if saw_it { "SAW" } else { "did not see" }
    );
}

/// An over-long identifier is silently shortened on one engine and refused on the other.
///
/// # Truncation is the worse outcome, and it is the one that succeeds
///
/// PostgreSQL creates the object under a name the author did not write, so a later statement
/// naming the full identifier fails with "does not exist" — at a distance from the migration that
/// caused it. MySQL refuses at the point of the mistake.
pub async fn an_oversized_identifier_is_refused_or_shortened<F: PortabilityFixture>(fixture: &F) {
    let outcome = fixture.oversized_identifier().await;
    match (fixture.database().kind(), &outcome) {
        (DatabaseKind::Postgres, OversizedIdentifier::Truncated(name)) => assert_eq!(
            name.chars().count(),
            63,
            "PostgreSQL truncated to {} characters; the documented limit is 63 bytes",
            name.chars().count()
        ),
        (DatabaseKind::MySql, OversizedIdentifier::Refused) => {}
        (kind, other) => panic!(
            "{kind:?} answered an over-long identifier with {other:?}, which is not what the \
             portability guide records. An engine that started truncating where it used to refuse \
             would create objects under names no migration wrote"
        ),
    }
}

/// The recommended JSON type round-trips **identically** on both engines.
///
/// # A portable answer exists here, unlike everywhere else in this file
///
/// MySQL's `JSON` normalises: keys sorted, duplicates resolved last-wins. PostgreSQL offers a type
/// that does the same (`jsonb`) and one that does not (`json`, which keeps the received text
/// verbatim, duplicates and all). Choosing `jsonb` makes the two engines agree exactly, which is
/// why the contract requires it and why this assertion is an equality rather than a table.
///
/// The unadvised type is measured as a **control**: if it produced the same string, this assertion
/// would be passing for free.
pub async fn json_normalisation_agrees_across_engines<F: PortabilityFixture>(fixture: &F) {
    /// What both engines must return for `{"b":1,"a":2,"a":3}`: sorted, deduplicated, last wins.
    const NORMALISED: &str = r#"{"a": 3, "b": 1}"#;

    let observed = fixture.json_round_trip().await;
    assert_eq!(
        observed,
        NORMALISED,
        "the recommended JSON type on {:?} returned `{observed}`. Both engines must normalise to \
         the same text, or a document written on one and compared on the other differs without \
         its content having changed",
        fixture.database().kind()
    );

    if let Some(unadvised) = fixture.json_round_trip_unadvised().await {
        assert_ne!(
            unadvised, NORMALISED,
            "the type this contract advises against produced the same text as the one it \
             recommends, so the recommendation is currently untested. Either the engine changed \
             or the probe is pointed at the wrong type"
        );
    }
}

/// Every portability assertion, in one call.
pub async fn run_every_portability_assertion<F: PortabilityFixture>(fixture: &F) {
    nulls_sort_to_the_documented_end(fixture).await;
    ddl_transactionality_is_engine_specific(fixture).await;
    default_timestamp_precision_is_engine_specific(fixture).await;
    an_unnamed_unique_key_is_not_a_portable_upsert_target(fixture).await;
    repeated_reads_differ_by_default_isolation(fixture).await;
    an_oversized_identifier_is_refused_or_shortened(fixture).await;
    json_normalisation_agrees_across_engines(fixture).await;
}
