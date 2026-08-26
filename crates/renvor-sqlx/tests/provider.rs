//! The database boots with the application, or the application does not boot.
//!
//! FR-011 and FR-012. Both are about *when* a failure surfaces rather than whether it does: a pool
//! opened lazily reports a wrong connection string as a failed request, to whoever happens to be
//! first, long after the deployment that caused it. These assert that the same mistake fails Boot.

mod support;

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use renvor_core::health::{Readiness, ReadinessContributor};
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use renvor_core::provider::registry::{CapabilityId, Provider, ProviderId};

macro_rules! suite {
    ($module:ident, $feature:literal, $driver:ty, $url:expr, $kind:expr) => {
        #[cfg(feature = $feature)]
        mod $module {
            use super::*;
            use renvor_database::{ConnectionString, DatabaseKind};
            use renvor_sqlx::provider::SqlxProvider;

            fn provider(dsn: ConnectionString) -> SqlxProvider<$driver> {
                SqlxProvider::new(
                    ProviderId::new("database"),
                    CapabilityId::new("database"),
                    dsn,
                    support::settings(),
                    $kind,
                )
            }

            /// A provider that has not booted is NOT ready, and says so.
            ///
            /// The default for `Readiness` is `Ready`, so a contributor that forgot to answer would
            /// report ready. This pins the answer rather than the default.
            #[tokio::test]
            async fn a_database_that_has_not_booted_is_not_ready() {
                let provider = provider(ConnectionString::new("unused".to_owned()));
                assert_eq!(provider.readiness(), Readiness::NotReady);
                assert!(provider.database().is_none());
            }

            /// FR-011: an unreachable database fails BOOT, not the first request.
            #[tokio::test]
            async fn an_unreachable_database_fails_initialisation() {
                // Port 1 has nothing listening on it, so this cannot succeed for an incidental
                // reason such as a warm pool or a cached lookup.
                let dsn = ConnectionString::new(match $kind {
                    DatabaseKind::Postgres => "postgres://u:p@127.0.0.1:1/none".to_owned(),
                    _ => "mysql://u:p@127.0.0.1:1/none".to_owned(),
                });
                let provider = provider(dsn);
                let outcome = support::initialise(&provider).await;
                assert!(outcome.is_err(), "an unreachable database booted");
                assert_eq!(
                    provider.readiness(),
                    Readiness::NotReady,
                    "a failed boot must not leave the database reporting ready"
                );
                // The refusal must not repeat the DSN. This is the whole reason `DatabaseError`
                // has one field.
                let rendered = format!("{:?}", outcome.unwrap_err());
                for (needle_index, secret) in ["127.0.0.1", ":p@", "none"].into_iter().enumerate() {
                    assert!(
                        !rendered.contains(secret),
                        "the boot failure leaked DSN needle {needle_index}"
                    );
                }
            }

            /// FR-012: ready only after the database has actually answered.
            #[tokio::test]
            async fn a_booted_database_is_ready_and_stops_cleanly() {
                let Some(dsn) = support::url($url) else {
                    return;
                };
                let provider = provider(dsn);
                assert_eq!(provider.readiness(), Readiness::NotReady, "before boot");

                support::initialise(&provider).await.expect("boots");
                assert_eq!(provider.readiness(), Readiness::Ready, "after boot");
                assert!(provider.database().is_some());

                provider.stop().await.expect("stops");
                assert_eq!(
                    provider.readiness(),
                    Readiness::NotReady,
                    "a stopped database must stop reporting ready"
                );
            }

            /// Stopping a provider that never booted is not an error.
            ///
            /// `stop` also runs during rollback, where a provider may never have been initialised.
            /// Reporting that as a failure would make every rollback report a second, invented one.
            #[tokio::test]
            async fn stopping_an_unbooted_database_is_not_a_failure() {
                let provider = provider(ConnectionString::new("unused".to_owned()));
                assert!(provider.stop().await.is_ok());
            }

            // ── FR-021: schema change during boot is an explicit, recorded choice ─────

            async fn migrations(
                policy: renvor_database::MigrationPolicy,
            ) -> renvor_sqlx::migrate::Migrations {
                let settings = renvor_database::MigrationSettings::default().with_policy(policy);
                renvor_sqlx::migrate::Migrations::load(
                    std::path::Path::new("tests/migrations"),
                    settings,
                )
                .await
                .expect("the migration directory loads")
            }

            /// A provider with no migrations never changes the schema.
            #[tokio::test]
            async fn a_provider_without_migrations_does_not_migrate_on_boot() {
                let provider = provider(ConnectionString::new("unused".to_owned()));
                assert!(!provider.migrates_on_boot());
            }

            /// SUPPLYING migrations is not the same as ASKING for them to run.
            ///
            /// The default policy is `Never`, so a deployment that hands over a migration
            /// directory and leaves the policy alone gets a boot that does not touch the schema.
            /// That is the whole of FR-021: two separate acts, neither of them a default.
            #[tokio::test]
            async fn supplying_migrations_is_not_asking_for_them() {
                let loaded = migrations(renvor_database::MigrationPolicy::Never).await;
                let provider =
                    provider(ConnectionString::new("unused".to_owned())).with_migrations(loaded);
                assert!(
                    !provider.migrates_on_boot(),
                    "the default policy applied migrations"
                );
            }

            /// Declaring `OnBoot` is RECORDED, and the provider says so.
            ///
            /// The provider does not itself apply migrations — `sqlx`'s migration future is not
            /// `Send` and the kernel's provider future must be. What FR-021 requires is that the
            /// choice be explicit and recorded rather than reachable by leaving a field unset, and
            /// that is what is asserted here.
            #[tokio::test]
            async fn declaring_on_boot_is_recorded() {
                let loaded = migrations(renvor_database::MigrationPolicy::OnBoot).await;
                let provider =
                    provider(ConnectionString::new("unused".to_owned())).with_migrations(loaded);
                assert!(provider.migrates_on_boot());
            }

            #[tokio::test]
            async fn the_provider_declares_the_capability_it_was_given() {
                let provider = provider(ConnectionString::new("unused".to_owned()));
                assert_eq!(provider.id().as_str(), "database");
                assert_eq!(provider.provides().len(), 1);
                assert_eq!(provider.provides()[0].as_str(), "database");
            }

            /// A provider's `Debug` renders exactly the fields it declares, and nothing else.
            ///
            /// # Why this is not the canary test it replaces
            ///
            /// That test planted a fake password in a DSN and asserted the password was absent
            /// from the rendering. It worked, and it had two problems.
            ///
            /// The smaller one: it proved the absence of **one string somebody thought of**. A
            /// field added later that leaked the host, the port, or the database name would have
            /// passed it.
            ///
            /// The larger one: CodeQL read `format!("{provider:?}")` on a value built from
            /// something named `CREDENTIAL_CANARY` and raised two high-severity
            /// `rust/cleartext-logging` alerts — on the line that *proves* nothing leaks. It was a
            /// false positive, and the honest fix is not to dismiss it but to stop writing a
            /// redaction proof that requires planting a credential to make its point.
            ///
            /// So: the DSN below carries no password at all, and the assertion is **structural**.
            /// `SqlxProvider`'s `Debug` declares four fields; this asserts the rendering carries
            /// those four and no fifth. A new field that carried the connection string fails here
            /// without anyone having to guess which substring to search for.
            #[tokio::test]
            async fn the_provider_debug_renders_only_its_declared_fields() {
                // DELIBERATELY NOT URL-SHAPED, and that is the second correction.
                //
                // The first rewrite dropped the password but kept
                // `postgres://host:5432/name`, and CodeQL still read a connection URL flowing
                // into a format as a credential reaching a sink. A `ConnectionString` holds
                // whatever the operator hands it; the property under test is that `Debug` does
                // not print it, and that property does not depend on the value looking like a
                // URL. So the value is a marker, and the assertions below are unchanged in
                // strength.
                let provider = provider(ConnectionString::new("zz-connection-marker-zz"));
                let rendered = format!("{provider:?}");

                // THE STRUCTURAL HALF. `debug_struct` renders `field: value` per entry, so the
                // count of separators is the count of fields.
                const DECLARED: [&str; 4] = ["id: ", "kind: ", "booted: ", "migrates_on_boot: "];
                for field in DECLARED {
                    assert_eq!(
                        rendered.matches(field).count(),
                        1,
                        "the provider's Debug no longer renders a field it declares"
                    );
                }
                assert_eq!(
                    rendered.matches(": ").count(),
                    DECLARED.len(),
                    "the provider's Debug grew a field, and a new field is how a connection \
                     string reaches a diagnostic"
                );

                // THE ABSENCE HALF. One marker, because the connection string is one value and
                // the claim is that none of it survives into a diagnostic.
                assert!(
                    !rendered.contains("zz-connection-marker-zz"),
                    "the connection string reached the provider's Debug"
                );
            }
        }
    };
}

