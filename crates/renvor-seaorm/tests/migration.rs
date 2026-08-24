//! Migrations for the SeaORM adapter: ordering, tamper refusal, policy, bounds, and boot.
//!
//! # Same engine, second wrapper, so the same properties are asserted again
//!
//! `renvor-seaorm` calls `sqlx::migrate::Migrator` itself rather than reusing `renvor-sqlx`'s
//! runner, because a SeaORM application must not resolve a direct-SQLx crate. Two wrappers around
//! one engine means two places the properties can stop holding, so both are tested — this file is
//! not a copy for symmetry, it is the second wrapper's own evidence.

mod support;

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use std::time::Duration;

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use renvor_database::{MigrationPolicy, MigrationSettings};

macro_rules! suite {
    ($module:ident, $feature:literal, $driver:ty, $connect:path, $url:expr, $run:ident) => {
        #[cfg(feature = $feature)]
        mod $module {
            use super::*;
            use renvor_core::health::{Readiness, ReadinessContributor as _};
            use renvor_core::provider::registry::{CapabilityId, ProviderId};
            use renvor_database::{DatabaseErrorKind, MigrationOutcome, PoolSettings};
            use renvor_seaorm::migrate::Migrations;
            use renvor_seaorm::provider::SeaOrmProvider;
            use sqlx::AssertSqlSafe;

            fn boot_settings(policy: MigrationPolicy) -> MigrationSettings {
                MigrationSettings::default()
                    .with_policy(policy)
                    .with_lock_timeout(Duration::from_secs(5))
                    .expect("bounded")
                    .with_run_timeout(Duration::from_secs(20))
                    .expect("bounded")
            }

            /// A database of this suite's own, wiped back to nothing.
            async fn blank() -> Option<(
                renvor_database::ConnectionString,
                tokio::sync::MutexGuard<'static, ()>,
            )> {
                let guard = support::SHARED_FIXTURE.lock().await;
                let dsn = support::isolated_url::<$driver>($url, "renvor_sea_boot_test").await?;
                let database = $connect(&dsn, &support::settings())
                    .await
                    .expect("connects");
                for statement in [
                    "DROP TABLE IF EXISTS rv_boot_marker",
                    "DROP TABLE IF EXISTS should_not_exist",
                    "DROP TABLE IF EXISTS _sqlx_migrations",
                ] {
                    let _ = sqlx::query(AssertSqlSafe(statement.to_owned()))
                        .execute(database.pool())
                        .await;
                }
                let _ = renvor_database::Database::close(&database).await;
                Some((dsn, guard))
            }

            fn provider(
                dsn: &renvor_database::ConnectionString,
                directory: &str,
                policy: MigrationPolicy,
            ) -> Option<SeaOrmProvider<$driver>> {
                let migrations = futures_lite_block(Migrations::load(
                    std::path::Path::new(directory),
                    boot_settings(policy),
                ))
                .expect("the fixture loads");
                Some(
                    SeaOrmProvider::<$driver>::new(
                        ProviderId::new("database"),
                        CapabilityId::new("database"),
                        dsn.clone(),
                        PoolSettings::default(),
                        renvor_database::DatabaseKind::parse(stringify!($module))
                            .expect("a known kind"),
                    )
                    .with_migrations(migrations),
                )
            }

            /// Runs a future to completion on the current runtime without nesting one.
            fn futures_lite_block<T>(future: impl std::future::Future<Output = T>) -> T {
                tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
            }

            async fn marker_rows(dsn: &renvor_database::ConnectionString) -> i64 {
                let database = $connect(dsn, &support::settings()).await.expect("connects");
                let count = sqlx::query_scalar(AssertSqlSafe(
                    "SELECT COUNT(*) FROM rv_boot_marker".to_owned(),
                ))
                .fetch_one(database.pool())
                .await
                .unwrap_or(-1);
                let _ = renvor_database::Database::close(&database).await;
                count
            }

            #[tokio::test(flavor = "multi_thread")]
            async fn on_boot_applies_migrations_before_readiness_is_reported() {
                let Some((dsn, _guard)) = blank().await else {
                    return;
                };
                let provider = provider(&dsn, "tests/migrations-boot", MigrationPolicy::OnBoot)
                    .expect("built");
                assert_eq!(
                    provider.readiness(),
                    Readiness::NotReady,
                    "ready before boot ran"
                );
                support::initialise(&provider).await.expect("boots");
                assert_eq!(provider.readiness(), Readiness::Ready);
                assert_eq!(
                    marker_rows(&dsn).await,
                    1,
                    "readiness was reported against an un-migrated schema"
                );
            }

            #[tokio::test(flavor = "multi_thread")]
            async fn never_applies_nothing_and_still_reaches_ready() {
                let Some((dsn, _guard)) = blank().await else {
                    return;
                };
                let provider =
                    provider(&dsn, "tests/migrations-boot", MigrationPolicy::Never).expect("built");
                support::initialise(&provider).await.expect("boots");
                assert_eq!(provider.readiness(), Readiness::Ready);
                assert_eq!(
                    marker_rows(&dsn).await,
                    -1,
                    "the default policy changed a schema"
                );
            }

            /// A changed already-applied migration is refused BEFORE the new one is applied.
            ///
            /// This is the property `sea-orm-migration` cannot provide: its bookkeeping table has
            /// `version` and `applied_at` and no checksum, so an edited migration is undetectable.
            #[tokio::test(flavor = "multi_thread")]
            async fn a_changed_migration_is_refused_before_the_schema_is_modified() {
                let Some((dsn, _guard)) = blank().await else {
                    return;
                };
                let first = provider(&dsn, "tests/migrations-boot", MigrationPolicy::OnBoot)
                    .expect("built");
                support::initialise(&first).await.expect("boots");

                let second = provider(
                    &dsn,
                    "tests/migrations-boot-changed",
                    MigrationPolicy::OnBoot,
                )
                .expect("built");
                support::initialise(&second)
                    .await
                    .expect_err("a changed migration set must be refused");
                assert_eq!(
                    second.readiness(),
                    Readiness::NotReady,
                    "a refused boot reported ready"
                );

                // The THIRD migration in the changed set creates `should_not_exist`. Refusing
                // after applying it would still fail the boot while having changed the schema.
                let database = $connect(&dsn, &support::settings())
                    .await
                    .expect("connects");
                let applied =
                    sqlx::query(AssertSqlSafe("SELECT 1 FROM should_not_exist".to_owned()))
                        .fetch_optional(database.pool())
                        .await;
                let _ = renvor_database::Database::close(&database).await;
                assert!(
                    applied.is_err(),
                    "the refusal happened AFTER the schema was modified"
                );
            }

            #[tokio::test(flavor = "multi_thread")]
            async fn an_unchanged_set_reaches_ready_a_second_time() {
                let Some((dsn, _guard)) = blank().await else {
                    return;
                };
                for round in 0..2 {
                    let provider = provider(&dsn, "tests/migrations-boot", MigrationPolicy::OnBoot)
                        .expect("built");
                    support::initialise(&provider)
                        .await
                        .unwrap_or_else(|_| panic!("round {round} refused an unchanged set"));
                    assert_eq!(provider.readiness(), Readiness::Ready);
                }
                assert_eq!(marker_rows(&dsn).await, 1, "the seed was applied twice");
            }

            /// Concurrent starters apply the set exactly once.
            #[tokio::test(flavor = "multi_thread")]
            async fn concurrent_starters_apply_exactly_once() {
                let Some((dsn, _guard)) = blank().await else {
                    return;
                };
                let mut handles = Vec::new();
                for _ in 0..4 {
                    let dsn = dsn.clone();
                    handles.push(tokio::spawn(async move {
                        let provider =
                            provider(&dsn, "tests/migrations-boot", MigrationPolicy::OnBoot)
                                .expect("built");
                        support::initialise(&provider).await
                    }));
                }
                let mut booted = 0;
                for handle in handles {
                    if handle.await.expect("the task did not panic").is_ok() {
                        booted += 1;
                    }
                }
                assert!(booted > 0, "every concurrent starter failed");
                assert_eq!(
                    marker_rows(&dsn).await,
                    1,
                    "the seed migration ran more than once under concurrent startup"
                );
            }

            /// Ordering is by version, not by directory enumeration order.
            #[tokio::test(flavor = "multi_thread")]
            async fn versions_are_strictly_increasing() {
                let migrations = Migrations::load(
                    std::path::Path::new("tests/migrations"),
                    MigrationSettings::default(),
                )
                .await
                .expect("loads");
                assert!(migrations.is_ordered(), "the set is not strictly ordered");
                assert_eq!(
                    migrations.versions().len(),
                    2,
                    "a reversible migration was counted twice"
                );
            }

            /// A migration with no `.down.sql` is refused BEFORE anything is locked or changed.
            #[tokio::test(flavor = "multi_thread")]
            async fn an_irreversible_migration_is_refused_before_it_runs() {
                let migrations = Migrations::load(
                    std::path::Path::new("tests/migrations-irreversible"),
                    MigrationSettings::default(),
                )
                .await
                .expect("loads");
                let version = migrations.versions()[0];
                let error = migrations
                    .ensure_reversible(version)
                    .expect_err("an irreversible migration must be refused");
                assert_eq!(error.kind(), DatabaseErrorKind::MigrationIrreversible);
            }

            /// The report names every migration and what happened to it.
            #[tokio::test(flavor = "multi_thread")]
            async fn the_report_distinguishes_applied_from_already_applied() {
                let Some((dsn, _guard)) = blank().await else {
                    return;
                };
                let database = $connect(&dsn, &support::settings())
                    .await
                    .expect("connects");
                let migrations = Migrations::load(
                    std::path::Path::new("tests/migrations-boot"),
                    boot_settings(MigrationPolicy::OnBoot),
                )
                .await
                .expect("loads");

                let first = migrations.$run(&database).await.expect("applies");
                assert_eq!(first.applied(), 2, "the first run applied nothing");
                assert!(first.is_ordered());

                let second = migrations.$run(&database).await.expect("re-runs");
                assert_eq!(
                    second.applied(),
                    0,
                    "the second run applied something again"
                );
                assert!(
                    second
                        .steps()
                        .iter()
                        .all(|step| step.outcome() == MigrationOutcome::AlreadyApplied),
                    "a re-run reported a migration as newly applied"
                );
                let _ = renvor_database::Database::close(&database).await;
            }

            /// A provider that migrates on boot asks for a deadline long enough to honour its own.
            #[tokio::test(flavor = "multi_thread")]
            async fn the_required_boot_deadline_covers_both_migration_bounds() {
                let Some((dsn, _guard)) = blank().await else {
                    return;
                };
                let migrating = provider(&dsn, "tests/migrations-boot", MigrationPolicy::OnBoot)
                    .expect("built");
                let idle =
                    provider(&dsn, "tests/migrations-boot", MigrationPolicy::Never).expect("built");

                assert!(
                    migrating.required_boot_deadline()
                        >= Duration::from_secs(5) + Duration::from_secs(20),
                    "the deadline is shorter than the bounds it must cover"
                );
                assert_eq!(
                    idle.required_boot_deadline(),
                    renvor_core::lifecycle::application::DEFAULT_PROVIDER_DEADLINE,
                    "a provider that migrates nothing asked for extra time"
                );
            }

            /// A failed boot publishes no database and names no credential.
            #[tokio::test(flavor = "multi_thread")]
            async fn a_failed_boot_publishes_nothing_and_leaks_no_credential() {
                let Some((dsn, _guard)) = blank().await else {
                    return;
                };
                let first = provider(&dsn, "tests/migrations-boot", MigrationPolicy::OnBoot)
                    .expect("built");
                support::initialise(&first).await.expect("boots");

                let second = provider(
                    &dsn,
                    "tests/migrations-boot-changed",
                    MigrationPolicy::OnBoot,
                )
                .expect("built");
                let error = support::initialise(&second).await.expect_err("refused");

                assert!(
                    second.database().is_none(),
                    "a failed boot published a database"
                );

                // The operator's REAL password, taken from the DSN this test is using.
                //
                // # An empty extraction is a FAILURE, not a skip
                //
                // This ended in `.unwrap_or_default()` guarded by `if !secret.is_empty()`, so a
                // DSN with no password — trust auth, a `?password=` query form, a socket URL —
                // turned the whole assertion into a no-op while the test still reported `ok`.
                // That is exactly the silent-skip failure mode `support/mod.rs` argues against at
                // length for the database URL, and the guard had not been applied here. A
                // security review found it.
                //
                // The suite only reaches this line when `RENVOR_TEST_REQUIRE_DATABASE` is set or a
                // URL was supplied, so a DSN this cannot read is a misconfiguration worth failing.
                let exposed = dsn.expose();
                let secret = exposed
                    .split_once("://")
                    .and_then(|(_, rest)| rest.split_once('@'))
                    .and_then(|(credentials, _)| credentials.split_once(':'))
                    .map(|(_, password)| password.to_owned())
                    .filter(|password| !password.is_empty())
                    .unwrap_or_else(|| {
                        panic!(
                            "no password could be extracted from the configured DSN, so the \
                             redaction assertion below would check nothing. Point the test at a \
                             password-authenticated instance rather than letting this pass"
                        )
                    });

                // AND a canary that is unmistakable, so the assertion does not depend on the
                // operator's password happening to be an unusual string. `CREDENTIAL_CANARY` was
                // defined, documented as "a recognisable value that must never appear in output",
                // and referenced by no test in this crate — `#![allow(dead_code)]` suppressed the
                // warning that would have said so.
                //
                // The canary is put somewhere it could leak from: a DSN whose password IS the
                // canary, connected with, and the resulting failure rendered. A version of this
                // that merely asserted a freshly-built `DatabaseError` lacks a string nothing ever
                // put in it would have proved nothing at all.
                let poisoned = renvor_database::ConnectionString::new(
                    exposed.replace(&secret, support::CREDENTIAL_CANARY),
                );
                let refusal = $connect(&poisoned, &support::settings())
                    .await
                    .err()
                    .expect("a wrong password must be refused");
                // Same promise as `renvor-sqlx`, asserted on the same terms: the two adapters
                // document one contract and must not disagree about which kind a wrong password is.
                assert_eq!(
                    refusal.kind(),
                    renvor_database::DatabaseErrorKind::ConnectFailed,
                    "a refused handshake must be reported as a failure to connect"
                );
                let canary_rendered = format!("{refusal:?} {refusal}");
                assert!(
                    !canary_rendered.contains(support::CREDENTIAL_CANARY),
                    "the canary reached a rendered connection error"
                );

                // Every `Debug` this crate defines, not just the provider's. The originals
                // rendered `SeaOrmProvider` alone, leaving `SeaOrmDatabase`, `SeaOrmUnitOfWork`,
                // `SeaOrmConnection` and `Migrations` unexercised by any test.
                let database = $connect(&dsn, &support::settings()).await.expect("connects");
                let unit = renvor_database::Database::begin(&database).await.expect("begins");
                let migrations_debug = Migrations::load(
                    std::path::Path::new("tests/migrations-boot"),
                    boot_settings(MigrationPolicy::Never),
                )
                .await
                .expect("loads");
                let rendered = format!(
                    "{error:?} {error} {second:?} {database:?} {unit:?} {migrations_debug:?}"
                );
                let _ = renvor_database::UnitOfWork::rollback(unit).await;
                let _ = renvor_database::Database::close(&database).await;

                assert!(
                    !rendered.contains(&secret),
                    "the password reached a diagnostic"
                );
                // Reported by INDEX rather than by naming the needle and printing the rendering:
                // this file plants a password, and a diagnostic that prints what it asserts about
                // prints it on the one run where the redaction was wrong.
                for (needle_index, forbidden) in
                    ["CREATE TABLE", "INSERT INTO", "tests/migrations"].into_iter().enumerate()
                {
                    assert!(
                        !rendered.contains(forbidden),
                        "forbidden needle {needle_index} reached a diagnostic"
                    );
                }
            }
        }
    };
}

suite!(
    postgres,
    "db-postgres",
    sqlx::Postgres,
    renvor_seaorm::connect_postgres,
    support::POSTGRES_URL,
    run_postgres
);

suite!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    renvor_seaorm::connect_mysql,
    support::MYSQL_URL,
    run_mysql
);
