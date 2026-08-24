//! The persistence contract every Renvor database adapter must satisfy.
//!
//! # Why these assertions live here rather than in each adapter's tests
//!
//! `PLAN.md` §17 requires *"contract tests shared across direct SQLx/SeaORM and PostgreSQL/MySQL
//! adapters"*, and §Phase 007 accepts the phase only when *"both SeaORM rows pass the same
//! application contracts as direct SQLx"*.
//!
//! **The same** is the load-bearing word. Two suites that assert the same things in two files are
//! two suites, and they diverge the first time one is edited — quietly, because a weakened
//! assertion still passes. The functions below are compiled once and called from every adapter's
//! test binary, so "the same contract" is a fact about the build rather than a claim in a document.
//!
//! # What is shared, and what cannot be
//!
//! Shared: everything expressible through [`renvor_database`]'s ports — transaction boundaries,
//! visibility, cancellation, capacity, nesting, shutdown. That is the contract.
//!
//! Not shared: the SQL, because the whole point of Phase 007 is that the two adapters offer
//! different programming models. Each adapter supplies those few operations by implementing
//! [`PersistenceFixture`]; the **assertions** are here, and only here.

use core::future::Future;

use renvor_database::{
    Database, DatabaseError, DatabaseErrorKind, DatabaseKind, SeedReport, SeedScope, SqlSeed,
    UnitOfWork,
};

/// The driver-specific operations a contract run needs, supplied by the adapter under test.
///
/// Deliberately tiny. Anything that could be expressed through the ports is an assertion below
/// rather than a method here — a fixture that grew a `commit` would be re-implementing the
/// contract instead of being measured against it.
pub trait PersistenceFixture: Sync {
    /// The adapter's database type.
    type Database: Database;

    /// The database under test.
    fn database(&self) -> &Self::Database;

    /// Inserts one row with the given identifier, inside the supplied transaction.
    ///
    /// `&mut` because the direct-SQLx adapter reaches its pooled connection through `&mut self`,
    /// while the SeaORM adapter's `ConnectionTrait` takes `&self` and coerces. Taking the stricter
    /// of the two here is what lets one signature serve both rows.
    fn insert(
        &self,
        unit: &mut <Self::Database as Database>::UnitOfWork<'_>,
        id: i64,
    ) -> impl Future<Output = Result<(), DatabaseError>> + Send;

    /// Counts rows through the supplied transaction's **own** connection.
    ///
    /// # Why this exists alongside [`count`](Self::count)
    ///
    /// Because reading from a third connection cannot tell a *separate* transaction from a
    /// *nested* one. Both engines hide an uncommitted write from every other session, so an outer
    /// write is invisible from outside whether or not the second `begin` actually took a new
    /// session — and an assertion that only looks from outside passes under the bug it exists to
    /// catch. Reading from inside the second transaction distinguishes them.
    fn count_within(
        &self,
        unit: &mut <Self::Database as Database>::UnitOfWork<'_>,
    ) -> impl Future<Output = i64> + Send;

    /// Counts rows **on a connection outside any open transaction**.
    ///
    /// Outside, because visibility is the property under test: a count taken inside the
    /// transaction would observe its own uncommitted writes and every assertion below would pass
    /// regardless of whether anything was committed.
    fn count(&self) -> impl Future<Output = i64> + Send;

    /// Empties the fixture table.
    fn reset(&self) -> impl Future<Output = ()> + Send;

    /// Applies a seed set through this adapter's own runner.
    ///
    /// # Why seeding is in the shared contract at all
    ///
    /// Phase 007's FR-033 requires SeaORM seeding to behave identically to the direct-SQLx row.
    /// A review found that claim evidenced by an argument rather than a test, and the argument was
    /// false: the seed types lived inside the SQLx adapter, where the other one could not reach
    /// them. They now live in `renvor-database`, and the two **runners** are compared here.
    ///
    /// Only the runner is per-adapter, so only the runner is behind this method.
    fn seed(
        &self,
        scope: SeedScope,
        seeds: &[SqlSeed],
    ) -> impl Future<Output = Result<SeedReport, DatabaseError>> + Send;