suite!(
    postgres,
    "db-postgres",
    sqlx::Postgres,
    support::POSTGRES_URL,
    DatabaseKind::Postgres
);

suite!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    support::MYSQL_URL,
    DatabaseKind::MySql
);

// GATED ON `db-postgres` ALONE, and it used to be `any(db-postgres, db-mysql)`.
//
// Every test in here is `db-postgres`-only and always was, but the module's helper returns
// `SqlxProvider<sqlx::Postgres>` unconditionally. Under `--features db-mysql` on its own the
// module therefore held a helper naming a type that was not compiled, and this crate did not
// build — while `cargo tree` reported the driver isolation intact, because resolving and
// compiling are different questions. `xtask`'s `adapters_compile_per_driver` now asks the second
// one.
//
// The deadline arithmetic under test is engine-independent; the `Postgres` type parameter is
// incidental to it. Gating the module the way its tests were already gated is the smallest change
// that makes the file honest, and it removes no coverage — none of these ran under MySQL before.
#[cfg(feature = "db-postgres")]
mod boot_deadline {
    use std::time::Duration;

    use renvor_core::lifecycle::application::DEFAULT_PROVIDER_DEADLINE;
    use renvor_core::provider::registry::{CapabilityId, ProviderId};
    use renvor_database::{
        ConnectionString, DatabaseKind, MigrationPolicy, MigrationSettings, PoolSettings,
    };
    use renvor_sqlx::Migrations;
    use renvor_sqlx::provider::SqlxProvider;

