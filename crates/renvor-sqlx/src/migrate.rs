//! Bounded, observable migrations over SQLx's migrator.
//!
//! # What this wraps, and what it adds
//!
//! SQLx already orders migrations deterministically, checksums them, fails closed on a mismatch,
//! and takes a **database-level** lock. Reimplementing that would breach constitution principle
//! III. What SQLx does not provide, and this module does:
//!
//! - **A bound on the lock wait.** `sqlx-mysql` issues `SELECT GET_LOCK(?, -1)` and
//!   `sqlx-postgres` issues `pg_advisory_lock` — both wait forever, and neither honours a
//!   statement timeout. Renvor takes the lock itself, under
//!   [`MigrationSettings::lock_timeout`], and tells SQLx not to.
//! - **A guard against a leaked lock.** In `Migrator::run_direct`, `unlock()` is reached only after
//!   the migration loop; every failure inside it is a `?` or an early return. A connection that
//!   fails mid-run is returned to the pool **still holding a session-level lock**, and the next
//!   migrator to draw a different connection blocks forever. This module runs migrations on a
//!   **dedicated** connection and ends that session on every path — success, failure, deadline,
//!   cancellation, panic — so the lock dies with the session rather than with a return value
//!   somebody remembered to check.
//! - **Observability.** `Migrator::run` returns `()`. A [`renvor_database::MigrationReport`] says which migrations
//!   ran, which were already applied, and how long each took.
//!
//! # Why `run_direct` and not `run`
//!
//! `Migrator::run<'a, A: Acquire<'a>>` parametrises a lifetime that appears **only inside a trait
//! bound**. Coercing the resulting future into the kernel's
//! [`ProviderFuture`](renvor_core::provider::registry::ProviderFuture) — a boxed `dyn Future +
//! Send` — erases that region, so the obligation must be discharged for *every* lifetime, and
//! SQLx's `impl<'c, DB> Acquire<'c> for &'c mut DB::Connection` holds for one at a time. The
//! compiler says so plainly:
//!
//! ```text
//! error: implementation of `sqlx::Acquire` is not general enough
//! ```
//!
//! Making the driver concrete does not help, and neither does boxing an `async fn` that wraps it.
//! `run_direct<C: Migrate>` mentions no lifetime at all, so it boxes. That is not a trick found
//! here — it is upstream's own escape hatch, carrying upstream's own comment:
//!
//! ```text
//! // Getting around the annoying "implementation of `Acquire` is not general enough" error
//! #[doc(hidden)]
//! pub async fn run_direct<C>(...)
//! ```
//!
//! `Migrator::run` is two lines: acquire, then `run_direct(None, &mut *conn, false)`. Calling
//! `run_direct` with a connection this module already holds is an exact equivalence, not a
//! reimplementation of anything.
//!
//! **The cost, stated rather than buried.** `run_direct` is `#[doc(hidden)]` and therefore exempt
//! from SQLx's semver guarantee: a patch release may change or remove it. Three things bound that
//! risk — it has exactly one call site in the workspace, `compile_guard` below fails the build the
//! moment its shape changes, and ADR-0018 records what removal would cost. The full compile probe
//! is in the Phase 006 evidence.
//!
//! # MySQL migrations are not atomic
//!
//! SQLx's own source: *"For MySQL we cannot really isolate migrations due to implicit commits
//! caused by table modification."* An interrupted MySQL migration is **detectable** — it reports
//! [`DatabaseErrorKind::MigrationDirty`] — but it is **not** rolled back, and the repair is manual.
//! PostgreSQL is atomic unless a migration opens with `-- no-transaction`.

#[cfg(feature = "db-postgres")]
use std::future::Future;
use std::path::Path;
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use std::time::Instant;

use renvor_database::{DatabaseError, DatabaseErrorKind, MigrationSettings, Reversibility};
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use renvor_database::{MigrationOutcome, MigrationReport, MigrationStep};
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use sqlx::Connection as _;
use sqlx::migrate::Migrator;

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use crate::error;

/// A loaded, ordered migration set.
#[derive(Debug)]
pub struct Migrations {
    migrator: Migrator,
    settings: MigrationSettings,
}

impl Migrations {
    /// Loads migrations from a directory at **runtime**.
    ///
    /// # Runtime rather than `migrate!`
    ///
    /// The `migrate!` macro embeds the directory at compile time, which ties the binary to the
    /// migration set present when it was built and makes the build depend on the filesystem layout.
    /// Loading at runtime keeps migrations a deployment artifact.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::MigrationFailed`] when the directory cannot be read or a migration is
    /// malformed. The path is **not** carried in the error: a filesystem path is an implementation
    /// detail of the deployment.
    pub async fn load(
        directory: &Path,
        settings: MigrationSettings,
    ) -> Result<Self, DatabaseError> {
        let migrator = Migrator::new(directory).await.map_err(|inner| {
            tracing::debug!(migrate_error = %inner, "migration source could not be loaded");
            DatabaseError::new(DatabaseErrorKind::MigrationFailed)
        })?;
        Ok(Self::from_migrator(migrator, settings))
    }

