//! Cancellation must not cost the pool capacity — and SeaORM's own transaction cannot promise it.
//!
//! # The two halves of this file
//!
//! `native_*` tests exercise `sea_orm::DatabaseTransaction` **directly**, on a pool this crate
//! owns. They are the red half: they are what Phase 007 would have shipped if the adapter had
//! wrapped SeaORM's transaction instead of replacing it, and they are kept so the decision in
//! ADR-0021 is a measurement rather than an argument.
//!
//! Everything else exercises [`renvor_seaorm::SeaOrmUnitOfWork`], which must pass.
//!
//! # Why the cancellation lands where it does
//!
//! Every test uses a **server-side** sleep, so the cancellation is guaranteed to land while a
//! statement is in flight. Phase 006 learned that a fixed client-side deadline lands at an
//! uncontrolled point and produces results that look like a difference between database versions
//! when they are only a difference in which code path got sampled.

mod support;

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use std::time::{Duration, Instant};

/// How long the pool may take to regain its full configured capacity after a cancellation.
///
/// Deliberately far below the ~9.5s an abandoned ten-second sleep pins a returned connection for,
/// so "recovered by waiting for the abandoned statement" cannot pass this gate.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
const RECOVERY_BOUND: Duration = Duration::from_secs(2);

/// The pool is tiny so that the loss of a single slot is unmistakable.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
const CAPACITY: u32 = 2;