    /// Empties the seed ledger, so a run starts from nothing.
    fn reset_seed_ledger(&self) -> impl Future<Output = ()> + Send;
}

/// An explicit commit persists, and is visible from another connection.
pub async fn commit_persists<F: PersistenceFixture>(fixture: &F) {
    fixture.reset().await;
    let mut unit = fixture.database().begin().await.expect("begins");
    fixture.insert(&mut unit, 1).await.expect("inserts");
    unit.commit().await.expect("commits");
    assert_eq!(
        fixture.count().await,
        1,
        "an explicit commit did not persist"
    );
}

/// An explicit rollback undoes.
pub async fn rollback_undoes<F: PersistenceFixture>(fixture: &F) {
    fixture.reset().await;
    let mut unit = fixture.database().begin().await.expect("begins");
    fixture.insert(&mut unit, 2).await.expect("inserts");
    unit.rollback().await.expect("rolls back");
    assert_eq!(
        fixture.count().await,
        0,
        "an explicit rollback did not undo"
    );
}

/// Dropping a unit of work without committing writes nothing.
///
/// This is the ordinary early-return shape — a `?` on a later step — not an exotic path.
pub async fn drop_without_commit_writes_nothing<F: PersistenceFixture>(fixture: &F) {
    fixture.reset().await;
    {
        let mut unit = fixture.database().begin().await.expect("begins");
        fixture.insert(&mut unit, 3).await.expect("inserts");
        // Dropped here. There is no commit-on-drop path, and this is what asserts it.
    }
    assert_eq!(fixture.count().await, 0, "a dropped unit of work committed");
}

/// Uncommitted rows are invisible to another connection while the transaction is still open.
///
/// Stronger than [`drop_without_commit_writes_nothing`]: that one observes the end state, this one
/// observes *during*, so an adapter that committed early and compensated afterwards fails here.
pub async fn uncommitted_rows_are_invisible<F: PersistenceFixture>(fixture: &F) {
    fixture.reset().await;
    let mut unit = fixture.database().begin().await.expect("begins");
    fixture.insert(&mut unit, 4).await.expect("inserts");
    assert_eq!(
        fixture.count().await,
        0,
        "an uncommitted row was visible to another connection"
    );
    unit.rollback().await.expect("rolls back");
}

/// Cancelling the surrounding future commits nothing.
pub async fn cancellation_commits_nothing<F: PersistenceFixture>(fixture: &F) {
    fixture.reset().await;
    let _ = tokio::time::timeout(core::time::Duration::from_millis(120), async {
        let mut unit = fixture.database().begin().await.expect("begins");
        fixture.insert(&mut unit, 5).await.expect("inserts");
        // Never reached: the deadline elapses first and drops this future mid-flight.
        tokio::time::sleep(core::time::Duration::from_secs(30)).await;
        unit.commit().await.expect("commits");
    })
    .await;
    assert_eq!(
        fixture.count().await,
        0,
        "a cancelled unit of work committed"
    );
}

/// A second `begin` is a separate session, not a nested transaction.
///
/// # Read from INSIDE the second transaction, with a control
///
/// This asserted only that the outer write was invisible from a third, unrelated connection — and
/// that assertion **passes under the exact bug it names**. Both engines hide an uncommitted write
/// from every other session, so a second `begin` that reused the first's session (on PostgreSQL a
/// second `BEGIN` on an open transaction emits a notice and continues the *same* one) would still
/// look correct from outside. A review caught it; the adapter-specific tests in
/// `renvor-sqlx/tests/ports.rs` and `renvor-seaorm/tests/cancellation.rs` were already doing this
/// properly, and only the shared version — the one both adapters rely on — was weak.
///
/// Two assertions, and the first is a **control**: the outer transaction must see its own write.
/// Without it, a `count_within` that quietly read from the pool would satisfy the second assertion
/// while measuring nothing.
pub async fn a_second_begin_is_separate<F: PersistenceFixture>(fixture: &F) {
    fixture.reset().await;
    let mut outer = fixture.database().begin().await.expect("begins");
    fixture.insert(&mut outer, 6).await.expect("inserts");

    // CONTROL: proves `count_within` really reads through the unit it is given.
    assert_eq!(
        fixture.count_within(&mut outer).await,
        1,
        "a transaction cannot see its own uncommitted write, so `count_within` is not reading \
         through the connection it was handed and the assertion below would prove nothing"
    );

    let mut inner = fixture.database().begin().await.expect("begins a second");
    assert_eq!(
        fixture.count_within(&mut inner).await,
        0,
        "the outer transaction's uncommitted write was visible to the second, so the second \
         `begin` continued the first transaction instead of starting a separate one"
    );
    assert_eq!(
        fixture.count().await,
        0,
        "the outer transaction's write was visible outside it"
    );

    inner.rollback().await.expect("rolls back the inner");
    outer.rollback().await.expect("rolls back the outer");
}