    /// Builds from an already-constructed migrator, for tests and for embedded sets.
    ///
    /// # Locking is turned off here, and taken back on deliberately
    ///
    /// SQLx would otherwise lock inside `run_direct` with an **unbounded** wait. Constitution
    /// principle VI does not permit that, so the runner below takes the same lock through the same
    /// public [`sqlx::migrate::Migrate`] trait, under [`MigrationSettings::lock_timeout`]. Turning
    /// it off in the one constructor every other path funnels through means no caller can
    /// accidentally get the unbounded version back.
    #[must_use]
    pub fn from_migrator(mut migrator: Migrator, settings: MigrationSettings) -> Self {
        migrator.set_locking(false);
        Self { migrator, settings }
    }

    /// The settings.
    #[must_use]
    pub const fn settings(&self) -> &MigrationSettings {
        &self.settings
    }

    /// Every migration's version, in the order they will be applied.
    ///
    /// # An assertion target, not a convenience
    ///
    /// FR-015 requires a deterministic total order *independent of filesystem enumeration order*.
    /// A test reads this and asserts it is strictly increasing, so a future change that broke the
    /// ordering is caught before a schema is.
    #[must_use]
    pub fn versions(&self) -> Vec<i64> {
        self.forward().map(|m| m.version).collect()
    }

    /// The migrations that are applied moving **forward**.
    ///
    /// # A reversible migration appears twice, and that is not a version appearing twice
    ///
    /// `Migrator::iter()` yields a `ReversibleUp` **and** a `ReversibleDown` entry for each
    /// reversible migration, both carrying the same version. Counting the raw iterator therefore
    /// reports every reversible migration twice — which is exactly what an early version of this
    /// code did, and what `versions_are_strictly_increasing_and_independent_of_directory_order`
    /// caught before it reached a report an operator would have trusted.
    fn forward(&self) -> impl Iterator<Item = &sqlx::migrate::Migration> {
        self.migrator
            .iter()
            .filter(|m| m.migration_type.is_up_migration())
    }

    /// Whether a rollback file exists for `version`.
    fn has_down(&self, version: i64) -> bool {
        self.migrator
            .iter()
            .any(|m| m.version == version && m.migration_type.is_down_migration())
    }

    /// Whether the set is strictly ordered.
    #[must_use]
    pub fn is_ordered(&self) -> bool {
        self.versions().windows(2).all(|pair| match pair {
            [first, second] => first < second,
            _ => true,
        })
    }

    /// Whether every migration declares a rollback.
    #[must_use]
    pub fn reversibility_of(&self, version: i64) -> Option<Reversibility> {
        if !self.forward().any(|m| m.version == version) {
            return None;
        }
        Some(if self.has_down(version) {
            Reversibility::Reversible
        } else {
            Reversibility::Irreversible
        })
    }

    /// Refuses a rollback the set cannot perform, **before** anything is locked or modified.
    ///
    /// # The refusal happens here, not at the database
    ///
    /// `PLAN.md` §12: *"Rollback support is declared per migration; unsupported rollback fails
    /// before changing data."* Checking after taking the lock would already have blocked other
    /// starters; checking after the first statement would already have changed data.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::MigrationIrreversible`] when the target declares no rollback, and
    /// [`DatabaseErrorKind::MigrationFailed`] when the version is not in the set at all.
    pub fn ensure_reversible(&self, version: i64) -> Result<(), DatabaseError> {
        match self.reversibility_of(version) {
            Some(Reversibility::Reversible) => Ok(()),
            Some(Reversibility::Irreversible) => {
                Err(DatabaseError::new(DatabaseErrorKind::MigrationIrreversible))
            }
            None => Err(DatabaseError::new(DatabaseErrorKind::MigrationFailed)),
        }
    }
}

/// Reads the applied versions.
///
/// # A missing table is not an error here
///
/// Before the first run the bookkeeping table does not exist. Reporting that as a failure would
/// make a first deployment look broken, so an unreadable table yields an empty set — which is the
/// truth, and which the migrator itself is about to correct.
///
/// The statement is a constant with no interpolation, so nothing untrusted reaches it.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
const APPLIED_VERSIONS: &str = "SELECT version FROM _sqlx_migrations ORDER BY version";

