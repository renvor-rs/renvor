//! The database as a lifecycle provider and a readiness contributor.
//!
//! # Why the pool is opened during Boot rather than on first use
//!
//! FR-011. A pool opened lazily reports a wrong connection string as a failed *request*, minutes or
//! hours after deployment, to whoever happened to be first. Opening it during Boot turns the same
//! mistake into a failed start, which is the moment an operator is watching and the moment a
//! rollback is still cheap. The kernel then stops every provider already initialised, in reverse
//! order, without this module arranging anything.

use std::sync::OnceLock;

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use renvor_core::error::BoxedCause;
use renvor_core::health::{Readiness, ReadinessContributor};
use renvor_core::provider::registry::{CapabilityId, ProviderId};
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use renvor_core::provider::registry::{InitContext, Provider, ProviderFuture};
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use renvor_database::Database;
use renvor_database::{
    ConnectionString, DatabaseKind, PoolSettings, StartupDiagnostic, StartupPhase,
};

/// This adapter's own crate name, as it appears in a startup diagnostic.
///
/// A literal rather than anything derived from configuration: `StartupDiagnostic` takes
/// `&'static str` precisely so that the adapter cannot be named by a formatted string.
const ADAPTER: &str = "renvor-sqlx";

use crate::SqlxDatabase;
use crate::migrate::Migrations;

/// A database, booted and stopped by the kernel.
///
/// # Why the pool lives in a `OnceLock`
///
/// [`renvor_core::provider::registry::Provider::initialise`] takes `&self`, because the kernel holds providers behind shared
/// references while it drives Boot. A `OnceLock` is the smallest thing that lets initialisation
/// publish a value without a lock on the read path — and "written exactly once, by Boot" is
/// precisely what a `OnceLock` promises, so the invariant is enforced by the type rather than by a
/// comment asking callers not to write twice.
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
pub struct SqlxProvider<DB: sqlx::Database> {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    dsn: ConnectionString,
    settings: PoolSettings,
    kind: DatabaseKind,
    database: OnceLock<SqlxDatabase<DB>>,
    /// `None` unless the deployment explicitly asked for migration on boot.
    ///
    /// FR-021. Reaching `Some` requires two separate acts: supplying migrations, and declaring
    /// [`renvor_database::MigrationPolicy::OnBoot`]. Neither is the default, so schema change
    /// during startup is never something a deployment falls into by leaving a field unset.
    migrations: Option<Migrations>,
}

impl<DB: sqlx::Database> core::fmt::Debug for SqlxProvider<DB> {
    /// Prints the identity and readiness, and nothing that could carry a credential.
    ///
    /// The [`ConnectionString`] is not printed even through its own redacting `Debug`: a provider
    /// appears in kernel diagnostics, and the fewer paths a DSN can travel, the fewer there are to
    /// audit.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SqlxProvider")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("booted", &self.database.get().is_some())
            .field("migrates_on_boot", &self.migrates_on_boot())
            .finish()
    }
}

impl<DB: sqlx::Database> SqlxProvider<DB> {
    /// Declares a database provider.
    ///
    /// `capability` is what other providers depend on to be ordered after this one.
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
    /// # Both halves are required, and that is the point
    ///
    /// FR-021. The `migrations` argument carries its own
    /// [`renvor_database::MigrationSettings`], and [`SqlxProvider::migrates_on_boot`] answers
    /// `true` only when that settings' policy is
    /// [`renvor_database::MigrationPolicy::OnBoot`]. A caller who supplies migrations but leaves
    /// the policy at its default has not asked for schema change — which is the safe reading of an
    /// ambiguous configuration, and the reading the default was chosen to produce.
    ///
    /// See [`renvor_core::provider::registry::Provider::initialise`] for when the recorded policy is acted on.
    #[must_use]
    pub fn with_migrations(mut self, migrations: Migrations) -> Self {
        self.migrations = Some(migrations);
        self
    }

    /// Whether this provider will change the schema during Boot.
    ///
    /// Exposed so a deployment can read the decision back, rather than an operator having to infer
    /// it from two separate settings. `true` here means Boot **applies** migrations — see
    /// [`renvor_core::provider::registry::Provider::initialise`].
    #[must_use]
    pub fn migrates_on_boot(&self) -> bool {
        self.migrations
            .as_ref()
            .is_some_and(|migrations| migrations.settings().policy().runs_on_boot())
    }

    /// The shortest provider deadline under which this provider can honour its own bounds.
    ///
    /// # The kernel's default is shorter than the migration defaults, and that silently wins
    ///
    /// `renvor_core`'s `DEFAULT_PROVIDER_DEADLINE` is **30 seconds** and it wraps the whole of
    /// [`renvor_core::provider::registry::Provider::initialise`]. The migration defaults are a **60-second** lock wait and a
    /// **300-second** run. With both at their defaults, the kernel drops this future long before
    /// either migration deadline can elapse, and three things follow:
    ///
    /// - [`renvor_database::DatabaseErrorKind::MigrationLockTimeout`] — the diagnostic whose entire
    ///   purpose is to say *another process is migrating* — can never be produced;
    /// - neither can [`renvor_database::DatabaseErrorKind::DeadlineExceeded`] from the run bound.
    ///   `migrate.rs` argues at length that reporting both as one kind sends an operator to the
    ///   wrong place; under defaults they are reported as a **third** kind that mentions neither
    ///   migrations nor locks;
    /// - the cleanup on this crate's returned-error paths does not run at all, because the kernel
    ///   does not take a returned error — it drops the future. Cleanup then falls to
    ///   `close_on_drop`, which is bounded but is not the ordered, awaited close those paths were
    ///   written to perform.
    ///
    /// So an application that declares [`renvor_database::MigrationPolicy::OnBoot`] must give the
    /// kernel a deadline at least this long. The number is returned rather than described, so it is
    /// not a value an application author has to derive from three constants in two crates:
    ///
    /// ```text
    /// let provider = SqlxProvider::<sqlx::Postgres>::new(...).with_migrations(migrations);
    /// let application = Application::builder()
    ///     .with_provider_deadline(provider.required_boot_deadline())?
    ///     .build()?;
    /// ```
    ///
    /// Returns the kernel's own default when this provider does not migrate on boot, because then
    /// there is nothing extra to accommodate.
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
    ///
    /// `None` is a fact about the lifecycle phase rather than an error: a caller holding this
    /// before Boot is asking too early, and saying so is more useful than a panic.
    #[must_use]
    pub fn database(&self) -> Option<&SqlxDatabase<DB>> {
        self.database.get()
    }
}

