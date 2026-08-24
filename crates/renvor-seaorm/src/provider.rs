//! The SeaORM database as a lifecycle provider and a readiness contributor.
//!
//! # The boot ordering is the requirement, not an implementation detail
//!
//! It is the same ordering `renvor-sqlx` established in Phase 006, and it is preserved here
//! deliberately rather than reinvented: a second persistence model that reported Ready under
//! different rules would make "the four rows pass the same contracts" false in the one place
//! nobody re-reads.

use std::sync::OnceLock;

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use renvor_core::error::BoxedCause;
use renvor_core::health::{Readiness, ReadinessContributor};
use renvor_core::provider::registry::{CapabilityId, ProviderId};
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use renvor_core::provider::registry::{InitContext, Provider, ProviderFuture};
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use renvor_database::Database;
use renvor_database::{ConnectionString, DatabaseKind, PoolSettings};

use crate::SeaOrmDatabase;
use crate::migrate::Migrations;

/// A SeaORM-backed database, booted and stopped by the kernel.
#[cfg_attr(
    not(any(feature = "db-postgres", feature = "db-mysql")),
    allow(
        dead_code,
        reason = "`provides`, `dsn` and `settings` are read by the per-driver `Provider` impl, \
                  which is feature-gated. With neither driver selected — the DEFAULT, since \
                  neither is a default feature — there is no impl to read them, and that is the \
                  configuration `cargo package` verifies"
    )
)]
pub struct SeaOrmProvider<DB: sqlx::Database> {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    dsn: ConnectionString,
    settings: PoolSettings,
    kind: DatabaseKind,
    database: OnceLock<SeaOrmDatabase<DB>>,
    /// `None` unless the deployment explicitly asked for migration on boot.
    migrations: Option<Migrations>,
}

impl<DB: sqlx::Database> core::fmt::Debug for SeaOrmProvider<DB> {
    /// Prints the identity and readiness, and nothing that could carry a credential.
    ///
    /// The [`ConnectionString`] is not printed even through its own redacting `Debug`.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SeaOrmProvider")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("booted", &self.database.get().is_some())
            .field("migrates_on_boot", &self.migrates_on_boot())
            .finish()
    }
}

impl<DB: sqlx::Database> SeaOrmProvider<DB> {
    /// Declares a database provider.
    pub fn new(
        id: ProviderId,
        capability: CapabilityId,
        dsn: ConnectionString,
        settings: PoolSettings,
        kind: DatabaseKind,
    ) -> Self {
        Self {
            id,
            provides: vec![capability],
            dsn,
            settings,
            kind,
            database: OnceLock::new(),
            migrations: None,
        }
    }

    /// Records the migration set and the policy that governs it.
    ///
    /// Both halves are required: supplying migrations is not the same as asking for schema change,
    /// and [`Self::migrates_on_boot`] answers `true` only when the policy is
    /// [`renvor_database::MigrationPolicy::OnBoot`].
    #[must_use]
    pub fn with_migrations(mut self, migrations: Migrations) -> Self {
        self.migrations = Some(migrations);
        self
    }

    /// Whether this provider will change the schema during Boot.
    #[must_use]
    pub fn migrates_on_boot(&self) -> bool {
        self.migrations
            .as_ref()
            .is_some_and(|migrations| migrations.settings().policy().runs_on_boot())
    }

    /// The shortest provider deadline under which this provider can honour its own bounds.
    ///
    /// The kernel's `DEFAULT_PROVIDER_DEADLINE` is 30 seconds and wraps the whole of
    /// `initialise`; the migration defaults are a 60-second lock wait and a 300-second run. With
    /// both at their defaults the kernel drops the future before either migration deadline can
    /// elapse, and the diagnostics that exist to distinguish *another process is migrating* from
    /// *your migration is too slow* can never be produced.
    ///
    /// Returns the kernel's own default when this provider does not migrate on boot.
    #[must_use]
    pub fn required_boot_deadline(&self) -> std::time::Duration {
        let Some(migrations) = self.migrations.as_ref() else {
            return renvor_core::lifecycle::application::DEFAULT_PROVIDER_DEADLINE;
        };
        if !migrations.settings().policy().runs_on_boot() {
            return renvor_core::lifecycle::application::DEFAULT_PROVIDER_DEADLINE;
        }
        // Lock wait, then run, then the bounded close of the migration session. Serial, because
        // that is the order they occur in.
        let needed = migrations.settings().lock_timeout()
            + migrations.settings().run_timeout()
            + crate::migrate::CLEANUP_TIMEOUT;
        needed.max(renvor_core::lifecycle::application::DEFAULT_PROVIDER_DEADLINE)
    }