macro_rules! suite {
    (
        $module:ident, $feature:literal, $alias:ty, $connect:path, $url:expr,
        $connector:path, $sleep:literal, $insert:literal
    ) => {
        #[cfg(feature = $feature)]
        mod $module {
            use super::*;
            use renvor_database::{Database, PoolSettings, UnitOfWork};
            use sea_orm::{ConnectionTrait as _, TransactionTrait as _};
            use sqlx::AssertSqlSafe;

            /// A clean fixture, and the guard that keeps it clean for the test's duration.
            ///
            /// Every test here shares one table. Returning the guard puts the serialisation
            /// requirement in the type rather than in a `--test-threads=1` nobody passes.
            async fn database() -> Option<($alias, tokio::sync::MutexGuard<'static, ()>)> {
                let guard = support::SHARED_FIXTURE.lock().await;
                let dsn = support::url($url)?;
                let settings = PoolSettings::default()
                    .with_max_connections(CAPACITY)
                    .expect("bounded")
                    // Short, so a lost slot surfaces as a fast refusal rather than a long stall.
                    .with_acquire_timeout(Duration::from_millis(750))
                    .expect("bounded");
                let database = $connect(&dsn, &settings).await.expect("connects");
                sqlx::query(AssertSqlSafe(
                    "CREATE TABLE IF NOT EXISTS rv_sea_cancel (id BIGINT PRIMARY KEY)".to_owned(),
                ))
                .execute(database.pool())
                .await
                .expect("creates");
                sqlx::query(AssertSqlSafe("DELETE FROM rv_sea_cancel".to_owned()))
                    .execute(database.pool())
                    .await
                    .expect("clears");
                Some((database, guard))
            }

            /// Whether the pool can hand out every connection it is configured for.
            ///
            /// Functional rather than metric-based: `size` and `num_idle` read the same in the
            /// healthy and the stranded case, so only actually acquiring proves capacity.
            async fn full_capacity(database: &$alias) -> bool {
                let mut held = Vec::new();
                for _ in 0..CAPACITY {
                    match database.begin().await {
                        Ok(uow) => held.push(uow),
                        Err(_) => {
                            for uow in held {
                                let _ = uow.rollback().await;
                            }
                            return false;
                        }
                    }
                }
                for uow in held {
                    let _ = uow.rollback().await;
                }
                true
            }

            async fn count(database: &$alias) -> i64 {
                sqlx::query_scalar(AssertSqlSafe(
                    "SELECT COUNT(*) FROM rv_sea_cancel".to_owned(),
                ))
                .fetch_one(database.pool())
                .await
                .expect("counts")
            }

            // ── Renvor's unit of work ─────────────────────────────────────────────────────

            /// Cancels mid-statement, through the idiomatic SeaORM surface.
            async fn cancel_mid_statement(database: &$alias) {
                let _ = tokio::time::timeout(Duration::from_millis(500), async {
                    let uow = database.begin().await.expect("begins");
                    // `ConnectionTrait`, not a SQLx call: the path an application actually uses.
                    uow.execute_unprepared($sleep).await.expect("sleeps");
                    uow.commit().await.expect("commits");
                })
                .await;
            }

            #[tokio::test]
            async fn one_cancellation_returns_capacity_within_a_bound() {
                let Some((database, _fixture)) = database().await else {
                    return;
                };
                cancel_mid_statement(&database).await;

                let started = Instant::now();
                let recovered = full_capacity(&database).await;
                let elapsed = started.elapsed();

                assert!(recovered, "pool never regained capacity after one cancellation");
                assert!(
                    elapsed < RECOVERY_BOUND,
                    "capacity took {elapsed:?}, over the {RECOVERY_BOUND:?} bound"
                );
            }

            #[tokio::test]
            async fn repeated_cancellation_does_not_shrink_capacity() {
                let Some((database, _fixture)) = database().await else {
                    return;
                };
                for round in 0..5 {
                    cancel_mid_statement(&database).await;
                    assert!(
                        full_capacity(&database).await,
                        "capacity lost after cancellation {round}"
                    );
                }
            }

            #[tokio::test]
            async fn cancelled_work_commits_nothing() {
                let Some((database, _fixture)) = database().await else {
                    return;
                };
                let _ = tokio::time::timeout(Duration::from_millis(500), async {
                    let uow = database.begin().await.expect("begins");
                    uow.execute_unprepared(
                        "INSERT INTO rv_sea_cancel (id) VALUES (9001)",
                    )
                    .await
                    .expect("inserts");
                    uow.execute_unprepared($sleep).await.expect("sleeps");
                    uow.commit().await.expect("commits");
                })
                .await;

                // Observed from a DIFFERENT connection, so this is the visibility guarantee rather
                // than a read of the transaction's own uncommitted state.
                assert_eq!(count(&database).await, 0, "a cancelled transaction committed");
            }

            #[tokio::test]
            async fn cancellation_before_a_connection_is_acquired_is_clean() {
                let Some((database, _fixture)) = database().await else {
                    return;
                };
                // A one-nanosecond deadline lands before `begin` can return.
                let _ = tokio::time::timeout(Duration::from_nanos(1), database.begin()).await;
                assert!(
                    full_capacity(&database).await,
                    "cancelling before acquisition cost the pool a slot"
                );
            }

            #[tokio::test]
            async fn cancellation_after_a_write_but_before_commit_commits_nothing() {
                let Some((database, _fixture)) = database().await else {
                    return;
                };
                {
                    let uow = database.begin().await.expect("begins");
                    uow.execute_unprepared("INSERT INTO rv_sea_cancel (id) VALUES (9002)")
                        .await
                        .expect("inserts");
                    // Dropped here without commit — the ordinary early-return shape.
                }
                assert_eq!(count(&database).await, 0, "a dropped transaction committed");
                assert!(full_capacity(&database).await, "drop cost the pool a slot");
            }

            #[tokio::test]
            async fn commit_reuses_a_pooled_connection() {
                let Some((database, _fixture)) = database().await else {
                    return;
                };
                // The capacity guarantee must NOT be met by discarding every connection. Reuse is
                // asserted so that "detach everything" can never quietly become the implementation.
                let before = database.connections().idle();
                let uow = database.begin().await.expect("begins");
                uow.execute_unprepared("INSERT INTO rv_sea_cancel (id) VALUES (1)")
                    .await
                    .expect("inserts");
                uow.commit().await.expect("commits");
                assert_eq!(count(&database).await, 1, "commit did not persist");
                assert!(
                    database.connections().idle() >= before,
                    "a committed transaction did not return a reusable connection"
                );
            }

            #[tokio::test]
            async fn rollback_reuses_a_pooled_connection() {
                let Some((database, _fixture)) = database().await else {
                    return;
                };
                let before = database.connections().idle();
                let uow = database.begin().await.expect("begins");
                uow.execute_unprepared("INSERT INTO rv_sea_cancel (id) VALUES (2)")
                    .await
                    .expect("inserts");
                uow.rollback().await.expect("rolls back");
                assert_eq!(count(&database).await, 0, "rollback did not undo");
                assert!(
                    database.connections().idle() >= before,
                    "an explicit rollback did not return a reusable connection"
                );
            }

            #[tokio::test]
            async fn a_second_begin_is_a_separate_session_rather_than_a_nested_one() {
                let Some((database, _fixture)) = database().await else {
                    return;
                };
                let outer = database.begin().await.expect("begins");
                outer
                    .execute_unprepared("INSERT INTO rv_sea_cancel (id) VALUES (3)")
                    .await
                    .expect("inserts");
                let inner = database.begin().await.expect("begins a second");
                let seen: i64 = inner
                    .query_one_raw(sea_orm::Statement::from_string(
                        inner.get_database_backend(),
                        "SELECT COUNT(*) AS c FROM rv_sea_cancel",
                    ))
                    .await
                    .expect("queries")
                    .expect("a row")
                    .try_get("", "c")
                    .expect("decodes");
                assert_eq!(seen, 0, "the outer transaction's write was visible: nested, not separate");
                let _ = inner.rollback().await;
                let _ = outer.rollback().await;
            }

            #[tokio::test]
            async fn shutdown_is_bounded_while_cancellation_cleanup_is_active() {
                let Some((database, _fixture)) = database().await else {
                    return;
                };
                cancel_mid_statement(&database).await;
                let started = Instant::now();
                let _ = database.close().await;
                assert!(
                    started.elapsed() < Duration::from_secs(10),
                    "close took {:?} with cleanup in flight",
                    started.elapsed()
                );
            }

            // ── The red half: SeaORM's own transaction, on the same pool ──────────────────

            /// Cancels mid-statement using `sea_orm::DatabaseTransaction` directly.
            async fn native_cancel_mid_statement(pool: &sea_orm::DatabaseConnection) {
                let _ = tokio::time::timeout(Duration::from_millis(500), async {
                    let txn = pool.begin().await.expect("begins");
                    txn.execute_unprepared($sleep).await.expect("sleeps");
                    txn.commit().await.expect("commits");
                })
                .await;
            }

            /// Measures how long the native transaction denies the pool its configured capacity.
            ///
            /// # This reports a duration, not a verdict, and the distinction is the point
            ///
            /// Two different failures look identical at a single instant. A connection whose
            /// abandoned statement is still running is **stranded** — it comes back when the
            /// server finishes, roughly ten seconds later. A connection whose return path is
            /// waiting on a packet that will never arrive is **lost permanently**. ADR-0017
            /// measured the second at 0 in 12 trials on PostgreSQL and 2–6 in 12 on MySQL.
            ///
            /// Asserting "the native path fails" without separating these would overstate the
            /// PostgreSQL case, so this measures time-to-recovery against a ceiling well past the
            /// ten-second sleep and prints it. The number in the evidence file is then one this
            /// run produced, and it says which of the two failures happened.
            ///
            /// Either way it is a contract failure: `renvor_database::UnitOfWork` requires the
            /// pool to regain capacity *"within a bounded, deterministic time"*, and ten seconds
            /// of denial per cancelled request is not one.
            #[tokio::test]
            async fn native_transaction_denies_capacity_for_a_measured_duration() {
                let Some((database, _fixture)) = database().await else {
                    return;
                };
                let native = $connector(database.pool().clone());
                native_cancel_mid_statement(&native).await;

                // Well past the 10s sleep, so "stranded" and "lost" separate cleanly.
                const CEILING: Duration = Duration::from_secs(25);
                let started = Instant::now();
                let mut recovered = None;
                while started.elapsed() < CEILING {
                    if full_capacity(&database).await {
                        recovered = Some(started.elapsed());
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }

                match recovered {
                    Some(after) => println!(
                        "NATIVE sea_orm::DatabaseTransaction on {}: capacity STRANDED for {after:?}                          after one mid-statement cancellation (Renvor's bound is {RECOVERY_BOUND:?})",
                        stringify!($module)
                    ),
                    None => println!(
                        "NATIVE sea_orm::DatabaseTransaction on {}: capacity NOT recovered within                          {CEILING:?} — permanently lost, not merely stranded",
                        stringify!($module)
                    ),
                }

                // The comparison that is actually asserted: Renvor's own path, same pool, same
                // cancellation, must be back inside its bound. A run where BOTH recovered instantly
                // would mean the probe measured nothing, and this is what would catch that.
                let renvor_started = Instant::now();
                cancel_mid_statement(&database).await;
                assert!(
                    full_capacity(&database).await,
                    "Renvor's unit of work failed to recover capacity"
                );
                assert!(
                    renvor_started.elapsed() < RECOVERY_BOUND + Duration::from_millis(600),
                    "Renvor recovered in {:?}, outside its own bound",
                    renvor_started.elapsed()
                );
            }

            /// The exact reason Renvor does not wrap SeaORM's transaction.
            ///
            /// # What this asserts, and why it is not a behavioural test
            ///
            /// `sea_orm::DatabaseTransaction::drop` is
            /// `self.start_rollback().expect("Fail to rollback transaction")`, and
            /// `start_rollback` returns `Err` when its `try_lock` fails. The connection lives
            /// behind an `Arc<Mutex<_>>`, so `try_lock` is the only option available to a `Drop`
            /// that has no `&mut`.
            ///
            /// Staging that lock contention deterministically from outside the crate is not
            /// possible — which is exactly why the guarantee is taken structurally instead:
            /// Renvor's unit of work holds its connection in a mutex that is **not** shared, so
            /// `Drop` reaches it with `get_mut()`, which cannot fail and has no `expect` to reach.
            ///
            /// This test asserts the structural property that makes the panic unreachable: a
            /// cancelled Renvor unit of work leaves the pool whole, every time, with no round in
            /// which cleanup was skipped.
            #[tokio::test]
            async fn renvor_owns_the_drop_path_because_seaorms_can_panic() {
                let Some((database, _fixture)) = database().await else {
                    return;
                };
                for round in 0..6 {
                    cancel_mid_statement(&database).await;
                    assert!(
                        full_capacity(&database).await,
                        "round {round}: Renvor's unit of work lost pool capacity, which is the \
                         guarantee that justifies not using sea_orm::DatabaseTransaction"
                    );
                }
            }
        }
    };
}

suite!(
    postgres,
    "db-postgres",
    renvor_seaorm::PostgresDatabase,
    renvor_seaorm::connect_postgres,
    support::POSTGRES_URL,
    sea_orm::SqlxPostgresConnector::from_sqlx_postgres_pool,
    "SELECT pg_sleep(10)",
    "INSERT INTO rv_sea_cancel (id) VALUES ($1)"
);

suite!(
    mysql,
    "db-mysql",
    renvor_seaorm::MySqlDatabase,
    renvor_seaorm::connect_mysql,
    support::MYSQL_URL,
    sea_orm::SqlxMySqlConnector::from_sqlx_mysql_pool,
    "SELECT SLEEP(10)",
    "INSERT INTO rv_sea_cancel (id) VALUES (?)"
);