    fn provider_with(settings: MigrationSettings) -> SqlxProvider<sqlx::Postgres> {
        SqlxProvider::new(
            ProviderId::new("database"),
            CapabilityId::new("database"),
            ConnectionString::new("unused"),
            PoolSettings::default(),
            DatabaseKind::Postgres,
        )
        .with_migrations(Migrations::from_migrator(
            sqlx::migrate::Migrator::with_migrations(Vec::new()),
            settings,
        ))
    }

    /// The kernel's default deadline is shorter than the migration defaults, and the provider says
    /// so rather than leaving an operator to derive it.
    ///
    /// With `OnBoot` and default bounds, a 30-second provider deadline drops the future before
    /// either migration deadline can elapse — so `MigrationLockTimeout` and `DeadlineExceeded`
    /// become unreachable and the operator sees a kernel timeout that mentions neither migrations
    /// nor locks. This asserts the number that avoids it.
    #[test]
    fn migrating_on_boot_needs_more_than_the_kernels_default_deadline() {
        let on_boot =
            provider_with(MigrationSettings::default().with_policy(MigrationPolicy::OnBoot));
        let needed = on_boot.required_boot_deadline();
        assert!(
            needed > DEFAULT_PROVIDER_DEADLINE,
            "the default migration bounds fit inside the kernel default, so this test is stale"
        );
        // lock 60 + run 300 + cleanup 5.
        assert_eq!(needed, Duration::from_secs(365));
    }

    /// A provider that does NOT migrate needs nothing extra.
    #[test]
    fn not_migrating_on_boot_needs_only_the_kernels_default() {
        let never = provider_with(MigrationSettings::default());
        assert_eq!(never.required_boot_deadline(), DEFAULT_PROVIDER_DEADLINE);
    }

    /// The answer tracks the configured bounds rather than the defaults.
    #[test]
    fn the_required_deadline_follows_the_configured_bounds() {
        let tight = MigrationSettings::default()
            .with_policy(MigrationPolicy::OnBoot)
            .with_lock_timeout(Duration::from_secs(2))
            .expect("bounded")
            .with_run_timeout(Duration::from_secs(3))
            .expect("bounded");
        // 2 + 3 + 5 = 10, which is BELOW the kernel default, so the kernel default governs and the
        // answer must not drop under it.
        assert_eq!(
            provider_with(tight).required_boot_deadline(),
            DEFAULT_PROVIDER_DEADLINE
        );
    }
}