    /// The booted database, or `None` before Boot has reached this provider.
    #[must_use]
    pub fn database(&self) -> Option<&SeaOrmDatabase<DB>> {
        self.database.get()
    }
}

/// Generates the `Provider` implementation for one concrete driver.
///
/// A generic impl cannot be written, for the reason `migrate.rs` records: the boxed
/// [`ProviderFuture`] erases every region, and the migration entry point is driver-concrete.
macro_rules! provider_for {
    ($driver:ty, $feature:literal, $run:ident) => {
        #[cfg(feature = $feature)]
        impl Provider for SeaOrmProvider<$driver> {
            fn id(&self) -> &ProviderId {
                &self.id
            }

            fn provides(&self) -> &[CapabilityId] {
                &self.provides
            }

            /// Connects, proves the database answers, migrates if asked, and only then publishes.
            ///
            /// ```text
            /// connect pool
            ///   -> prove connectivity                     (one round trip, not just a socket)
            ///   -> if the policy is OnBoot:
            ///        dedicated migration connection
            ///        apply under the lock and run deadlines
            ///        end that session whatever happened
            ///   -> publish the database into provider state
            ///   -> readiness may report Ready
            /// ```
            ///
            /// Every failure short-circuits **before** `self.database.set`, so a provider that did
            /// not finish migrating has no database to hand out and readiness answers `NotReady`
            /// for the only reason it can: there is nothing there.
            ///
            /// A failed boot closes the pool it opened, so a crash-looping deployment does not
            /// exhaust the server's connections for everything else on the same database.
            fn initialise<'a>(&'a self, _context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
                Box::pin(async move {
                    let database =
                        SeaOrmDatabase::<$driver>::connect(&self.dsn, &self.settings, self.kind)
                            .await
                            .map_err(|error| Box::new(error) as BoxedCause)?;

                    if let Err(error) = database.check().await {
                        let _ = database.close().await;
                        return Err(Box::new(error) as BoxedCause);
                    }

                    if let Some(migrations) = self.migrations.as_ref() {
                        if migrations.settings().policy().runs_on_boot() {
                            if let Err(error) = migrations.$run(&database).await {
                                let _ = database.close().await;
                                return Err(Box::new(error) as BoxedCause);
                            }
                        }
                    }

                    // `set` returns the value back on a second call, which cannot happen — the
                    // kernel initialises each provider once — so the result is discarded rather
                    // than unwrapped, keeping a panic off a Boot path.
                    let _ = self.database.set(database);
                    Ok(())
                })
            }

            /// Drains the pool within its configured bound.
            ///
            /// A forced close is reported rather than swallowed (principle IV).
            fn stop(&self) -> ProviderFuture<'_> {
                Box::pin(async move {
                    match self.database.get() {
                        // Nothing was opened. Not an error: `stop` also runs during rollback.
                        None => Ok(()),
                        Some(database) => database
                            .close()
                            .await
                            .map_err(|error| Box::new(error) as BoxedCause),
                    }
                })
            }
        }
    };
}

provider_for!(sqlx::Postgres, "db-postgres", run_postgres);
provider_for!(sqlx::MySql, "db-mysql", run_mysql);

impl<DB: sqlx::Database> ReadinessContributor for SeaOrmProvider<DB> {
    fn name(&self) -> &str {
        self.id.as_str()
    }

    /// Ready only once Boot has opened the pool and the database has answered.
    ///
    /// It observes that the boot-time check passed and that the pool has not been closed. It is
    /// **not** a continuous probe: a database that becomes unreachable after Boot does not flip
    /// this to `NotReady` on its own. That is a limit of the trait — `readiness` is synchronous
    /// and a round trip is not — stated here so nobody reads it as a liveness check.
    fn readiness(&self) -> Readiness {
        match self.database.get() {
            Some(database) if !database.pool().is_closed() => Readiness::Ready,
            _ => Readiness::NotReady,
        }
    }
}
