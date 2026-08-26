//! One bounded domain example, run identically against all four persistence rows.
//!
//! # Why a domain example on top of the contract suite
//!
//! [`crate::persistence`] measures the *ports* — commit, rollback, cancellation, visibility. It
//! says nothing about whether an ordinary application can migrate a table and then read, write and
//! page it, which is what `PLAN.md` §10.1 requires of all four rows: *"compile, migrations,
//! transactions, CRUD, auth, generated app"*.
//!
//! This module is the CRUD half, expressed once and executed four times.
//!
//! # The domain is `rv_widget`, and it was not invented here
//!
//! Both adapters already ship byte-identical migration fixtures for it:
//!
//! ```sql
//! CREATE TABLE rv_widget (id BIGINT PRIMARY KEY, name VARCHAR(100) NOT NULL);
//! ALTER TABLE rv_widget ADD COLUMN rank_value BIGINT NOT NULL DEFAULT 0;
//! ```
//!
//! That schema is portable across both engines, is already exercised by the migration suites, and
//! carries everything the example needs: a **uniqueness boundary** (`id` is the primary key, so a
//! duplicate is a real constraint failure with no extra DDL), a **default value** to observe, and
//! **two ordered migrations** so "migration" is a sequence rather than one statement.
//!
//! A second domain table would be a second portable schema to keep correct, and this one is
//! already proven.
//!
//! # This crate still names no driver
//!
//! [`renvor_database::Executor`] deliberately exposes *"no statement type, no row type, no driver
//! handle"*. So every statement below is a [`WidgetFixture`] method, implemented in the adapter's
//! own test crate where a driver is already visible, while the **assertions** are compiled once
//! here. That split is what makes "the same operations on all four rows" a fact about the build
//! rather than four suites that happen to agree today.

use core::future::Future;

use renvor_database::{Database, DatabaseError, DatabaseErrorKind, UnitOfWork as _};

/// One row of the example domain, as an application would see it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Widget {
    /// The `name` column.
    pub name: String,
    /// The `rank_value` column added by the second migration.
    pub rank: i64,
}

/// The driver-specific operations the domain example needs.
///
/// Deliberately one method per observable operation, and no general "run this SQL" escape hatch. A
/// fixture that accepted arbitrary SQL from the assertions would let each row be handed *different*
/// SQL, which is the one thing a four-row example exists to rule out. The SQL that genuinely must
/// differ — placeholder syntax, and MySQL's table-level `FOREIGN KEY` — differs inside the
/// implementation, not in what the assertion asks for.
pub trait WidgetFixture: Sync {
    /// The adapter's database type.
    type Database: Database;

    /// The connected database.
    fn database(&self) -> &Self::Database;

    /// Applies the widget migrations through the adapter's own migration runner.
    ///
    /// Returns how many were applied, so a caller can distinguish a first run from a re-run.
    /// **Not raw DDL**: running the real migration path is what makes "migration" one of the
    /// operations this example proves rather than a setup step it skips past.
    fn migrate(&self) -> impl Future<Output = Result<usize, DatabaseError>> + Send;

    /// Applies only the migrations a **previous release** shipped.
    ///
    /// Backed by `tests/migrations-upgrade-base/`, whose files are byte-identical to the leading
    /// migrations in the full set. The identity is what makes an upgrade over them legal: the
    /// ledger stores a checksum, and a forward run over an edited file fails closed.
    ///
    /// Used by [`crate::upgrade`], which is the only suite that starts from a database somebody
    /// else migrated.
    fn migrate_base(&self) -> impl Future<Output = Result<usize, DatabaseError>> + Send;

    /// Removes the table and the migration ledger, so the next `migrate` starts from nothing.
    fn drop_schema(&self) -> impl Future<Output = ()> + Send;

    /// Whether the column added by the SECOND migration is present.
    ///
    /// Read from the server's own catalogue rather than inferred from a successful run: a
    /// migration runner that recorded a version without executing its SQL would otherwise pass.
    fn rank_column_exists(&self) -> impl Future<Output = bool> + Send;

    /// Inserts one widget on the unit's own connection, with bound values.
    fn insert(
        &self,
        unit: &mut <Self::Database as Database>::UnitOfWork<'_>,
        id: i64,
        name: &str,
    ) -> impl Future<Output = Result<(), DatabaseError>> + Send;

    /// Reads one widget on the unit's own connection.
    fn find(
        &self,
        unit: &mut <Self::Database as Database>::UnitOfWork<'_>,
        id: i64,
    ) -> impl Future<Output = Option<Widget>> + Send;

    /// Sets `rank_value`, returning the number of rows the server reports it changed.
    fn set_rank(
        &self,
        unit: &mut <Self::Database as Database>::UnitOfWork<'_>,
        id: i64,
        rank: i64,
    ) -> impl Future<Output = Result<u64, DatabaseError>> + Send;

