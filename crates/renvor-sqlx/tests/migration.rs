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
    (
        $module:ident,
        $feature:literal,
        $driver:ty,
        $connect:path,
        $run:ident,
        $url:expr,
        $partial_exists:literal,
        $dirty_rows:literal
    ) => {
        #[cfg(feature = $feature)]
        mod $module {
            use std::path::{Path, PathBuf};
            use std::time::Duration;

            use renvor_database::{
                Database, DatabaseErrorKind, DatabaseKind, MigrationOutcome, MigrationPolicy,
                MigrationSettings, Reversibility,
            };
            use renvor_sqlx::Migrations;
            use sqlx::AssertSqlSafe;

            use crate::support;

            fn set(name: &str) -> PathBuf {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests")
                    .join(name)
            }

            /// A database with no Renvor tables and no migration bookkeeping, plus the guard
            /// that keeps it that way.
            ///
            /// Every test here drops `_sqlx_migrations`, and several take the migration lock. Two
            /// running at once tear down each other's bookkeeping and block on each other's lock,
            /// so the suite passed only under `--test-threads=1` — a requirement nothing stated.
            /// See [`support::SHARED_FIXTURE`].
            async fn blank() -> Option<(
                renvor_sqlx::SqlxDatabase<$driver>,
                tokio::sync::MutexGuard<'static, ()>,
            )> {
                let guard = support::SHARED_FIXTURE.lock().await;
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
                Some((database, guard))
            }

            // ------------------------------------------------------------------ ordering

            #[tokio::test]
            async fn versions_are_strictly_increasing_and_independent_of_directory_order() {
                let migrations = Migrations::load(&set("migrations"), MigrationSettings::default())
                    .await
                    .expect("loads");
                let versions = migrations.versions();
                assert_eq!(versions, vec![20_260_101_000_001, 20_260_101_000_002]);
                assert!(migrations.is_ordered());
            }

            #[tokio::test]
            async fn a_run_reports_which_migrations_it_applied() {
                let Some((database, _fixture)) = blank().await else {
                    return;
                };
                let migrations = Migrations::load(&set("migrations"), MigrationSettings::default())
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

            // -------------------------------------------------- partial failure, dirty ledger

            /// A migration set whose single migration fails **after** an earlier statement.
            ///
            /// Two `CREATE TABLE`s for the same name in one file: the first succeeds, the second
            /// is refused because the object now exists. Whether the first survives is the
            /// engine's decision, and it is the decision the whole recovery procedure turns on.
            ///
            /// Built in a temporary directory rather than committed, for the reason the checksum
            /// test gives: `xtask` step 11 treats a dirty tree as a failure, and a migration set
            /// that is *designed* to fail has no business in the repository's own fixtures.
            fn partial_set() -> PathBuf {
                let directory = std::env::temp_dir().join(format!(
                    "renvor-partial-{}-{}",
                    stringify!($module),
                    std::process::id()
                ));
                let _ = std::fs::remove_dir_all(&directory);
                std::fs::create_dir_all(&directory).expect("creates");
                std::fs::write(
                    directory.join("20260101000001_partial.up.sql"),
                    "CREATE TABLE rv_partial (id BIGINT PRIMARY KEY);\n\
                     CREATE TABLE rv_partial (id BIGINT PRIMARY KEY);\n",
                )
                .expect("writes");
                std::fs::write(
                    directory.join("20260101000001_partial.down.sql"),
                    "DROP TABLE IF EXISTS rv_partial;\n",
                )
                .expect("writes");
                directory
            }

            /// After a partial failure the next run is **refused**, not resumed.
            ///
            /// # This test exists because the portability contract said the opposite
            ///
            /// `contracts/database-portability.md` used to instruct operators that after a
            /// partial MySQL failure *"the recovery path is 'run the rest', not 'run it again
            /// from the start'"*. A review found that false, and this is the measurement that
            /// settles it: SQLx writes its ledger row with `success = FALSE` **before** running
            /// the migration, MySQL's implicit commit makes that row permanent the moment the
            /// first DDL statement executes, and every later run sees a dirty version and returns
            /// [`DatabaseErrorKind::MigrationDirty`] without executing anything at all. There is
            /// no "rest" to run.
            ///
            /// # Both engines are asserted, because the contrast is the contract
            ///
            /// PostgreSQL wraps the ledger row and the migration in one transaction, so a failure
            /// rolls back both: no table, no dirty row, and the next run genuinely retries from
            /// the start. Asserting only MySQL would leave the PostgreSQL half of the contract
            /// resting on the same reasoning that produced the wrong MySQL half.
            #[tokio::test]
            async fn a_partial_migration_is_refused_on_the_next_run_rather_than_resumed() {
                let Some((database, _fixture)) = blank().await else {
                    return;
                };
                let clean = |sql: &'static str| {
                    let pool = database.pool().clone();
                    async move {
                        sqlx::query(AssertSqlSafe(sql.to_owned()))
                            .execute(&pool)
                            .await
                            .expect("cleans");
                    }
                };
                clean("DROP TABLE IF EXISTS rv_partial").await;

                let directory = partial_set();
                let migrations = Migrations::load(&directory, MigrationSettings::default())
                    .await
                    .expect("loads");

                let failure = migrations
                    .$run(&database)
                    .await
                    .expect_err("the second CREATE TABLE must be refused");
                assert_eq!(
                    failure.kind(),
                    DatabaseErrorKind::MigrationFailed,
                    "the FIRST run fails on the statement, not on the ledger"
                );

                // Did the statement BEFORE the failure survive its transaction?
                let survived: i64 = sqlx::query_scalar(AssertSqlSafe($partial_exists.to_owned()))
                    .fetch_one(database.pool())
                    .await
                    .expect("reads the catalogue");
                // And did SQLx leave a version marked unsuccessful behind?
                let dirty: i64 = sqlx::query_scalar(AssertSqlSafe($dirty_rows.to_owned()))
                    .fetch_one(database.pool())
                    .await
                    .expect("reads the ledger");

                let (expected_survived, expected_dirty, expected_second) =
                    match database.kind() {
                        // Transactional DDL: the ledger row and the table go back together.
                        DatabaseKind::Postgres => (0, 0, DatabaseErrorKind::MigrationFailed),
                        // An implicit commit made both permanent before the failure was raised.
                        DatabaseKind::MySql => (1, 1, DatabaseErrorKind::MigrationDirty),
                        kind => panic!(
                            "{kind:?} has never been measured against a partial migration. Record \
                             what it does with the ledger before this contract claims to cover it"
                        ),
                    };

                assert_eq!(
                    survived,
                    expected_survived,
                    "on {:?} the statement before the failure {} committed. That is what decides \
                     whether an operator has a half-migrated schema to repair",
                    database.kind(),
                    if survived == 1 { "HAD" } else { "had not" }
                );
                assert_eq!(
                    dirty,
                    expected_dirty,
                    "on {:?} the ledger held {dirty} unsuccessful row(s) after the failure",
                    database.kind()
                );

                // THE FINDING: what the NEXT run does.
                let second = migrations
                    .$run(&database)
                    .await
                    .expect_err("a second run must not succeed either");
                assert_eq!(
                    second.kind(),
                    expected_second,
                    "on {:?} the run AFTER a partial failure reported {:?}. On MySQL it must be \
                     `MigrationDirty` — refused before any statement is sent, so no operator can \
                     recover by re-running — and on PostgreSQL it must be the original \
                     `MigrationFailed`, because there is nothing partial to be blocked by",
                    database.kind(),
                    second.kind()
                );

                clean("DROP TABLE IF EXISTS rv_partial").await;
                clean("DROP TABLE IF EXISTS _sqlx_migrations").await;
                let _ = std::fs::remove_dir_all(&directory);
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
                let Some((database, _fixture)) = blank().await else {
                    return;
                };

                let original = Migrations::load(&set("migrations"), MigrationSettings::default())
                    .await
                    .expect("loads");
                original.$run(&database).await.expect("applies");

                // Build a mutated copy of the SAME versions.
                let temporary = std::env::temp_dir().join(format!(
                    "renvor-mig-{}-{}",
                    stringify!($module),
                    std::process::id()
                ));
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

                // Start from a blank database, and KEEP THE GUARD for the whole test.
                //
                // It was bound inside this `block_on` and therefore dropped the moment the block
                // ended, leaving everything below unguarded. Another test's `blank()` then dropped
                // `_sqlx_migrations` while these starters were racing, and the count came back 0 —
                // reported as "a migration was applied more than once", which is the opposite of
                // what had happened. A guard whose scope is smaller than the thing it guards is
                // worse than none: it looks like protection in the diff.
                let Some(_fixture) = runtime.block_on(async {
                    let (database, fixture) = blank().await?;
                    database.close().await.expect("closes");
                    Some(fixture)
                }) else {
                    return;
                };

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
                let migrations = Migrations::load(&set("migrations"), MigrationSettings::default())
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
                let migrations = Migrations::load(&set("migrations"), MigrationSettings::default())
                    .await
                    .expect("loads");
                assert_eq!(migrations.settings().policy(), MigrationPolicy::Never);
                assert!(!migrations.settings().policy().runs_on_boot());
            }

            #[tokio::test]
            async fn the_run_deadline_is_bounded_and_enforced() {
                let Some((database, _fixture)) = blank().await else {
                    return;
                };
                // A MIGRATION THAT REALLY TAKES TEN SECONDS, against a two-second deadline.
                //
                // This used `Duration::from_nanos(1)` against the ordinary set, on the theory that
                // no real migration finishes in a nanosecond. That stopped being sound when the
                // lock moved OUT of the timed region: the timed region is now the migrations
                // alone, `tokio::time::timeout` polls the inner future before it checks the
                // deadline, and two tiny statements on a warm connection can finish inside the
                // timer's own granularity. The test then observed a SUCCESSFUL run and failed on
                // the recovery count — a real race, reported as an unrelated assertion.
                //
                // A server-side sleep removes the race instead of widening the margin around it.
                let slow = format!("migrations-boot-slow-{}", stringify!($module));
                let settings = MigrationSettings::default()
                    .with_run_timeout(Duration::from_secs(2))
                    .expect("bounded");
                let migrations = Migrations::load(&set(&slow), settings)
                    .await
                    .expect("loads");
                let error = migrations
                    .$run(&database)
                    .await
                    .expect_err("the deadline must be enforced");
                // `DeadlineExceeded`, NOT `MigrationLockTimeout`, and the distinction is the point.
                //
                // This asserted `MigrationLockTimeout` until the two deadlines were separated,
                // because one `tokio::time::timeout` wrapped the whole run and every elapsed
                // deadline was reported as a lock problem. The two mean opposite things to whoever
                // is on call: a lock timeout says *another process is migrating* — wait, or go and
                // look at what it is doing; a run timeout says *your migration is too slow* — go
                // and look at the migration. Sending an operator to the wrong one is worse than
                // saying nothing.
                //
                // The lock deadline has its own coverage in `tests/migrate_on_boot.rs`, where a
                // competing session really holds the lock.
                assert_eq!(error.kind(), DatabaseErrorKind::DeadlineExceeded);
                assert!(error.kind().is_transient());

                // THE LOCK GUARD: after a timed-out run, a fresh run must not hang. If the lock
                // had been leaked with the connection, this would block until the test harness
                // gave up.
                let recovered = Migrations::load(&set("migrations"), MigrationSettings::default())
                    .await
                    .expect("loads");
                let report =
                    tokio::time::timeout(Duration::from_secs(30), recovered.$run(&database))
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
    support::POSTGRES_URL,
    "SELECT (CASE WHEN to_regclass('rv_partial') IS NULL THEN 0 ELSE 1 END)::bigint",
    "SELECT COUNT(*)::bigint FROM _sqlx_migrations WHERE success = FALSE"
);
migration_suite!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    renvor_sqlx::connect_mysql,
    run_mysql,
    support::MYSQL_URL,
    "SELECT CAST(COUNT(*) AS SIGNED) FROM information_schema.tables \
     WHERE table_schema = DATABASE() AND table_name = 'rv_partial'",
    "SELECT CAST(COUNT(*) AS SIGNED) FROM _sqlx_migrations WHERE success = FALSE"
);