/// The unit of work reports that it is inside a transaction, and the database reports its engine.
pub async fn identity_is_reported<F>(fixture: &F, expected: DatabaseKind)
where
    F: PersistenceFixture,
    // Stated here rather than on the port: `renvor_database::UnitOfWork` requires only `Send`, and
    // being an `Executor` is a separate implementation both adapters happen to provide. A test
    // harness does not get to widen a published port trait for its own convenience.
    for<'c> <F::Database as Database>::UnitOfWork<'c>: renvor_database::Executor,
{
    use renvor_database::Executor as _;
    assert_eq!(fixture.database().kind(), expected, "wrong database kind");
    let unit = fixture.database().begin().await.expect("begins");
    assert!(
        unit.in_transaction(),
        "a unit of work denied being in a transaction"
    );
    assert_eq!(
        unit.kind(),
        expected,
        "a unit of work reported the wrong kind"
    );
    unit.rollback().await.expect("rolls back");
}

/// Seeding honours scope, records what it applied, and is idempotent across runs.
///
/// Three properties in one pass, because the interesting failures are in the interaction: a seed
/// out of scope must not be recorded, a `RunOnce` seed must be skipped the second time, and an
/// `Idempotent` seed must run again.
///
/// # The scopes are `Development` and `Test`, and that is the whole set
///
/// `SeedScope` is deny-by-default and exact — a seed runs only in the scope it declared. There is
/// deliberately no "any" scope and no production scope, so the two seeds below declare *different*
/// scopes and each run refuses one of them.
pub async fn seeding_honours_scope_and_idempotence<F: PersistenceFixture>(fixture: &F) {
    use renvor_database::{Idempotence, SeedDeclaration};

    fixture.reset_seed_ledger().await;

    let seeds = vec![
        SqlSeed::new(
            SeedDeclaration::new("dev_once", SeedScope::Development, Idempotence::RunOnce),
            vec!["INSERT INTO rv_seed_probe (id) VALUES (1)".to_owned()],
        ),
        SqlSeed::new(
            SeedDeclaration::new("test_repeat", SeedScope::Test, Idempotence::Idempotent),
            vec![
                "DELETE FROM rv_seed_probe WHERE id = 2".to_owned(),
                "INSERT INTO rv_seed_probe (id) VALUES (2)".to_owned(),
            ],
        ),
    ];

    // ── Test scope: the development-only seed is refused, and refused for the RIGHT reason.
    let under_test = fixture.seed(SeedScope::Test, &seeds).await.expect("seeds");
    assert_eq!(
        under_test.skipped_out_of_scope(),
        ["dev_once"],
        "a `Development` seed was not refused under the `Test` scope"
    );
    assert_eq!(
        under_test.applied(),
        ["test_repeat"],
        "the `Test` seed did not run under the `Test` scope"
    );

    // ── Development scope: the other half, which proves the refusal above was about scope rather
    //    than about that seed being broken.
    let under_dev = fixture
        .seed(SeedScope::Development, &seeds)
        .await
        .expect("seeds");
    assert_eq!(
        under_dev.applied(),
        ["dev_once"],
        "the `Development` seed did not run under the `Development` scope"
    );
    assert_eq!(
        under_dev.skipped_out_of_scope(),
        ["test_repeat"],
        "a `Test` seed ran under the `Development` scope"
    );

    // ── Second runs: `RunOnce` is skipped, `Idempotent` runs again. A runner that ignored the
    //    ledger would apply both, and a runner that consulted it too eagerly would skip both.
    let dev_again = fixture
        .seed(SeedScope::Development, &seeds)
        .await
        .expect("seeds");
    assert_eq!(
        dev_again.skipped_already_applied(),
        ["dev_once"],
        "a `RunOnce` seed ran a second time"
    );
    assert!(
        dev_again.applied().is_empty(),
        "a second development run applied something: {:?}",
        dev_again.applied()
    );

    let test_again = fixture.seed(SeedScope::Test, &seeds).await.expect("seeds");
    assert_eq!(
        test_again.applied(),
        ["test_repeat"],
        "an `Idempotent` seed was skipped on a second run"
    );

    fixture.reset_seed_ledger().await;
}