/// How long the migration session is given to close itself.
///
/// Matches `sqlx`'s own `CLOSE_ON_DROP_TIMEOUT` rather than inventing a second number: the
/// fallback path on cancellation *is* `sqlx`'s, so a different bound here would only mean the two
/// disagree about when a wedged socket stops being waited on.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
const CLEANUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Fails the build if `Migrator::run_direct` stops being the thing this module depends on.
///
/// # Why a guard rather than trusting the call site
///
/// `run_direct` is `#[doc(hidden)]`, so SQLx may change it in a **patch** release without breaking
/// its own semver promise. A removal or a rename would already be a compile error, but a signature
/// that still accepts the same call while meaning something else would not be — and neither would
/// a future whose `Send` bound quietly disappeared, which is the exact property that makes the
/// migration reachable from a [`ProviderFuture`](renvor_core::provider::registry::ProviderFuture)
/// at all.
///
/// This function is never called. It exists to be type-checked, and to give the resulting failure a
/// name and a paragraph instead of an error inside a macro expansion.
#[cfg(feature = "db-postgres")]
#[expect(
    dead_code,
    reason = "type-checked, never called: it is a compile-time assertion"
)]
fn compile_guard(migrator: &Migrator, connection: &mut sqlx::PgConnection) {
    fn assert_send_future<T: Send + Future<Output = Result<(), sqlx::migrate::MigrateError>>>(
        _: T,
    ) {
    }
    assert_send_future(migrator.run_direct(None::<i64>, connection, false));
}

/// Generates the per-driver runner.
///
/// # Why a macro and not a generic function
///
/// See the module documentation: the obligation `Migrator` places on its caller cannot be
/// discharged for all lifetimes at once. The bodies below are identical, and deliberately so — two
/// expansions of one text is a smaller risk than a generic signature that does not compile.
macro_rules! runner {
    ($name:ident, $driver:ty, $feature:literal) => {
        #[cfg(feature = $feature)]
        impl Migrations {
            /// Applies every pending migration on a **dedicated** connection, under two deadlines.
            ///
            /// # The session always ends here
            ///
            /// Not "on the failure paths" — always. A migration holds a database-wide lock that is
            /// released by the session dying, and there is exactly one way to be sure a session
            /// died: end it. So the connection is marked
            /// [`close_on_drop`](sqlx::pool::PoolConnection::close_on_drop) the instant it is
            /// acquired — which covers cancellation and panic, where no code of ours runs — and is
            /// then detached and closed explicitly on every path that returns normally.
            ///
            /// [`detach`](sqlx::pool::PoolConnection::detach) releases the pool slot
            /// **synchronously**, so the one connection this costs is not one the pool loses: it
            /// opens a replacement on demand. The cost is a single reconnect, once, at the moment
            /// a migration ran — not a per-operation tax.
            ///
            /// # Two deadlines, because they mean different things
            ///
            /// A lock wait that elapses means *another process is migrating*; wait, or look at what
            /// it is doing. A run that elapses means *your migration is too slow*; look at the
            /// migration. Reporting both as one kind would send an operator to the wrong place.
            ///
            /// # Errors
            ///
            /// [`DatabaseErrorKind::MigrationLockTimeout`] when the lock wait elapsed,
            /// [`DatabaseErrorKind::DeadlineExceeded`] when the whole run elapsed,
            /// [`DatabaseErrorKind::MigrationChecksumMismatch`] when an applied migration's content
            /// changed, [`DatabaseErrorKind::MigrationDirty`] when a previous run did not complete,
            /// and [`DatabaseErrorKind::MigrationFailed`] otherwise.
            pub async fn $name(
                &self,
                database: &crate::SqlxDatabase<$driver>,
            ) -> Result<MigrationReport, DatabaseError> {
                use sqlx::migrate::Migrate as _;

                let pool = database.pool();

                let before: Vec<i64> = sqlx::query_scalar(APPLIED_VERSIONS)
                    .fetch_all(pool)
                    .await
                    .unwrap_or_default();
                let started = Instant::now();

                let mut connection = pool
                    .acquire()
                    .await
                    .map_err(|inner| error::classify_error(&inner))?;
                // THE CLEANUP GUARD, armed before anything can go wrong. Every later path either
                // closes this connection explicitly or is a path where no code of ours runs at all.
                connection.close_on_drop();

                // Borrowed in an inner block so that the cleanup below is written once rather than
                // once per exit. Every `return` in this body would otherwise be a chance to forget.
                let outcome: Result<(), DatabaseError> = async {
                    match tokio::time::timeout(self.settings.lock_timeout(), connection.lock())
                        .await
                    {
                        Ok(Ok(())) => {}
                        Ok(Err(inner)) => {
                            return Err(error::classify_error(&sqlx::Error::Migrate(Box::new(
                                inner,
                            ))));
                        }
                        Err(_) => {
                            return Err(DatabaseError::new(
                                DatabaseErrorKind::MigrationLockTimeout,
                            ));
                        }
                    }

                    match tokio::time::timeout(
                        self.settings.run_timeout(),
                        self.migrator.run_direct(None, &mut *connection, false),
                    )
                    .await
                    {
                        Ok(Ok(())) => {
                            // Released explicitly as well as by the close below. The close is what
                            // guarantees it; this is what makes it prompt, so a queued starter is
                            // not waiting on a socket teardown.
                            let _ = tokio::time::timeout(
                                self.settings.lock_timeout(),
                                connection.unlock(),
                            )
                            .await;
                            Ok(())
                        }
                        Ok(Err(inner)) => Err(error::classify_error(&sqlx::Error::Migrate(
                            Box::new(inner),
                        ))),
                        Err(_) => Err(DatabaseError::new(DatabaseErrorKind::DeadlineExceeded)),
                    }
                }
                .await;

                // THE ONE CLEANUP. Bounded, because a wedged socket must not become a hung boot,
                // and `detach` has already freed the pool slot by the time the close is waited on.
                let _ = tokio::time::timeout(CLEANUP_TIMEOUT, connection.detach().close()).await;

                outcome?;

                let after: Vec<i64> = sqlx::query_scalar(APPLIED_VERSIONS)
                    .fetch_all(pool)
                    .await
                    .unwrap_or_default();
                Ok(self.report(&before, &after, started.elapsed()))
            }
        }
    };
}

