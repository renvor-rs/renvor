//! Migration ordering, checksums, concurrency, bounds, and reversibility — against real databases.
//!
//! # Why these run against a real server rather than a fake
//!
//! Every property here is a property of the **database**, not of Renvor's code: an advisory lock is
//! held by a session, a checksum is compared against a row, and a concurrent start is two real
//! connections racing. Constitution principle IX requires the test to *"exercise the boundary where
//! the risk exists"*, and none of these risks exist in a mock.

mod support;

macro_rules! migration_suite {
    ($module:ident, $feature:literal, $driver:ty, $connect:path, $run:ident, $url:expr) => {
        #[cfg(feature = $feature)]
        mod $module {
            use std::path::{Path, PathBuf};
            use std::time::Duration;

            use renvor_database::{
                Database, DatabaseErrorKind, MigrationOutcome, MigrationPolicy, MigrationSettings,
                Reversibility,
            };
            use renvor_sqlx::Migrations;
            use sqlx::AssertSqlSafe;

            use crate::support;

            fn set(name: &str) -> PathBuf {
                Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(name)
            }

            /// A database with no Renvor tables and no migration bookkeeping.
            async fn blank() -> Option<renvor_sqlx::SqlxDatabase<$driver>> {
                let dsn = support::url($url)?;
                let database = $connect(&dsn, &support::settings())
                    .await
                    .expect("connects");
                for statement in [
                    "DROP TABLE IF EXISTS rv_widget",
                    "DROP TABLE IF EXISTS rv_ledger",
                    "DROP TABLE IF EXISTS _sqlx_migrations",
                ] {
                    sqlx::query(AssertSqlSafe(statement.to_owned()))
                        .execute(database.pool())
                        .await
                        .expect("cleans");
                }
                Some(database)
            }

            // ------------------------------------------------------------------ ordering

            #[tokio::test]
            async fn versions_are_strictly_increasing_and_independent_of_directory_order() {
                let migrations =
                    Migrations::load(&set("migrations"), MigrationSettings::default())
                        .await
                        .expect("loads");
                let versions = migrations.versions();
                assert_eq!(versions, vec![20_260_101_000_001, 20_260_101_000_002]);
                assert!(migrations.is_ordered());
            }

            #[tokio::test]
            async fn a_run_reports_which_migrations_it_applied() {
                let Some(database) = blank().await else {
                    return;
                };
                let migrations =
                    Migrations::load(&set("migrations"), MigrationSettings::default())
                        .await
                        .expect("loads");

                let first = migrations.$run(&database).await.expect("applies");
                assert_eq!(first.applied(), 2, "both migrations should be new");
                assert!(first.is_ordered());
                assert!(
                    first
                        .steps()
                        .iter()
                        .all(|s| s.outcome() == MigrationOutcome::Applied)
                );

                // A SECOND RUN APPLIES NOTHING. Idempotence is a property, not a hope.
                let second = migrations.$run(&database).await.expect("re-runs");
                assert_eq!(second.applied(), 0);
                assert!(
                    second
                        .steps()
                        .iter()
                        .all(|s| s.outcome() == MigrationOutcome::AlreadyApplied)
                );

                database.close().await.expect("closes");
            }

            // ------------------------------------------------------------------ checksums

            /// A migration whose content changed after being applied fails startup, and the schema
            /// is unmodified.
            ///
            /// The mutated copy is built in a temporary directory rather than by editing the
            /// committed set, so the test cannot leave the repository dirty — `xtask` step 11
            /// treats a dirty tree as a failure.
            #[tokio::test]
            async fn a_changed_applied_migration_fails_closed() {
                let Some(database) = blank().await else {
                    return;
                };

                let original = Migrations::load(&set("migrations"), MigrationSettings::default())
                    .await
                    .expect("loads");
                original.$run(&database).await.expect("applies");

                // Build a mutated copy of the SAME versions.
                let temporary = std::env::temp_dir()
                    .join(format!("renvor-mig-{}-{}", stringify!($module), std::process::id()));
                let _ = std::fs::remove_dir_all(&temporary);
                std::fs::create_dir_all(&temporary).expect("creates");
                for entry in std::fs::read_dir(set("migrations")).expect("reads") {
                    let entry = entry.expect("entry");
                    let name = entry.file_name();
                    let mut body = std::fs::read_to_string(entry.path()).expect("reads");
                    if name.to_string_lossy().contains("create_widget.up") {
                        body.push_str("\n-- content changed after it was applied\n");
                    }
                    std::fs::write(temporary.join(name), body).expect("writes");
                }

                let mutated = Migrations::load(&temporary, MigrationSettings::default())
                    .await
                    .expect("loads");
                let error = mutated
                    .$run(&database)
                    .await
                    .expect_err("a changed applied migration must not be accepted");
                assert_eq!(error.kind(), DatabaseErrorKind::MigrationChecksumMismatch);

                // THE SCHEMA IS UNMODIFIED: the original two migrations, and nothing more.
                let applied: i64 = sqlx::query_scalar(AssertSqlSafe(
                    "SELECT COUNT(*) FROM _sqlx_migrations".to_owned(),
                ))
                .fetch_one(database.pool())
                .await
                .expect("counts");
                assert_eq!(applied, 2);

                let _ = std::fs::remove_dir_all(&temporary);
                database.close().await.expect("closes");
            }

            // ---------------------------------------------------------------- concurrency

            /// Two concurrent starters apply each migration exactly once.
            ///
            /// This is the property `pg_advisory_lock` and `GET_LOCK` exist to provide, and it is
            /// asserted against a real server because a lock that is not taken against a real
            /// server is not a lock.
            ///
            /// # Real threads, each with its own runtime
            ///
            /// `tokio::spawn` cannot be used here: the ports use `async fn` in traits, and the
            /// resulting higher-ranked lifetimes are not provably `Send` across a spawn. Separate
            /// OS threads with separate runtimes sidestep that **and** model concurrent startup
            /// more faithfully — the real hazard is two processes booting at once, not two tasks.
            #[test]
            fn concurrent_startup_applies_each_migration_exactly_once() {
                let runtime = tokio::runtime::Runtime::new().expect("runtime");
                let Some(dsn_text) = std::env::var($url).ok().filter(|v| !v.is_empty()) else {
                    println!("SKIPPED: set {} to run this test", $url);
                    return;
                };

                // Start from a blank database.
                runtime.block_on(async {
                    let Some(database) = blank().await else {
                        return;
                    };
                    database.close().await.expect("closes");
                });

                let starters = 4;
                let mut handles = Vec::new();
                for _ in 0..starters {
                    let dsn_text = dsn_text.clone();
                    handles.push(std::thread::spawn(move || {
                        let runtime = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .expect("runtime");
                        runtime.block_on(async move {
                            let dsn = renvor_database::ConnectionString::new(dsn_text);
                            let database = $connect(&dsn, &support::settings())
                                .await
                                .expect("connects");
                            let migrations = Migrations::load(
                                &Path::new(env!("CARGO_MANIFEST_DIR"))
                                    .join("tests")
                                    .join("migrations"),
                                MigrationSettings::default(),
                            )
                            .await
                            .expect("loads");
                            let applied = migrations
                                .$run(&database)
                                .await
                                .expect("migrates")
                                .applied();
                            let _ = database.close().await;
                            applied
                        })
                    }));
                }

                // Every starter must SUCCEED. A starter that failed would mean the lock did not
                // serialise them, which is the failure this test exists to catch.
                for handle in handles {
                    handle.join().expect("a concurrent starter panicked");
                }

                // THE PROPERTY IS ABOUT THE DATABASE, NOT ABOUT THE REPORTS. Each starter reads
                // the applied set before racing for the lock, so under concurrency several will
                // truthfully report the same migration as "not applied when I began" — see
                // `MigrationOutcome`. What must hold is that the bookkeeping table has exactly one
                // row per migration.

                runtime.block_on(async {
                    let dsn = renvor_database::ConnectionString::new(dsn_text);
                    let database = $connect(&dsn, &support::settings())
                        .await
                        .expect("connects");
                    let rows: i64 = sqlx::query_scalar(AssertSqlSafe(
                        "SELECT COUNT(*) FROM _sqlx_migrations".to_owned(),
                    ))
                    .fetch_one(database.pool())
                    .await
                    .expect("counts");
                    assert_eq!(
                        rows, 2,
                        "a migration was applied more than once under concurrent startup"
                    );

                    // And the schema itself reflects exactly one application: the column added by
                    // the second migration exists once, which a double-apply would have failed on.
                    let widgets: i64 = sqlx::query_scalar(AssertSqlSafe(
                        "SELECT COUNT(*) FROM rv_widget".to_owned(),
                    ))
                    .fetch_one(database.pool())
                    .await
                    .expect("the migrated table exists");
                    assert_eq!(widgets, 0);

                    database.close().await.expect("closes");
                });
            }

            // -------------------------------------------------------------- reversibility

            #[tokio::test]
            async fn an_irreversible_migration_is_refused_before_anything_is_locked() {
                let migrations = Migrations::load(
                    &set("migrations-irreversible"),
                    MigrationSettings::default(),
                )
                .await
                .expect("loads");

                assert_eq!(
                    migrations.reversibility_of(20_260_101_000_001),
                    Some(Reversibility::Irreversible)
                );
                let error = migrations
                    .ensure_reversible(20_260_101_000_001)
                    .expect_err("refused");
                assert_eq!(error.kind(), DatabaseErrorKind::MigrationIrreversible);
            }

            #[tokio::test]
            async fn a_reversible_migration_is_permitted() {
                let migrations =
                    Migrations::load(&set("migrations"), MigrationSettings::default())
                        .await
                        .expect("loads");
                assert_eq!(
                    migrations.reversibility_of(20_260_101_000_001),
                    Some(Reversibility::Reversible)
                );
                migrations
                    .ensure_reversible(20_260_101_000_001)
                    .expect("permitted");
            }

            // --------------------------------------------------------------------- policy

            #[tokio::test]
            async fn automatic_migration_is_not_the_default() {
                let migrations =
                    Migrations::load(&set("migrations"), MigrationSettings::default())
                        .await
                        .expect("loads");
                assert_eq!(migrations.settings().policy(), MigrationPolicy::Never);
                assert!(!migrations.settings().policy().runs_on_boot());
            }

            #[tokio::test]
            async fn the_run_deadline_is_bounded_and_enforced() {
                let Some(database) = blank().await else {
                    return;
                };
                // A deadline too short for any real migration proves the bound is enforced rather
                // than merely configured.
                let settings = MigrationSettings::default()
                    .with_run_timeout(Duration::from_nanos(1))
                    .expect("bounded");
                let migrations = Migrations::load(&set("migrations"), settings)
                    .await
                    .expect("loads");
                let error = migrations
                    .$run(&database)
                    .await
                    .expect_err("the deadline must be enforced");
                assert_eq!(error.kind(), DatabaseErrorKind::MigrationLockTimeout);
                assert!(error.kind().is_transient());

                // THE LOCK GUARD: after a timed-out run, a fresh run must not hang. If the lock
                // had been leaked with the connection, this would block until the test harness
                // gave up.
                let recovered = Migrations::load(&set("migrations"), MigrationSettings::default())
                    .await
                    .expect("loads");
                let report = tokio::time::timeout(
                    Duration::from_secs(30),
                    recovered.$run(&database),
                )
                .await
                .expect("a leaked migration lock would hang here")
                .expect("applies");
                assert_eq!(report.applied(), 2);

                database.close().await.expect("closes");
            }
        }
    };
}

migration_suite!(
    postgres,
    "db-postgres",
    sqlx::Postgres,
    renvor_sqlx::connect_postgres,
    run_postgres,
    support::POSTGRES_URL
);
migration_suite!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    renvor_sqlx::connect_mysql,
    run_mysql,
    support::MYSQL_URL
);
