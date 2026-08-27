//! The upgrade path: a database an older release left behind, brought forward.
//!
//! # What the existing migration suites do not cover
//!
//! Each adapter already proves that migrations apply in order, that a *changed* applied migration
//! fails closed, and that an irreversible one is refused. All of those start from **nothing**.
//!
//! The case an operator actually meets starts from **something**: a database that a previous
//! release migrated and then filled with rows. Upgrading it is where the interesting failures
//! live, and none of them are visible from an empty database:
//!
//! - The runner re-applies a migration the old release already ran, and the second run fails or
//!   duplicates.
//! - The new migration adds a `NOT NULL` column and the existing rows have nothing to put in it.
//! - The upgrade succeeds and the pre-existing rows are silently lost or truncated.
//!
//! # Why the base fixture is a byte-identical copy
//!
//! `tests/migrations-upgrade-base/` holds **the same bytes** as the first migration in
//! `tests/migrations/`. That is what makes the upgrade legal: the ledger records a checksum, and a
//! forward run over an edited file fails closed by design.
//!
//! So this suite also proves the converse of the fail-closed test — that a migration which was
//! *not* edited is recognised as already applied and skipped, rather than re-run.

use renvor_database::{Database as _, UnitOfWork as _};

use crate::domain::WidgetFixture;

/// A database written by the previous release upgrades without losing a row.
///
/// # The three claims, and why each needs the one before it
///
/// 1. The old release's schema applies on its own — otherwise there is no "before" to upgrade.
/// 2. The current migration set applies **only what is missing**. A runner that re-ran migration
///    one would fail on `CREATE TABLE`, and one that reported success without running migration
///    two would leave the column absent.
/// 3. The row written before the upgrade is still there, and now carries the new column's
///    **default**. This is the assertion that an empty-database test cannot make at all: a
///    `NOT NULL` column added to a table with rows in it has to come from somewhere.
pub async fn a_database_from_the_previous_release_upgrades<F: WidgetFixture>(fixture: &F) {
    const CARRIED: i64 = 700;

    fixture.drop_schema().await;

    // ---- as the previous release left it ----
    let base = fixture
        .migrate_base()
        .await
        .expect("the previous release's migrations apply");
    assert_eq!(
        base, 1,
        "the upgrade base must be exactly the first migration, or this measures a different \
         upgrade than the one it claims"
    );
    assert!(
        !fixture.rank_column_exists().await,
        "the base fixture already has the column the upgrade is supposed to add, so the upgrade \
         would be a no-op and every assertion below would pass without it"
    );

    let mut unit = fixture.database().begin().await.expect("begins");
    fixture
        .insert(&mut unit, CARRIED, "written before the upgrade")
        .await
        .expect("the old schema accepts a row");
    unit.commit().await.expect("commits");

    // ---- the upgrade ----
    let applied = fixture
        .migrate()
        .await
        .expect("the current migration set applies over the previous release's database");
    assert_eq!(
        applied, 1,
        "the upgrade applied {applied} migrations. Exactly one was missing; a larger number means \
         the runner re-ran a migration the previous release had already applied, and zero means it \
         advanced the ledger without running anything"
    );
    assert!(
        fixture.rank_column_exists().await,
        "the upgrade reported success but the column its migration adds is absent"
    );

    // ---- the row that was there before ----
    let mut unit = fixture.database().begin().await.expect("begins");
    let carried = fixture
        .find(&mut unit, CARRIED)
        .await
        .expect("a row written before the upgrade must survive it");
    assert_eq!(
        carried.name, "written before the upgrade",
        "the carried row survived with different contents"
    );
    assert_eq!(
        carried.rank, 0,
        "the carried row came back with rank {}. The upgrade adds a NOT NULL column with \
         DEFAULT 0, and an existing row has to take that default — an engine that back-filled \
         something else, or a migration written without one, would show up here and nowhere else",
        carried.rank
    );
    unit.rollback().await.expect("rolls back");
}

/// Re-running the current migration set over an already-upgraded database changes nothing.
///
/// # Idempotence at the schema level
///
/// A deployment that runs migrations on boot runs them on **every** boot, including the boots
/// where nothing changed. That path has to be free, and "free" has to mean zero applied rather
/// than "applies but happens not to break".
pub async fn re_running_the_upgrade_applies_nothing<F: WidgetFixture>(fixture: &F) {
    const CARRIED: i64 = 701;

    let mut unit = fixture.database().begin().await.expect("begins");
    fixture
        .insert(&mut unit, CARRIED, "written after the upgrade")
        .await
        .expect("inserts");
    unit.commit().await.expect("commits");

    let again = fixture.migrate().await.expect("re-runs");
    assert_eq!(
        again, 0,
        "a second run of an unchanged migration set applied {again} migrations rather than none"
    );

    let mut unit = fixture.database().begin().await.expect("begins");
    assert!(
        fixture.find(&mut unit, CARRIED).await.is_some(),
        "a no-op migration run removed a row, which means it was not a no-op"
    );
    // Cleanup: these two rows are committed, unlike every rolled-back assertion elsewhere.
    fixture.remove(&mut unit, CARRIED).await.expect("deletes");
    fixture.remove(&mut unit, 700).await.expect("deletes");
    unit.commit().await.expect("commits");
}

/// Every upgrade assertion, in the order the operator meets them.
pub async fn run_every_upgrade_assertion<F: WidgetFixture>(fixture: &F) {
    a_database_from_the_previous_release_upgrades(fixture).await;
    re_running_the_upgrade_applies_nothing(fixture).await;
}