runner!(run_postgres, sqlx::Postgres, "db-postgres");
runner!(run_mysql, sqlx::MySql, "db-mysql");

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
impl Migrations {
    /// Builds the report by comparing the applied set before and after.
    fn report(
        &self,
        before: &[i64],
        after: &[i64],
        elapsed: std::time::Duration,
    ) -> MigrationReport {
        let mut report = MigrationReport::new();
        let newly = after.len().saturating_sub(before.len()).max(1);
        // Elapsed time is attributed evenly across newly-applied migrations. Per-migration timing
        // would need a hook SQLx does not expose, and inventing a precise-looking number would be
        // a measurement claim this code cannot support.
        let each = elapsed / u32::try_from(newly).unwrap_or(1);

        for migration in self.forward() {
            let outcome = if before.contains(&migration.version) {
                MigrationOutcome::AlreadyApplied
            } else {
                MigrationOutcome::Applied
            };
            let reversibility = if self.has_down(migration.version) {
                Reversibility::Reversible
            } else {
                Reversibility::Irreversible
            };
            report.push(MigrationStep::new(
                migration.version,
                migration.description.as_ref(),
                reversibility,
                outcome,
                if outcome == MigrationOutcome::Applied {
                    each
                } else {
                    std::time::Duration::ZERO
                },
            ));
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> MigrationSettings {
        MigrationSettings::default()
    }

    #[test]
    fn an_empty_set_is_trivially_ordered() {
        let migrations =
            Migrations::from_migrator(Migrator::with_migrations(Vec::new()), settings());
        assert!(migrations.is_ordered());
        assert!(migrations.versions().is_empty());
    }

    /// Renvor owns the migration lock; `sqlx` must not also be taking it.
    ///
    /// # Why this is asserted on the field rather than through a database
    ///
    /// Because the behavioural test cannot see it. `pg_advisory_lock` and `GET_LOCK` are both
    /// **re-entrant within one session**, so a `run_direct` that locked again on the connection
    /// Renvor has already locked would succeed instantly and every real-database test would still
    /// pass. The unbounded wait that `set_locking(true)` reintroduces is only reachable in a race
    /// this suite cannot stage deterministically — which is exactly the situation a white-box
    /// assertion is for.
    ///
    /// Found by mutation testing: flipping `set_locking(false)` to `true` survived the whole
    /// real-database suite. This is the test that kills it.
    #[test]
    fn renvor_owns_the_migration_lock_rather_than_sqlx() {
        let migrations =
            Migrations::from_migrator(Migrator::with_migrations(Vec::new()), settings());
        assert!(
            !migrations.migrator.locking,
            "sqlx is taking the migration lock again, with an unbounded wait: `sqlx-mysql` issues \
             `GET_LOCK(?, -1)` and `sqlx-postgres` issues `pg_advisory_lock`, and neither honours \
             a deadline"
        );
    }

    #[test]
    fn an_unknown_version_is_refused_rather_than_treated_as_reversible() {
        let migrations =
            Migrations::from_migrator(Migrator::with_migrations(Vec::new()), settings());
        assert_eq!(migrations.reversibility_of(1), None);
        let error = migrations.ensure_reversible(1).expect_err("refused");
        assert_eq!(error.kind(), DatabaseErrorKind::MigrationFailed);
    }
}