/// Readiness costs a real round trip and succeeds against a live database.
pub async fn check_succeeds<F: PersistenceFixture>(fixture: &F) {
    fixture
        .database()
        .check()
        .await
        .expect("a live database answers its readiness check");
}

/// Repeated cancellation does not shrink the pool.
///
/// Takes the configured capacity rather than reading it back, so a pool that silently grew cannot
/// make this pass by having spare slots.
pub async fn cancellation_does_not_shrink_the_pool<F: PersistenceFixture>(
    fixture: &F,
    capacity: usize,
    rounds: usize,
) {
    for round in 0..rounds {
        let _ = tokio::time::timeout(core::time::Duration::from_millis(120), async {
            let mut unit = fixture.database().begin().await.expect("begins");
            fixture.insert(&mut unit, 7).await.expect("inserts");
            tokio::time::sleep(core::time::Duration::from_secs(30)).await;
            unit.commit().await.expect("commits");
        })
        .await;

        // Functional, not metric-based: `size` and `num_idle` read the same in the healthy and the
        // stranded case, so only actually acquiring every slot proves capacity.
        let mut held = Vec::new();
        for slot in 0..capacity {
            match fixture.database().begin().await {
                Ok(unit) => held.push(unit),
                Err(error) => panic!(
                    "round {round}: the pool refused slot {slot} of {capacity} after a \
                     cancellation ({:?}) — capacity was lost",
                    error.kind()
                ),
            }
        }
        for unit in held {
            let _ = unit.rollback().await;
        }
    }
}

/// After the pool is closed, beginning a transaction is refused rather than hanging.
///
/// Consumes the fixture's usefulness, so it runs last in a suite.
pub async fn a_closed_pool_refuses_rather_than_hangs<F: PersistenceFixture>(fixture: &F) {
    fixture.database().close().await.expect("closes");
    let error = fixture
        .database()
        .begin()
        .await
        .err()
        .expect("a closed pool must refuse to begin a transaction");
    assert!(
        matches!(
            error.kind(),
            DatabaseErrorKind::PoolClosed | DatabaseErrorKind::AcquireTimeout
        ),
        "a closed pool reported {:?}, which names neither closure nor a deadline",
        error.kind()
    );
}

/// Every shared assertion, in one call.
///
/// # Why a single entry point exists
///
/// So that an adapter cannot run *some* of the contract. Adding a function above without adding it
/// here would be the one way to let the two rows diverge again, and the list is short enough to
/// read in full.
///
/// `a_closed_pool_refuses_rather_than_hangs` is deliberately **not** included: it closes the pool,
/// and a suite that ran it in the middle would report every later failure as a closed pool.
pub async fn run_every_shared_assertion<F>(fixture: &F, expected: DatabaseKind, capacity: usize)
where
    F: PersistenceFixture,
    for<'c> <F::Database as Database>::UnitOfWork<'c>: renvor_database::Executor,
{
    check_succeeds(fixture).await;
    identity_is_reported(fixture, expected).await;
    commit_persists(fixture).await;
    rollback_undoes(fixture).await;
    drop_without_commit_writes_nothing(fixture).await;
    uncommitted_rows_are_invisible(fixture).await;
    cancellation_commits_nothing(fixture).await;
    a_second_begin_is_separate(fixture).await;
    cancellation_does_not_shrink_the_pool(fixture, capacity, 3).await;
    seeding_honours_scope_and_idempotence(fixture).await;
    fixture.reset().await;
}