    /// Deletes one widget, returning the number of rows the server reports it removed.
    fn remove(
        &self,
        unit: &mut <Self::Database as Database>::UnitOfWork<'_>,
        id: i64,
    ) -> impl Future<Output = Result<u64, DatabaseError>> + Send;

    /// Ids strictly greater than `after`, ascending, at most `limit` of them.
    ///
    /// The keyset shape, not `OFFSET` — an offset page is not stable under concurrent insertion,
    /// and stability is the property the pagination assertion measures.
    fn ids_after(
        &self,
        unit: &mut <Self::Database as Database>::UnitOfWork<'_>,
        after: i64,
        limit: i64,
    ) -> impl Future<Output = Vec<i64>> + Send;
}

/// A migration run applies both steps, in order, and the schema really changes.
///
/// Three claims, and the third is the one that needs the server: a runner that recorded versions
/// without executing their SQL would satisfy the first two.
pub async fn migration_applies_in_order_and_changes_the_schema<F: WidgetFixture>(fixture: &F) {
    fixture.drop_schema().await;
    assert!(
        !fixture.rank_column_exists().await,
        "the fixture did not start from a clean schema, so this assertion would measure the \
         previous run rather than this one"
    );

    let applied = fixture.migrate().await.expect("migrations apply");
    assert_eq!(applied, 2, "both widget migrations should be new");
    assert!(
        fixture.rank_column_exists().await,
        "the migration run reported success but the column the second migration adds is absent, \
         so the ledger advanced without the SQL running"
    );

    // IDEMPOTENCE. A second run must apply nothing rather than fail or re-apply.
    let again = fixture.migrate().await.expect("re-runs");
    assert_eq!(again, 0, "a second run must apply nothing");
}

/// What was written is what is read back.
///
/// The read goes through a different statement than the write, so a fixture whose `insert` did
/// nothing cannot satisfy it.
pub async fn insert_then_lookup_round_trips<F: WidgetFixture>(fixture: &F) {
    let mut unit = fixture.database().begin().await.expect("begins");
    fixture
        .insert(&mut unit, 1, "alpha")
        .await
        .expect("inserts");

    let found = fixture
        .find(&mut unit, 1)
        .await
        .expect("the row just written");
    assert_eq!(found.name, "alpha");
    assert_eq!(
        found.rank, 0,
        "`rank_value` declares DEFAULT 0, so an insert that names no rank must read back as 0 on \
         both engines"
    );

    assert!(
        fixture.find(&mut unit, 99).await.is_none(),
        "a lookup for an absent id must be None rather than a default row — the CONTROL for the \
         assertion above, which a `find` that fabricated a row would otherwise pass"
    );
    unit.rollback().await.expect("rolls back");
}

/// An update changes exactly one row, and the change is observable.
///
/// The affected-row count is asserted, not just the value. A fixture whose `set_rank` was a no-op
/// would return 0 and fail here even if a later read happened to see the value it wanted.
pub async fn update_changes_exactly_one_row<F: WidgetFixture>(fixture: &F) {
    let mut unit = fixture.database().begin().await.expect("begins");
    fixture
        .insert(&mut unit, 1, "alpha")
        .await
        .expect("inserts");
    fixture.insert(&mut unit, 2, "beta").await.expect("inserts");

    let changed = fixture.set_rank(&mut unit, 1, 42).await.expect("updates");
    assert_eq!(
        changed, 1,
        "an update by primary key must affect exactly one row"
    );
    assert_eq!(fixture.find(&mut unit, 1).await.expect("present").rank, 42);
    assert_eq!(
        fixture.find(&mut unit, 2).await.expect("present").rank,
        0,
        "the other row must be untouched, or the update was not scoped by its key"
    );

    let absent = fixture.set_rank(&mut unit, 99, 1).await.expect("succeeds");
    assert_eq!(
        absent, 0,
        "an update matching nothing must report zero rows rather than an error: an application \
         distinguishes those, and both engines report it the same way"
    );
    unit.rollback().await.expect("rolls back");
}

/// A delete removes exactly one row, and the row is gone.
pub async fn delete_removes_exactly_one_row<F: WidgetFixture>(fixture: &F) {
    let mut unit = fixture.database().begin().await.expect("begins");
    fixture
        .insert(&mut unit, 1, "alpha")
        .await
        .expect("inserts");
    fixture.insert(&mut unit, 2, "beta").await.expect("inserts");

    let removed = fixture.remove(&mut unit, 1).await.expect("deletes");
    assert_eq!(
        removed, 1,
        "a delete by primary key must remove exactly one row"
    );
    assert!(
        fixture.find(&mut unit, 1).await.is_none(),
        "the row must be gone"
    );
    assert!(
        fixture.find(&mut unit, 2).await.is_some(),
        "the other row must survive, or the delete was not scoped by its key"
    );

    assert_eq!(
        fixture.remove(&mut unit, 99).await.expect("succeeds"),
        0,
        "deleting nothing must report zero rows rather than an error"
    );
    unit.rollback().await.expect("rolls back");
}

