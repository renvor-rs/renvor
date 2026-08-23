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
                for secret in ["127.0.0.1", ":p@", "none"] {
                    assert!(
                        !rendered.contains(secret),
                        "the boot failure leaked `{secret}`: {rendered}"
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

            async fn migrations(policy: renvor_database::MigrationPolicy) -> renvor_sqlx::migrate::Migrations {
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
                let provider = provider(ConnectionString::new("unused".to_owned()))
                    .with_migrations(loaded);
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

            /// A provider's `Debug` must not carry a connection string.
            #[tokio::test]
            async fn the_provider_debug_carries_no_credential() {
                let provider = provider(ConnectionString::new(format!(
                    "postgres://user:{}@db.internal:5432/app",
                    support::CREDENTIAL_CANARY
                )));
                let rendered = format!("{provider:?}");
                assert!(!rendered.contains(support::CREDENTIAL_CANARY));
                assert!(!rendered.contains("db.internal"));
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