/// Generates the `Provider` implementation for one concrete driver.
///
/// # Why this is a macro and not a generic impl
///
/// [`Migrations::run_postgres`] and [`Migrations::run_mysql`] are driver-concrete, and a generic
/// impl would have to reach them through a trait whose bound cannot be written: the boxed
/// [`renvor_core::provider::registry::ProviderFuture`] erases every region, and `sqlx::Acquire` is implemented for one region at a
/// time. `migrate.rs` reaches for the same construction for the same reason, which is the
/// strongest argument that it is the shape of the problem rather than a shortcut.
macro_rules! provider_for {
    ($driver:ty, $feature:literal, $run:ident) => {
        #[cfg(feature = $feature)]
        impl Provider for SqlxProvider<$driver> {
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
            /// # The order is the requirement, not an implementation detail
            ///
            /// FR-021. Every failure above short-circuits **before** `self.database.set`, so a
            /// provider that did not finish migrating has no database to hand out and
            /// [`renvor_core::health::ReadinessContributor::readiness`] answers `NotReady` for the only reason it can:
            /// there is nothing there. A migration that ran *after* publication would leave a
            /// window in which the application is ready against a schema that does not exist yet,
            /// which is the failure this ordering exists to make unrepresentable.
            ///
            /// Connectivity is proved separately from connecting because opening a pool can
            /// succeed against a host that accepts TCP and serves nothing.
            /// [`renvor_database::Database::check`] costs one round trip and closes that gap.
            ///
            /// # A failed boot closes the pool it opened
            ///
            /// Otherwise a refused start leaves its connections on the server until the process
            /// exits — and a crash-looping deployment turns that into connection exhaustion for
            /// everything else on the same database.
            fn initialise<'a>(&'a self, _context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
                Box::pin(async move {
                    let database =
                        SqlxDatabase::<$driver>::connect(&self.dsn, &self.settings, self.kind)
                            .await
                            .map_err(|error| {
                                Box::new(StartupDiagnostic::new(
                                    ADAPTER,
                                    self.kind,
                                    StartupPhase::Connect,
                                    error.kind(),
                                )) as BoxedCause
                            })?;

                    if let Err(error) = database.check().await {
                        let _ = database.close().await;
                        return Err(Box::new(StartupDiagnostic::new(
                            ADAPTER,
                            self.kind,
                            StartupPhase::Readiness,
                            error.kind(),
                        )) as BoxedCause);
                    }

                    // The two halves of FR-021 are checked together: migrations supplied, AND a
                    // policy that asked for them. Either alone is not a request to change a schema.
                    if let Some(migrations) = self.migrations.as_ref() {
                        if migrations.settings().policy().runs_on_boot() {
                            if let Err(error) = migrations.$run(&database).await {
                                let _ = database.close().await;
                                return Err(Box::new(StartupDiagnostic::new(
                                    ADAPTER,
                                    self.kind,
                                    StartupPhase::Migration,
                                    error.kind(),
                                )) as BoxedCause);
                            }
                        }
                    }

                    // `set` returns the value back on a second call. That cannot happen — the
                    // kernel initialises each provider once — so the result is discarded rather
                    // than unwrapped, which keeps a panic off a Boot path for a case the lifecycle
                    // already prevents.
                    let _ = self.database.set(database);
                    Ok(())
                })
            }

            /// Drains the pool within its configured bound.
            ///
            /// A forced close is reported rather than swallowed: constitution principle IV treats
            /// forced termination as a visible event, and a shutdown reporting success while
            /// abandoning connections would hide the condition an operator needs to see.
            fn stop(&self) -> ProviderFuture<'_> {
                Box::pin(async move {
                    match self.database.get() {
                        // Nothing was opened, so there is nothing to close. Not an error: `stop`
                        // also runs during rollback, where a provider may never have initialised.
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

impl<DB: sqlx::Database> ReadinessContributor for SqlxProvider<DB> {
    fn name(&self) -> &str {
        self.id.as_str()
    }

    /// Ready only once Boot has opened the pool and the database has answered.
    ///
    /// # What this observes, and what it does not
    ///
    /// It observes that the boot-time [`renvor_database::Database::check`] passed and that the pool has not been
    /// closed. It is **not** a continuous probe: a database that becomes unreachable after Boot
    /// does not flip this to `NotReady` on its own.
    ///
    /// That is a limit of the trait rather than a choice — [`renvor_core::health::ReadinessContributor::readiness`] is
    /// synchronous, and a round trip is not. The alternatives were both worse: blocking a readiness
    /// probe on I/O makes the probe itself a failure mode, and a background task refreshing a
    /// cached verdict is the unbounded orphaned work principle VI prohibits. Stated here rather
    /// than implied, so nobody reads this as a liveness check.
    fn readiness(&self) -> Readiness {
        match self.database.get() {
            Some(database) if !database.pool().is_closed() => Readiness::Ready,
            _ => Readiness::NotReady,
        }
    }
}
