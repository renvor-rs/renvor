//! Bounded, observable migrations over SQLx's migrator.
//!
//! # What this wraps, and what it adds
//!
//! SQLx already orders migrations deterministically, checksums them, fails closed on a mismatch,
//! and takes a **database-level** lock. Reimplementing that would breach constitution principle
//! III. What SQLx does not provide, and this module does:
//!
//! - **A bound on the lock wait.** `sqlx-mysql` issues `SELECT GET_LOCK(?, -1)` — an infinite wait.
//! - **A guard against a leaked lock.** In `Migrator::run_direct`, `unlock()` is reached only after
//!   the migration loop; every failure inside it is a `?` or an early return. A connection that
//!   fails mid-run is returned to the pool **still holding a session-level lock**, and the next
//!   migrator to draw a different connection blocks forever. This module runs migrations on a
//!   **dedicated** connection and closes it on every failure path, so the lock dies with the
//!   session.
//! - **Observability.** `Migrator::run` returns `()`. A [`MigrationReport`] says which migrations
//!   ran, which were already applied, and how long each took.
//!
//! # MySQL migrations are not atomic
//!
//! SQLx's own source: *"For MySQL we cannot really isolate migrations due to implicit commits
//! caused by table modification."* An interrupted MySQL migration is **detectable** — it reports
//! [`DatabaseErrorKind::MigrationDirty`] — but it is **not** rolled back, and the repair is manual.
//! PostgreSQL is atomic unless a migration opens with `-- no-transaction`.

use std::path::Path;
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use std::time::Instant;

use renvor_database::{DatabaseError, DatabaseErrorKind, MigrationSettings, Reversibility};
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use renvor_database::{MigrationOutcome, MigrationReport, MigrationStep};
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use sqlx::Connection as _;
use sqlx::migrate::{MigrationType, Migrator};

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
        Ok(Self {
            migrator,
            settings,
        })
    }

    /// Builds from an already-constructed migrator, for tests and for embedded sets.
    #[must_use]
    pub const fn from_migrator(migrator: Migrator, settings: MigrationSettings) -> Self {
        Self {
            migrator,
            settings,
        }
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
        self.migrator.iter().map(|m| m.version).collect()
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
        self.migrator.iter().find(|m| m.version == version).map(|m| {
            if matches!(
                m.migration_type,
                MigrationType::ReversibleUp | MigrationType::ReversibleDown
            ) {
                Reversibility::Reversible
            } else {
                Reversibility::Irreversible
            }
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
            Some(Reversibility::Irreversible) => Err(DatabaseError::new(
                DatabaseErrorKind::MigrationIrreversible,
            )),
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

/// Generates the per-driver runner.
///
/// # Why a macro and not a generic function
///
/// `Migrator::run` takes an `Acquire` implementor, and `&mut <DB as Database>::Connection`
/// satisfies it only for concrete drivers — the bound cannot be written generically without
/// reaching for `run_direct`, which is `#[doc(hidden)]` and documented as semver-exempt. Two
/// twenty-line expansions of one body is a smaller risk than depending on a hidden item.
macro_rules! runner {
    ($name:ident, $driver:ty, $feature:literal) => {
        #[cfg(feature = $feature)]
        impl Migrations {
            /// Applies every pending migration on a **dedicated** connection.
            ///
            /// # The connection is closed on every failure path
            ///
            /// See the module documentation: SQLx leaks the migration lock when a migration fails.
            /// Closing the session releases a session-level lock unconditionally, which is a
            /// property of the protocol rather than of the driver remembering to unlock.
            ///
            /// # Errors
            ///
            /// [`DatabaseErrorKind::MigrationChecksumMismatch`] when an applied migration's content
            /// changed, [`DatabaseErrorKind::MigrationDirty`] when a previous run did not complete,
            /// [`DatabaseErrorKind::MigrationLockTimeout`] when the bounded wait elapsed, and
            /// [`DatabaseErrorKind::MigrationFailed`] otherwise.
            pub async fn $name(
                &self,
                database: &crate::SqlxDatabase<$driver>,
            ) -> Result<MigrationReport, DatabaseError> {
                let pool = database.pool();

                let before: Vec<i64> = sqlx::query_scalar(APPLIED_VERSIONS)
                    .fetch_all(pool)
                    .await
                    .unwrap_or_default();
                let started = Instant::now();

                let mut connection = pool
                    .acquire()
                    .await
                    .map_err(|inner| error::translate(&inner))?;

                let outcome = tokio::time::timeout(
                    self.settings.run_timeout(),
                    self.migrator.run(&mut *connection),
                )
                .await;

                match outcome {
                    Ok(Ok(())) => {
                        drop(connection);
                        let after: Vec<i64> = sqlx::query_scalar(APPLIED_VERSIONS)
                            .fetch_all(pool)
                            .await
                            .unwrap_or_default();
                        Ok(self.report(&before, &after, started.elapsed()))
                    }
                    Ok(Err(inner)) => {
                        // THE LOCK GUARD. Detaching removes the connection from the pool so it is
                        // never handed to another caller, and closing ends the session, which
                        // releases a session-level advisory lock whether or not `unlock()` ran.
                        let _ = connection.detach().close().await;
                        Err(error::translate(&sqlx::Error::Migrate(Box::new(inner))))
                    }
                    Err(_) => {
                        let _ = connection.detach().close().await;
                        Err(DatabaseError::new(DatabaseErrorKind::MigrationLockTimeout))
                    }
                }
            }
        }
    };
}

runner!(run_postgres, sqlx::Postgres, "db-postgres");
runner!(run_mysql, sqlx::MySql, "db-mysql");

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
impl Migrations {
    /// Builds the report by comparing the applied set before and after.
    fn report(&self, before: &[i64], after: &[i64], elapsed: std::time::Duration) -> MigrationReport {
        let mut report = MigrationReport::new();
        let newly = after.len().saturating_sub(before.len()).max(1);
        // Elapsed time is attributed evenly across newly-applied migrations. Per-migration timing
        // would need a hook SQLx does not expose, and inventing a precise-looking number would be
        // a measurement claim this code cannot support.
        let each = elapsed / u32::try_from(newly).unwrap_or(1);

        for migration in self.migrator.iter() {
            let outcome = if before.contains(&migration.version) {
                MigrationOutcome::AlreadyApplied
            } else {
                MigrationOutcome::Applied
            };
            let reversibility = if matches!(
                migration.migration_type,
                MigrationType::ReversibleUp | MigrationType::ReversibleDown
            ) {
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

    #[test]
    fn an_unknown_version_is_refused_rather_than_treated_as_reversible() {
        let migrations =
            Migrations::from_migrator(Migrator::with_migrations(Vec::new()), settings());
        assert_eq!(migrations.reversibility_of(1), None);
        let error = migrations.ensure_reversible(1).expect_err("refused");
        assert_eq!(error.kind(), DatabaseErrorKind::MigrationFailed);
    }
}
