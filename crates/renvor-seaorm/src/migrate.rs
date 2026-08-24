//! Migrations for the SeaORM adapter — run by SQLx's engine, deliberately.
//!
//! # Why not `sea-orm-migration`
//!
//! It was evaluated first, as constitution principle III requires, and it was **rejected on a
//! measured capability gap** rather than on preference. Its bookkeeping table is, in full:
//!
//! ```text
//! #[sea_orm(table_name = "seaql_migrations")]
//! pub struct Model {
//!     #[sea_orm(primary_key, auto_increment = false)]
//!     pub version: String,
//!     pub applied_at: i64,
//! }
//! ```
//!
//! Two columns. **There is no checksum**, and the string `checksum` does not appear anywhere in
//! `sea-orm-migration` 2.0.2. A migration whose body is edited after it has been applied is
//! therefore undetectable: the version is still recorded, so the run reports success and the
//! schema silently disagrees with the source. `PLAN.md` §12 requires migrations to be
//! *"ordered, checksummed, observable, and safe under concurrent startup"*, and FR-023 requires a
//! changed migration to be refused **before** any schema modification.
//!
//! SQLx's `_sqlx_migrations` carries `checksum` and `success` columns and enforces both. Using it
//! also means a project has exactly **one** migration history whichever ORM it selected, so
//! switching between them is not a re-migration — and the phase brief's prohibition on
//! *"two competing migration histories"* is satisfied structurally rather than by a warning.
//!
//! The cost is stated rather than absorbed: migrations are **SQL files**, not Rust
//! `MigrationTrait` implementations. A team that wants SeaORM's Rust-authored migrations does not
//! get them here. That is a real trade, and it buys a tamper gate that the alternative does not
//! have at all. See ADR-0022.
//!
//! # Why this is not shared with `renvor-sqlx`
//!
//! Both adapters wrap the same **public** SQLx API, independently. Sharing would mean
//! `renvor-seaorm` depending on `renvor-sqlx`, which would put a direct-SQLx crate into every
//! SeaORM application's graph and give a facade re-export somewhere to leak from — the exact
//! accident `PLAN.md` §Phase 007 names. The two are held equivalent by the **shared contract
//! suite** running against both, not by shared source.

use std::path::Path;
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use std::time::Instant;

use renvor_database::{DatabaseError, DatabaseErrorKind, MigrationSettings, Reversibility};
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use renvor_database::{MigrationOutcome, MigrationReport, MigrationStep};
use sqlx::migrate::Migrator;

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use crate::error;

/// A loaded, ordered migration set with its deadlines.
pub struct Migrations {
    migrator: Migrator,
    settings: MigrationSettings,
}

impl core::fmt::Debug for Migrations {
    /// Prints counts, never a filesystem path — a path is a deployment detail.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Migrations")
            .field("migrations", &self.versions().len())
            .finish()
    }
}

impl Migrations {
    /// Loads migrations from a directory at **runtime**.
    ///
    /// Runtime rather than `migrate!`: the macro embeds the directory at compile time, which ties
    /// the binary to the migration set present when it was built.
    ///
    /// # Errors
    ///
    /// [`DatabaseErrorKind::MigrationFailed`] when the directory cannot be read or a migration is
    /// malformed. The path is **not** carried in the error.
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
    /// SQLx would otherwise lock inside `run_direct` with an **unbounded** wait, which principle
    /// VI does not permit. The runner below takes the same lock through the same public
    /// [`sqlx::migrate::Migrate`] trait, under [`MigrationSettings::lock_timeout`].
    ///
    /// This cannot be caught behaviourally: both engines' migration locks are **re-entrant within
    /// one session**, so a `run_direct` that locked again on a connection this crate had already
    /// locked would succeed instantly and every real-database test would still pass. A white-box
    /// unit test pins it.
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
    #[must_use]
    pub fn versions(&self) -> Vec<i64> {
        self.forward().map(|m| m.version).collect()
    }

    /// The migrations that are applied moving **forward**.
    ///
    /// `Migrator::iter()` yields a `ReversibleUp` **and** a `ReversibleDown` entry for each
    /// reversible migration, both carrying the same version, so counting the raw iterator reports
    /// every reversible migration twice.
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

    /// Whether the set is strictly ordered, independent of directory enumeration order.
    #[must_use]
    pub fn is_ordered(&self) -> bool {
        self.versions().windows(2).all(|pair| match pair {
            [first, second] => first < second,
            _ => true,
        })
    }

