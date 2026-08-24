//! `MigrationPolicy::OnBoot` applies migrations before readiness, or the application does not boot.
//!
//! FR-021. Phase 006 originally shipped this as a *recorded* decision that nothing performed — the
//! provider answered `migrates_on_boot() == true` and then migrated nothing. That is the worst of
//! the three possible behaviours: an operator reads a `true`, deploys, and gets a running process
//! against an un-migrated schema. These tests exist so that answer cannot drift back.
//!
//! # Why every one of these runs against a real server
//!
//! Every property asserted here belongs to the **database**, not to Renvor's code. A migration lock
//! is held by a session and dies with it. A checksum is compared against a stored row. Two
//! concurrent starters are two real connections racing for one advisory lock. Constitution
//! principle IX requires the test to exercise the boundary where the risk lives, and none of these
//! risks exist against a fake.
//!
//! A missing database **skips** with a printed reason rather than passing quietly; `PLAN.md` §17
//! forbids reporting a skipped test as a gate.

mod support;

macro_rules! boot_suite {
    ($module:ident, $feature:literal, $driver:ty, $connect:path, $run:ident, $url:expr, $kind:expr, $slow:literal) => {
        #[cfg(feature = $feature)]
        mod $module {
            use std::path::{Path, PathBuf};
            use std::time::{Duration, Instant};

            use renvor_core::health::{Readiness, ReadinessContributor};
            use renvor_core::provider::registry::{CapabilityId, Provider, ProviderId};
            use renvor_database::{
                ConnectionString, Database, DatabaseErrorKind, MigrationPolicy, MigrationSettings,
            };
            use renvor_sqlx::provider::SqlxProvider;
            use renvor_sqlx::{Migrations, SqlxDatabase};
            use sqlx::AssertSqlSafe;

            use crate::support;

            fn set(name: &str) -> PathBuf {
                Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(name)
            }

            fn on_boot() -> MigrationSettings {
                MigrationSettings::default().with_policy(MigrationPolicy::OnBoot)
            }

            async fn migrations(name: &str, settings: MigrationSettings) -> Migrations {
                Migrations::load(&set(name), settings).await.expect("loads")
            }

            /// A database with none of this suite's tables and no migration bookkeeping, plus
            /// the guard that keeps it that way.
            ///
            /// Returns the DSN as well, because most tests need a *second*, independent handle:
            /// the whole point of several of them is that the provider's own pool is gone.
            ///
            /// # The guard is returned, not taken and dropped
            ///
            /// Every test here drops `_sqlx_migrations` to start from nothing, and one of them
            /// deliberately HOLDS the migration lock. Two running at once would tear down each
            /// other's bookkeeping and block on each other's lock, so the suite passed only under
            /// `--test-threads=1` — a requirement nothing in the code stated. Holding this for the
            /// test's duration makes it structural. See [`support::SHARED_FIXTURE`].
            ///
            /// It does not serialise the *starters* inside
            /// `concurrent_starters_apply_each_migration_exactly_once`; those are spawned tasks
            /// that never call this, which is what keeps that test a real race.
            async fn blank() -> Option<(
                ConnectionString,
                SqlxDatabase<$driver>,
                tokio::sync::MutexGuard<'static, ()>,
            )> {
                let guard = support::SHARED_FIXTURE.lock().await;
                // ITS OWN DATABASE. See `support::isolated_url`: this suite drops
                // `_sqlx_migrations`, `tests/migration.rs` uses it, and cargo runs test binaries in
                // parallel.
                let dsn = support::isolated_url::<$driver>($url, "renvor_boot_test").await?;
                let database = $connect(&dsn, &support::settings()).await.expect("connects");
                for statement in [
                    "DROP TABLE IF EXISTS rv_boot_marker",
                    "DROP TABLE IF EXISTS rv_should_not_exist",
                    "DROP TABLE IF EXISTS _sqlx_migrations",
                ] {
                    sqlx::query(AssertSqlSafe(statement.to_owned()))
                        .execute(database.pool())
                        .await
                        .expect("cleans");
                }
                Some((dsn, database, guard))
            }

            fn new_provider(dsn: &ConnectionString) -> SqlxProvider<$driver> {
                SqlxProvider::new(
                    ProviderId::new("database"),
                    CapabilityId::new("database"),
                    dsn.clone(),
                    support::settings(),
                    $kind,
                )
            }

            /// THE POSITIVE CONTROL.
            ///
            /// Reads the row migration `…0002` inserts, through a connection that is **not** the
            /// provider's. Without this, every assertion below could be satisfied by a provider
            /// that ran nothing and reported success — the failure mode this whole file exists to
            /// prevent. A test that only asserts "boot returned `Ok`" proves nothing about whether
            /// a migration executed.
            async fn marker(database: &SqlxDatabase<$driver>) -> Option<String> {
                sqlx::query_scalar(AssertSqlSafe(
                    "SELECT mark FROM rv_boot_marker".to_owned(),
                ))
                .fetch_optional(database.pool())
                .await
                .unwrap_or(None)
            }

            /// Whether a table exists, asked through the connection rather than through a catalogue.
            ///
            /// # `information_schema` was the obvious way and it was wrong
            ///
            /// On MySQL, `information_schema.tables` spans **every database on the server**, so a
            /// leftover `rv_boot_marker` in an unrelated database answered "yes" for a database
            /// that did not have one — and the test reported that the default policy had changed a
            /// schema it had never touched. PostgreSQL scopes the same view to the current database
            /// and gave the right answer, so the defect showed on one engine only.
            ///
            /// Selecting from the table itself has no such ambiguity: it is resolved against the
            /// connection's own database by the server, on both engines. An error means the table
            /// is not reachable from here, which is exactly the question being asked.
            async fn table_exists(database: &SqlxDatabase<$driver>, name: &str) -> bool {
                sqlx::query(AssertSqlSafe(format!("SELECT 1 FROM {name} LIMIT 1")))
                    .fetch_optional(database.pool())
                    .await
                    .is_ok()
            }

            // ------------------------------------------------------------- 1. the main property

            /// FR-021: `OnBoot` migrates, and it does so **before** readiness is reported.
            ///
            /// The ordering half is what makes this more than "migrations work". Readiness is
            /// sampled at three points: before boot, after boot, and — the one that matters — the
            /// marker row is read after boot returns, so a provider that reported ready and
            /// migrated afterwards would still be caught by the readiness assertion made against a
            /// database whose marker is already present.
            #[tokio::test]
            async fn on_boot_applies_migrations_before_readiness_is_reported() {
                let Some((dsn, observer, _fixture)) = blank().await else {
                    return;
                };
                assert_eq!(marker(&observer).await, None, "the fixture is not blank");

                let provider =
                    new_provider(&dsn).with_migrations(migrations("migrations-boot", on_boot()).await);
                assert!(provider.migrates_on_boot());
                assert_eq!(
                    provider.readiness(),
                    Readiness::NotReady,
                    "a provider that has not booted must not be ready"
                );

                support::initialise(&provider).await.expect("boots");

                assert_eq!(
                    provider.readiness(),
                    Readiness::Ready,
                    "a migrated boot must report ready"
                );
                assert_eq!(
                    marker(&observer).await.as_deref(),
                    Some("migrated-on-boot"),
                    "READINESS WAS REPORTED WITHOUT THE MIGRATION HAVING RUN"
                );

                provider.stop().await.expect("stops");
                observer.close().await.expect("closes");
            }

            // ------------------------------------------------------------- 2. the default

            /// `Never` is the default, and it applies nothing.
            ///
            /// Asserted against a provider that *has* migrations: the interesting failure is not
            /// "no migrations, none ran" but "migrations supplied, policy left alone, ran anyway".
            #[tokio::test]
            async fn never_applies_no_migration_even_when_migrations_are_supplied() {
                let Some((dsn, observer, _fixture)) = blank().await else {
                    return;
                };

                let provider = new_provider(&dsn)
                    .with_migrations(migrations("migrations-boot", MigrationSettings::default()).await);
                assert!(
                    !provider.migrates_on_boot(),
                    "the default policy must not migrate"
                );

                support::initialise(&provider).await.expect("boots");

                assert_eq!(provider.readiness(), Readiness::Ready);
                assert!(
                    !table_exists(&observer, "rv_boot_marker").await,
                    "the default policy changed the schema"
                );
                assert!(
                    !table_exists(&observer, "_sqlx_migrations").await,
                    "the default policy created migration bookkeeping"
                );

                provider.stop().await.expect("stops");
                observer.close().await.expect("closes");
            }

            /// A provider with no migrations at all also boots and migrates nothing.
            #[tokio::test]
            async fn a_provider_without_migrations_boots_and_changes_nothing() {
                let Some((dsn, observer, _fixture)) = blank().await else {
                    return;
                };
                let provider = new_provider(&dsn);
                assert!(!provider.migrates_on_boot());

                support::initialise(&provider).await.expect("boots");

                assert_eq!(provider.readiness(), Readiness::Ready);
                assert!(!table_exists(&observer, "_sqlx_migrations").await);

                provider.stop().await.expect("stops");
                observer.close().await.expect("closes");
            }

            // ------------------------------------------------------------- 3. re-boot

            /// An unchanged, already-applied set boots again and reaches ready.
            ///
            /// The ordinary redeployment. It must not be mistaken for a checksum problem, and it
            /// must not apply anything twice.
            #[tokio::test]
            async fn an_unchanged_applied_set_boots_again() {
                let Some((dsn, observer, _fixture)) = blank().await else {
                    return;
                };

                for attempt in 0..3 {
                    let provider = new_provider(&dsn)
                        .with_migrations(migrations("migrations-boot", on_boot()).await);
                    support::initialise(&provider)
                        .await
                        .unwrap_or_else(|_| panic!("boot {attempt} failed"));
                    assert_eq!(provider.readiness(), Readiness::Ready);
                    provider.stop().await.expect("stops");
                }

                // `mark` is the primary key, so a migration applied twice would have failed the
                // second boot outright. Counting proves the bookkeeping, not just the absence of
                // an error.
                let rows: i64 = sqlx::query_scalar(AssertSqlSafe(
                    "SELECT count(*) FROM rv_boot_marker".to_owned(),
                ))
                .fetch_one(observer.pool())
                .await
                .expect("counts");
                assert_eq!(rows, 1, "a repeated boot re-applied a migration");

                observer.close().await.expect("closes");
            }

            // ------------------------------------------------------------- 4. checksum

            /// A changed applied migration refuses boot, and refuses it **before** touching the
            /// schema.
            ///
            /// The changed set carries a third migration that creates `rv_should_not_exist`. If the
            /// refusal happened after the loop began, that table would be there.
            #[tokio::test]
            async fn a_changed_applied_migration_refuses_boot_before_modifying_the_schema() {
                let Some((dsn, observer, _fixture)) = blank().await else {
                    return;
                };

                let first =
                    new_provider(&dsn).with_migrations(migrations("migrations-boot", on_boot()).await);
                support::initialise(&first).await.expect("first boot");
                first.stop().await.expect("stops");

                let second = new_provider(&dsn)
                    .with_migrations(migrations("migrations-boot-changed", on_boot()).await);
                let outcome = support::initialise(&second).await;

                assert!(outcome.is_err(), "a changed checksum booted");
                assert_eq!(
                    second.readiness(),
                    Readiness::NotReady,
                    "a refused boot reported ready"
                );
                assert!(
                    second.database().is_none(),
                    "a refused boot published a database"
                );
                assert!(
                    !table_exists(&observer, "rv_should_not_exist").await,
                    "THE SCHEMA WAS MODIFIED BEFORE THE CHECKSUM WAS REFUSED"
                );

                observer.close().await.expect("closes");
            }

            // ------------------------------------------------------------- 5. dirty

            /// A dirty migration table refuses boot.
            ///
            /// Written directly rather than produced by killing a real run: MySQL migrations are
            /// not atomic, so manufacturing a genuine interruption is engine-dependent, while the
            /// **state** an interruption leaves is identical and is what the boot path must reject.
            #[tokio::test]
            async fn a_dirty_migration_refuses_boot() {
                let Some((dsn, observer, _fixture)) = blank().await else {
                    return;
                };

                let first =
                    new_provider(&dsn).with_migrations(migrations("migrations-boot", on_boot()).await);
                support::initialise(&first).await.expect("first boot");
                first.stop().await.expect("stops");

                sqlx::query(AssertSqlSafe(
                    "UPDATE _sqlx_migrations SET success = false WHERE version = 20260201000002"
                        .to_owned(),
                ))
                .execute(observer.pool())
                .await
                .expect("dirties");

                let second =
                    new_provider(&dsn).with_migrations(migrations("migrations-boot", on_boot()).await);
                let outcome = support::initialise(&second).await;

                assert!(outcome.is_err(), "a dirty migration table booted");
                assert_eq!(second.readiness(), Readiness::NotReady);
                assert!(second.database().is_none());

                observer.close().await.expect("closes");
            }

            // ------------------------------------------------------------- 6. concurrency

            /// Concurrent starters apply each migration exactly once.
            ///
            /// Asserted against the **bookkeeping table**, not against the sum of the reports. Under
            /// a race, more than one starter may truthfully report "this was not applied when I
            /// began"; what must hold is that the database applied it once.
            #[tokio::test]
            async fn concurrent_starters_apply_each_migration_exactly_once() {
                let Some((dsn, observer, _fixture)) = blank().await else {
                    return;
                };

                let mut starters = Vec::new();
                for _ in 0..4 {
                    let dsn = dsn.clone();
                    starters.push(tokio::spawn(async move {
                        let provider = SqlxProvider::<$driver>::new(
                            ProviderId::new("database"),
                            CapabilityId::new("database"),
                            dsn,
                            support::settings(),
                            $kind,
                        )
                        .with_migrations(
                            Migrations::load(
                                &Path::new(env!("CARGO_MANIFEST_DIR"))
                                    .join("tests")
                                    .join("migrations-boot"),
                                MigrationSettings::default().with_policy(MigrationPolicy::OnBoot),
                            )
                            .await
                            .expect("loads"),
                        );
                        let outcome = support::initialise(&provider).await;
                        let _ = provider.stop().await;
                        outcome.is_ok()
                    }));
                }

                let mut booted = 0;
                for starter in starters {
                    if starter.await.expect("the starter did not panic") {
                        booted += 1;
                    }
                }
                assert_eq!(booted, 4, "a concurrent starter failed to boot");

                let applied: i64 = sqlx::query_scalar(AssertSqlSafe(
                    "SELECT count(*) FROM _sqlx_migrations".to_owned(),
                ))
                .fetch_one(observer.pool())
                .await
                .expect("counts");
                assert_eq!(applied, 2, "a migration was recorded more than once");

                let rows: i64 = sqlx::query_scalar(AssertSqlSafe(
                    "SELECT count(*) FROM rv_boot_marker".to_owned(),
                ))
                .fetch_one(observer.pool())
                .await
                .expect("counts");
                assert_eq!(rows, 1, "a migration ran more than once");

                observer.close().await.expect("closes");
            }

            // ------------------------------------------------------------- 7. lock deadline

            /// A held migration lock is refused within the configured bound.
            ///
            /// The competing lock is taken through `sqlx::migrate::Migrate::lock` on a connection
            /// this test owns, so it is byte-identical to the one the migrator would take — no
            /// guess about how the driver derives its key.
            ///
            /// Both engines wait forever by default: `sqlx-mysql` issues `SELECT GET_LOCK(?, -1)`
            /// and `sqlx-postgres` issues `pg_advisory_lock`, neither of which honours a statement
            /// timeout. Renvor's bound is the only thing between this and a hung boot.
            #[tokio::test]
            async fn a_held_lock_is_refused_within_the_configured_bound() {
                use sqlx::Connection as _;
                use sqlx::migrate::Migrate as _;

                let Some((dsn, observer, _fixture)) = blank().await else {
                    return;
                };

                let mut holder = <$driver as sqlx::Database>::Connection::connect(dsn.expose())
                    .await
                    .expect("the competing session connects");
                holder.lock().await.expect("takes the migration lock");

                let settings = on_boot()
                    .with_lock_timeout(Duration::from_secs(2))
                    .expect("bounded");
                let provider =
                    new_provider(&dsn).with_migrations(migrations("migrations-boot", settings).await);

                // THE DEADLINE IS THE ASSERTION, not the elapsed-time check below it.
                // Measuring elapsed time *after* the call returns cannot fail when the wait is
                // unbounded — it hangs instead, and a hung test in CI reports a timeout with no
                // name attached. This bound turns the same regression into a named failure.
                let started = Instant::now();
                let outcome = tokio::time::timeout(
                    Duration::from_secs(30),
                    support::initialise(&provider),
                )
                .await
                .expect("THE LOCK WAIT WAS NOT BOUNDED — boot is still waiting for the lock");
                let elapsed = started.elapsed();

                assert!(outcome.is_err(), "boot proceeded while the lock was held");
                assert!(
                    elapsed < Duration::from_secs(30),
                    "the lock wait was not bounded: {elapsed:?}"
                );
                assert_eq!(provider.readiness(), Readiness::NotReady);
                assert!(provider.database().is_none());

                // Releasing the competing session must leave the next starter able to migrate. A
                // boot that failed while leaking its own lock would hang here instead.
                holder.close().await.expect("releases");
                let recovered =
                    new_provider(&dsn).with_migrations(migrations("migrations-boot", on_boot()).await);
                tokio::time::timeout(
                    Duration::from_secs(30),
                    support::initialise(&recovered),
                )
                .await
                .expect("A LEAKED MIGRATION LOCK WOULD HANG HERE")
                .expect("boots once the lock is free");
                recovered.stop().await.expect("stops");

                assert_eq!(marker(&observer).await.as_deref(), Some("migrated-on-boot"));
                observer.close().await.expect("closes");
            }

            // ------------------------------------------------------------- 8. run deadline

            /// The whole-run deadline is bounded, and elapsing it refuses boot.
            ///
            /// The slow set sleeps for ten seconds inside a single migration; the deadline is
            /// three. A run that ignored its bound would take the full ten and then report
            /// SUCCESS — which is why the assertion below is on the outcome as well as the clock.
            #[tokio::test]
            async fn the_whole_run_deadline_is_bounded() {
                let Some((dsn, observer, _fixture)) = blank().await else {
                    return;
                };

                let settings = on_boot()
                    .with_run_timeout(Duration::from_secs(3))
                    .expect("bounded");
                let provider = new_provider(&dsn).with_migrations(migrations($slow, settings).await);

                // TWELVE SECONDS, AND THE ARITHMETIC IS THE POINT.
                //
                // The bound `initialise` promises is `run_timeout + CLEANUP_TIMEOUT`, not
                // `run_timeout`. After the run deadline fires, the migration session still has to
                // be ended, and on MySQL that close waits behind the statement it is interrupting —
                // so it uses its full five seconds. 3 + 5 = 8, and an 8s assertion is therefore a
                // race that PostgreSQL wins and MySQL loses. Measured, not guessed: this test
                // failed on MySQL at exactly that boundary and passed on PostgreSQL.
                //
                // Bounded for the same reason as the lock test above: without a deadline at all
                // this waits out the migration's own ten seconds rather than failing.
                let started = Instant::now();
                let outcome = tokio::time::timeout(
                    Duration::from_secs(12),
                    support::initialise(&provider),
                )
                .await
                .expect("THE RUN DEADLINE WAS NOT HONOURED — the migration is still running");
                let elapsed = started.elapsed();

                // The outcome assertion is the one that catches a deleted deadline: without it the
                // migration SUCCEEDS after ten seconds, which is inside the bound above.
                assert!(outcome.is_err(), "an over-long migration booted");
                assert!(
                    elapsed < Duration::from_secs(12),
                    "the run deadline was not honoured: {elapsed:?}"
                );
                assert_eq!(provider.readiness(), Readiness::NotReady);
                assert!(provider.database().is_none());

                // The abandoned run must not have left its lock behind.
                let recovered =
                    new_provider(&dsn).with_migrations(migrations("migrations-boot", on_boot()).await);
                tokio::time::timeout(
                    Duration::from_secs(30),
                    support::initialise(&recovered),
                )
                .await
                .expect("A LEAKED MIGRATION LOCK WOULD HANG HERE")
                .expect("boots after the timed-out run");
                recovered.stop().await.expect("stops");

                observer.close().await.expect("closes");
            }

            // ------------------------------------------------------------- 9. cleanup

            /// A failed migration closes its dedicated connection and the pool it opened.
            ///
            /// Observed **server-side**, through a session this test owns. Asking the pool whether
            /// it thinks it is closed would only prove the provider set a flag; counting the
            /// sessions the server still holds proves the sockets are gone.
            #[tokio::test]
            async fn a_failed_migration_closes_its_connection_and_its_pool() {
                let Some((dsn, observer, _fixture)) = blank().await else {
                    return;
                };

                let first =
                    new_provider(&dsn).with_migrations(migrations("migrations-boot", on_boot()).await);
                support::initialise(&first).await.expect("first boot");
                first.stop().await.expect("stops");

                let baseline = support::sessions(observer.pool()).await;

                let second = new_provider(&dsn)
                    .with_migrations(migrations("migrations-boot-changed", on_boot()).await);
                support::initialise(&second)
                    .await
                    .expect_err("a changed checksum must refuse boot");

                // The server reaps a closed socket promptly but not instantly, so this polls to a
                // deadline rather than sleeping for a guessed interval.
                let deadline = Instant::now() + Duration::from_secs(20);
                loop {
                    let now = support::sessions(observer.pool()).await;
                    if now <= baseline {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "a failed migration left {} session(s) open above the {baseline} baseline",
                        now - baseline
                    );
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }

                observer.close().await.expect("closes");
            }

            /// A failed boot publishes no database, so nothing downstream can reach a half-built
            /// one.
            #[tokio::test]
            async fn a_failed_boot_publishes_no_database() {
                let Some((dsn, observer, _fixture)) = blank().await else {
                    return;
                };

                let first =
                    new_provider(&dsn).with_migrations(migrations("migrations-boot", on_boot()).await);
                support::initialise(&first).await.expect("first boot");
                first.stop().await.expect("stops");

                let second = new_provider(&dsn)
                    .with_migrations(migrations("migrations-boot-changed", on_boot()).await);
                support::initialise(&second).await.expect_err("refuses");

                assert!(second.database().is_none(), "a failed boot published a pool");
                assert_eq!(second.readiness(), Readiness::NotReady);
                // `stop` after a failed boot must be a no-op rather than a panic: the kernel calls
                // it during rollback for providers that never initialised.
                second.stop().await.expect("stop after a failed boot");

                observer.close().await.expect("closes");
            }

            // ------------------------------------------------------------- 10. cancellation

            /// Cancelling boot mid-migration must not park a LOCK-HOLDING connection in the pool.
            ///
            /// # What this can prove, and what it cannot
            ///
            /// It cannot prove that the lock is released quickly. Measured, the recovery takes as
            /// long as the abandoned statement had left to run — 28.5s for a 30s sleep cancelled at
            /// 1.5s, on both engines. That is inherent rather than a defect: PostgreSQL processes a
            /// Terminate only after the current statement finishes, and `sqlx` issues no separate
            /// cancel request on drop. A test asserting a short recovery would be asserting
            /// something no client-side code can deliver.
            ///
            /// What it CAN prove is the thing that actually goes wrong. When the migration future
            /// is dropped, `run_direct` never reaches its unlock. Without
            /// `PoolConnection::close_on_drop`, `sqlx` returns that connection to the **pool** once
            /// its statement drains — still holding a database-wide advisory lock — where it sits
            /// available for unrelated work and blocks every future migrator for as long as the
            /// pool lives.
            ///
            /// So the cancelled provider is deliberately **kept alive** below. Dropping it would
            /// close its pool and release the lock as a side effect, which is exactly how the
            /// earlier version of this test passed whether or not the guard existed.
            #[tokio::test]
            async fn cancelling_boot_mid_migration_leaves_no_lock_in_the_pool() {
                let Some((dsn, observer, _fixture)) = blank().await else {
                    return;
                };
                let baseline = support::sessions(observer.pool()).await;

                // NOT in an inner scope. See above: dropping this would do the guard's job for it.
                let cancelled = new_provider(&dsn)
                    .with_migrations(migrations($slow, on_boot()).await);
                let outcome = tokio::time::timeout(
                    Duration::from_millis(1500),
                    support::initialise(&cancelled),
                )
                .await;
                assert!(outcome.is_err(), "the slow migration finished too early");
                assert!(cancelled.database().is_none());
                assert_eq!(cancelled.readiness(), Readiness::NotReady);

                // The abandoned statement sleeps for ten seconds; thirty leaves room for it to
                // drain and for the guard to end the session, and is still finite.
                let recovered = new_provider(&dsn)
                    .with_migrations(migrations("migrations-boot", on_boot()).await);
                let started = Instant::now();
                tokio::time::timeout(Duration::from_secs(30), support::initialise(&recovered))
                    .await
                    .expect(
                        "A CANCELLED BOOT PARKED A LOCK-HOLDING CONNECTION IN ITS POOL — the next                          migrator can never take the lock while that pool lives",
                    )
                    .expect("boots after a cancelled boot");
                assert_eq!(marker(&observer).await.as_deref(), Some("migrated-on-boot"));
                println!("recovery after cancellation: {:?}", started.elapsed());

                recovered.stop().await.expect("stops");
                drop(cancelled);

                // And once the cancelled provider IS dropped, nothing of it remains server-side.
                let deadline = Instant::now() + Duration::from_secs(30);
                loop {
                    let now = support::sessions(observer.pool()).await;
                    if now <= baseline {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "a cancelled boot leaked {} session(s)",
                        now - baseline
                    );
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }

                observer.close().await.expect("closes");
            }

            // ------------------------------------------------------------- 11. redaction

            /// A migration failure names no credential, no SQL, and no filesystem path.
            ///
            /// The DSN carries a canary chosen to be unmistakable. Asserting the **absence** of it
            /// is stronger than asserting the presence of a redaction marker: a marker can be
            /// printed alongside the secret it was supposed to replace.
            #[tokio::test]
            async fn a_migration_failure_names_no_credential_no_sql_and_no_path() {
                let Some((dsn, observer, _fixture)) = blank().await else {
                    return;
                };

                let first =
                    new_provider(&dsn).with_migrations(migrations("migrations-boot", on_boot()).await);
                support::initialise(&first).await.expect("first boot");
                first.stop().await.expect("stops");

                let poisoned = ConnectionString::new(
                    dsn.expose().replace("devpassword", support::CREDENTIAL_CANARY),
                );
                let second = new_provider(&poisoned)
                    .with_migrations(migrations("migrations-boot-changed", on_boot()).await);
                let error = support::initialise(&second)
                    .await
                    .expect_err("a changed checksum must refuse boot");

                let rendered = format!("{error} {error:?}");
                assert!(
                    !rendered.contains(support::CREDENTIAL_CANARY),
                    "a credential reached a migration diagnostic"
                );
                assert!(
                    !rendered.contains("CREATE TABLE") && !rendered.contains("INSERT INTO"),
                    "migration SQL reached a diagnostic: {rendered}"
                );
                assert!(
                    !rendered.contains("migrations-boot") && !rendered.contains('/'),
                    "a filesystem path reached a diagnostic: {rendered}"
                );
                // The provider's own `Debug` is on the same audit path.
                let printed = format!("{second:?}");
                assert!(!printed.contains(support::CREDENTIAL_CANARY));
                assert!(!printed.contains('/'));

                observer.close().await.expect("closes");
            }

            /// The refusal is classified, not merely "something went wrong".
            #[tokio::test]
            async fn a_changed_checksum_is_classified_rather_than_unclassified() {
                let Some((dsn, observer, _fixture)) = blank().await else {
                    return;
                };

                let first =
                    new_provider(&dsn).with_migrations(migrations("migrations-boot", on_boot()).await);
                support::initialise(&first).await.expect("first boot");
                first.stop().await.expect("stops");

                let changed = migrations("migrations-boot-changed", on_boot()).await;
                let error = changed
                    .$run(&observer)
                    .await
                    .expect_err("a changed checksum must be refused");
                assert_eq!(error.kind(), DatabaseErrorKind::MigrationChecksumMismatch);

                observer.close().await.expect("closes");
            }

            // ------------------------------------------------------------- 12. shutdown

            /// Stopping after a migrated boot stays bounded.
            ///
            /// The migration path opens a dedicated connection outside the pool's ordinary
            /// borrowing. If it were left attached, shutdown would wait on it.
            #[tokio::test]
            async fn stopping_after_a_migrated_boot_is_bounded() {
                let Some((dsn, observer, _fixture)) = blank().await else {
                    return;
                };

                let provider =
                    new_provider(&dsn).with_migrations(migrations("migrations-boot", on_boot()).await);
                support::initialise(&provider).await.expect("boots");

                let started = Instant::now();
                tokio::time::timeout(Duration::from_secs(30), provider.stop())
                    .await
                    .expect("SHUTDOWN HUNG ON MIGRATION CLEANUP")
                    .expect("stops");
                assert!(
                    started.elapsed() < Duration::from_secs(30),
                    "shutdown took {:?}",
                    started.elapsed()
                );
                assert_eq!(provider.readiness(), Readiness::NotReady);

                observer.close().await.expect("closes");
            }
        }
    };
}

boot_suite!(
    postgres,
    "db-postgres",
    sqlx::Postgres,
    renvor_sqlx::connect_postgres,
    run_postgres,
    support::POSTGRES_URL,
    renvor_database::DatabaseKind::Postgres,
    "migrations-boot-slow-postgres"
);
boot_suite!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    renvor_sqlx::connect_mysql,
    run_mysql,
    support::MYSQL_URL,
    renvor_database::DatabaseKind::MySql,
    "migrations-boot-slow-mysql"
);