/// A duplicate identity is refused, and classified the same way on every row.
///
/// # This is where Batch B meets the error vocabulary
///
/// The assertion is not merely that the second insert fails — it is that it fails as
/// [`DatabaseErrorKind::UniqueViolation`] on all four rows. PostgreSQL reports SQLSTATE `23505` and
/// MySQL error `1062` with the generic SQLSTATE `23000`; an application that had to tell those
/// apart itself would be writing engine-specific code at exactly the boundary Renvor exists to
/// remove.
pub async fn a_duplicate_identity_is_a_unique_violation<F: WidgetFixture>(fixture: &F) {
    let mut unit = fixture.database().begin().await.expect("begins");
    fixture
        .insert(&mut unit, 1, "alpha")
        .await
        .expect("inserts");

    let error = fixture
        .insert(&mut unit, 1, "different name, same id")
        .await
        .expect_err("a duplicate primary key must be refused by the server");
    assert_eq!(
        error.kind(),
        DatabaseErrorKind::UniqueViolation,
        "every row must classify a duplicate identity identically"
    );

    // A transaction that has hit an error is not reusable on PostgreSQL, so it ends here rather
    // than continuing to assert. The rollback is the point: the failed insert wrote nothing.
    unit.rollback().await.expect("rolls back");
    let mut after = fixture.database().begin().await.expect("begins");
    assert!(
        fixture.find(&mut after, 1).await.is_none(),
        "the rolled-back transaction must have left no row at all"
    );
    after.rollback().await.expect("rolls back");
}

/// Paging by keyset returns every row exactly once, in a stable order.
///
/// # Totality and stability are different properties, and both are asserted
///
/// A pager can be stable and lose rows, or return everything in an order that changes between
/// runs. The first is checked by collecting every page and comparing against the full set; the
/// second by paging twice and requiring the same sequence.
pub async fn pagination_is_total_and_stable<F: WidgetFixture>(fixture: &F) {
    const ROWS: i64 = 7;
    const PAGE: i64 = 3;

    // INSERTED OUT OF ORDER, ON PURPOSE. Rows written 1..=7 in sequence come back in that order
    // from both engines even with no `ORDER BY` at all — PostgreSQL from heap order on a small
    // table, MySQL from the clustered primary key. A pager that had lost its ordering clause would
    // then pass, which was observed: the mutation that deletes `ORDER BY id ASC` SURVIVED against
    // sequential inserts. Scrambling the write order makes insertion order differ from key order,
    // so the clause has to do real work.
    const SCRAMBLED: [i64; ROWS as usize] = [4, 7, 2, 5, 1, 6, 3];

    let mut unit = fixture.database().begin().await.expect("begins");
    for id in SCRAMBLED {
        fixture
            .insert(&mut unit, id, &format!("widget-{id}"))
            .await
            .expect("inserts");
    }

    let walk = async |unit: &mut <F::Database as Database>::UnitOfWork<'_>| {
        let mut seen = Vec::new();
        let mut after = 0;
        loop {
            let page = fixture.ids_after(unit, after, PAGE).await;
            if page.is_empty() {
                break;
            }
            assert!(
                page.len() as i64 <= PAGE,
                "a page returned more rows than its limit"
            );
            after = *page.last().expect("non-empty");
            seen.extend(page);
        }
        seen
    };

    let first = walk(&mut unit).await;
    assert_eq!(
        first,
        (1..=ROWS).collect::<Vec<_>>(),
        "paging must return every row exactly once, ascending, with no gap and no repeat"
    );

    let second = walk(&mut unit).await;
    assert_eq!(
        first, second,
        "the same query over unchanged data returned a different order, so the ordering is not \
         total and a cursor cannot be resumed from"
    );

    unit.rollback().await.expect("rolls back");
}

/// Every domain assertion, in the order a reader of the evidence will want them.
///
/// Migration runs first because the rest need the table it creates.
pub async fn run_every_domain_assertion<F: WidgetFixture>(fixture: &F) {
    migration_applies_in_order_and_changes_the_schema(fixture).await;
    insert_then_lookup_round_trips(fixture).await;
    update_changes_exactly_one_row(fixture).await;
    delete_removes_exactly_one_row(fixture).await;
    a_duplicate_identity_is_a_unique_violation(fixture).await;
    pagination_is_total_and_stable(fixture).await;
}