    /// Whether a given migration declares a rollback.
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

/// Reads the applied versions. A missing table before the first run yields an empty set.
///
/// The statement is a constant with no interpolation, so nothing untrusted reaches it.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
const APPLIED_VERSIONS: &str = "SELECT version FROM _sqlx_migrations ORDER BY version";

/// Whether a failed read of the bookkeeping table means "it does not exist yet".
///
/// # Why this is not `unwrap_or_default()`
///
/// It was. Before the first run the table genuinely does not exist, and an empty set is the truth
/// — but the same fallback also swallowed a denied privilege, a transient failure, and pool
/// exhaustion. The report is then built by comparing an empty "before" against the real "after",
/// so **every** migration is reported `Applied` and the elapsed time is divided across a count
/// that never happened. A security review found it, and it sits badly in a crate whose migration
/// engine was chosen over `sea-orm-migration` precisely because that one cannot tell you when its
/// bookkeeping is wrong.
///
/// The report is boot evidence. Evidence that fabricates itself when it cannot read is worse than
/// evidence that says it could not read.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
fn table_is_simply_absent(error: &sqlx::Error) -> bool {
    // Both engines report an unknown relation through `Error::Database` with a SQLSTATE:
    // PostgreSQL `42P01` (undefined_table), MySQL `42S02` (base table not found).
    matches!(error, sqlx::Error::Database(inner)
        if matches!(inner.code().as_deref(), Some("42P01" | "42S02")))
}

/// How long the migration session is given to close itself.
///
/// Public and **not** feature-gated: [`crate::provider::SeaOrmProvider::required_boot_deadline`]
/// composes it, and that method exists on the generic provider, which compiles with no driver
/// feature at all. Gating it broke exactly the configuration `cargo package` verifies — a defect
/// Phase 006 found only at packaging time, and which is not repeated here.
///
/// Matches SQLx's own `CLOSE_ON_DROP_TIMEOUT` rather than inventing a second number.
pub const CLEANUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Generates the per-driver migration runner.
///
/// The bodies are identical two expansions of one text, for the reason `renvor-sqlx` records: a
/// generic signature over `Migrate` cannot be written, because the higher-ranked region obligation
/// it would carry cannot be discharged for all lifetimes at once (ADR-0018).
macro_rules! runner {
    ($name:ident, $driver:ty, $feature:literal) => {
        #[cfg(feature = $feature)]
        impl Migrations {
            /// Applies every pending migration on a **dedicated** connection, under two deadlines.
            ///
            /// # The session always ends here
            ///
            /// Not "on the failure paths" — always. A migration holds a database-wide lock
            /// released by the session dying, so the connection is marked
            /// [`close_on_drop`](sqlx::pool::PoolConnection::close_on_drop) the instant it is
            /// acquired — covering cancellation and panic, where no code of ours runs — and is
            /// detached and closed explicitly on every path that returns normally.
            ///
            /// # Two deadlines, because they mean different things
            ///
            /// A lock wait that elapses means *another process is migrating*. A run that elapses
            /// means *your migration is too slow*. One kind for both sends an operator to the
            /// wrong place.
            ///
            /// # Errors
            ///
            /// [`DatabaseErrorKind::MigrationLockTimeout`] when the lock wait elapsed,
            /// [`DatabaseErrorKind::DeadlineExceeded`] when the whole run elapsed,
            /// [`DatabaseErrorKind::MigrationChecksumMismatch`] when an applied migration's
            /// content changed, [`DatabaseErrorKind::MigrationDirty`] when a previous run did not
            /// complete, and [`DatabaseErrorKind::MigrationFailed`] otherwise.
            pub async fn $name(
                &self,
                database: &crate::SeaOrmDatabase<$driver>,
            ) -> Result<MigrationReport, DatabaseError> {
                use sqlx::Connection as _;
                use sqlx::migrate::Migrate as _;

                let pool = database.pool();

                let before: Vec<i64> =
                    match sqlx::query_scalar(APPLIED_VERSIONS).fetch_all(pool).await {
                        Ok(versions) => versions,
                        // The first run. There is nothing applied, and that IS the answer.
                        Err(inner) if table_is_simply_absent(&inner) => Vec::new(),
                        Err(inner) => return Err(error::classify_error(&inner)),
                    };
                let started = Instant::now();

                let mut connection = pool
                    .acquire()
                    .await
                    .map_err(|inner| error::classify_error(&inner))?;
                // The cleanup guard, armed before anything can go wrong.
                connection.close_on_drop();

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

                // The one cleanup. Bounded, because a wedged socket must not become a hung boot.
                let _ = tokio::time::timeout(CLEANUP_TIMEOUT, connection.detach().close()).await;

                outcome?;

                // The migration itself has already succeeded by here — `outcome?` is above. A
                // failure to read the table back is therefore a failure to REPORT, and it is
                // returned rather than papered over with an empty set that would mark every
                // migration newly applied.
                let after: Vec<i64> = sqlx::query_scalar(APPLIED_VERSIONS)
                    .fetch_all(pool)
                    .await
                    .map_err(|inner| error::classify_error(&inner))?;
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
        // would need a hook SQLx does not expose, and a precise-looking number this code cannot
        // measure would be a claim rather than a measurement.
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

    /// Renvor owns the migration lock; SQLx must not also be taking it.
    ///
    /// Asserted on the field rather than through a database because the behavioural test cannot
    /// see it: `pg_advisory_lock` and `GET_LOCK` are both re-entrant within one session, so a
    /// `run_direct` that locked again would succeed instantly and the whole real-database suite
    /// would still pass. Mutation testing found this in Phase 006; the same guard is needed here
    /// because this crate calls `run_direct` itself rather than reusing the other adapter's.
    #[test]
    fn renvor_owns_the_migration_lock_rather_than_sqlx() {
        let migrations =
            Migrations::from_migrator(Migrator::with_migrations(Vec::new()), settings());
        assert!(
            !migrations.migrator.locking,
            "sqlx is taking the migration lock again, with an unbounded wait"
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
