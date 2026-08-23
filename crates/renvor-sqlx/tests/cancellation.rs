//! Cancellation must not cost the pool capacity.
//!
//! # What this suite exists to catch
//!
//! Dropping a future does not cancel the statement it started. The client stops waiting; the
//! server keeps working. Until the driver has read the response it is still owed, the connection
//! cannot be reused — and if the driver's bookkeeping is disturbed while that response is in
//! flight, it may wait for a packet that will never arrive.
//!
//! `sqlx-core`'s return path calls `Connection::ping()` with **no deadline**, while holding the
//! guard that keeps `Pool::size()` incremented. A ping that never completes therefore costs the
//! pool one slot *permanently*, and the pool will not open a replacement because it still counts
//! the lost connection against `max_connections`. Measured on MySQL 9.7.2 at 6 occurrences in 12
//! trials and on MySQL 8.4.11 at 2 in 12; PostgreSQL is immune because its protocol resynchronises
//! at `ReadyForQuery`.
//!
//! Every test here uses a **server-side** sleep so the cancellation is guaranteed to land while a
//! statement is in flight. An earlier probe used a fixed 200µs client deadline, which landed at an
//! uncontrolled point and produced results that looked like a difference between database versions
//! when they were only a difference in which code path got sampled.

mod support;

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use std::time::{Duration, Instant};

/// How long the pool may take to regain its full configured capacity after a cancellation.
///
/// Deliberately far below the ~9.5s an abandoned `SLEEP(10)` would pin a returned connection for,
/// so that "recovered by waiting for the abandoned statement" cannot pass this gate.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
const RECOVERY_BOUND: Duration = Duration::from_secs(2);

/// The pool is tiny so that the loss of a single slot is unmistakable.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
const CAPACITY: u32 = 2;

macro_rules! suite {
    ($module:ident, $feature:literal, $alias:ty, $connect:path, $url:expr, $sleep:literal, $insert:literal, $session:literal) => {
        #[cfg(feature = $feature)]
        mod $module {
            use super::*;
            use renvor_database::{Database, PoolSettings, UnitOfWork};
            use sqlx::AssertSqlSafe;

            async fn database() -> Option<$alias> {
                let dsn = support::url($url)?;
                let settings = PoolSettings::default()
                    .with_max_connections(CAPACITY)
                    .expect("bounded")
                    // Short, so a lost slot surfaces as a fast refusal rather than a long stall.
                    .with_acquire_timeout(Duration::from_millis(750))
                    .expect("bounded");
                let database = $connect(&dsn, &settings).await.expect("connects");
                sqlx::query(AssertSqlSafe(
                    "CREATE TABLE IF NOT EXISTS rv_cancel (id BIGINT PRIMARY KEY)".to_owned(),
                ))
                .execute(database.pool())
                .await
                .expect("creates");
                sqlx::query(AssertSqlSafe("DELETE FROM rv_cancel".to_owned()))
                    .execute(database.pool())
                    .await
                    .expect("clears");
                Some(database)
            }

            /// Abandons a statement while it is provably still executing on the server.
            async fn cancel_mid_statement(database: &$alias) {
                let _ = tokio::time::timeout(Duration::from_millis(500), async {
                    let mut uow = database.begin().await.expect("begins");
                    sqlx::query(AssertSqlSafe($sleep.to_owned()))
                        .execute(&mut **uow.inner())
                        .await
                        .expect("sleeps");
                    uow.commit().await.expect("commits");
                })
                .await;
            }

            /// Whether the pool can hand out every connection it is configured for.
            ///
            /// Functional rather than metric-based: `size` and `num_idle` both read the same in the
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
                    "SELECT COUNT(*) FROM rv_cancel".to_owned(),
                ))
                .fetch_one(database.pool())
                .await
                .expect("counts")
            }

            #[tokio::test]
            async fn one_cancellation_returns_capacity_within_a_bound() {
                let Some(database) = database().await else {
                    return;
                };
                cancel_mid_statement(&database).await;

                let started = Instant::now();
                let recovered = full_capacity(&database).await;
                let elapsed = started.elapsed();

                assert!(
                    recovered,
                    "pool never regained capacity after one cancellation"
                );
                assert!(
                    elapsed < RECOVERY_BOUND,
                    "capacity took {elapsed:?}, over the {RECOVERY_BOUND:?} bound"
                );
            }

            #[tokio::test]
            async fn repeated_cancellation_does_not_shrink_capacity() {
                let Some(database) = database().await else {
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
                let Some(database) = database().await else {
                    return;
                };
                let _ = tokio::time::timeout(Duration::from_millis(500), async {
                    let mut uow = database.begin().await.expect("begins");
                    sqlx::query(AssertSqlSafe($insert.to_owned()))
                        .bind(1_i64)
                        .execute(&mut **uow.inner())
                        .await
                        .expect("inserts");
                    sqlx::query(AssertSqlSafe($sleep.to_owned()))
                        .execute(&mut **uow.inner())
                        .await
                        .expect("sleeps");
                    uow.commit().await.expect("commits");
                })
                .await;

                assert_eq!(count(&database).await, 0, "cancelled work must not commit");
            }

            /// A commit must REUSE its connection.
            ///
            /// Without this, "discard the connection on cancellation" could be satisfied by
            /// discarding every connection always — correct, and quietly ruinous.
            #[tokio::test]
            async fn commit_reuses_a_pooled_connection() {
                let Some(database) = database().await else {
                    return;
                };
                let mut sessions = Vec::new();
                for _ in 0..6 {
                    let mut uow = database.begin().await.expect("begins");
                    let session: String =
                        sqlx::query_scalar(AssertSqlSafe($session.to_owned()))
                            .fetch_one(&mut **uow.inner())
                            .await
                            .expect("reads session");
                    uow.commit().await.expect("commits");
                    sessions.push(session);
                }
                // At most CAPACITY distinct sessions across six commits. The pool may
                // legitimately alternate between its slots, because `return_to_pool` is a
                // spawned task and a connection is not always back in the idle list before
                // the next acquire. What this must catch is six DISTINCT sessions, which is
                // what "discard the connection every time" would produce.
                let distinct = sessions.iter().collect::<std::collections::BTreeSet<_>>().len();
                assert!(
                    distinct <= CAPACITY as usize,
                    "commit opened a new server session instead of reusing one: {sessions:?}"
                );
            }

            /// An explicit rollback must also reuse its connection.
            #[tokio::test]
            async fn rollback_reuses_a_pooled_connection() {
                let Some(database) = database().await else {
                    return;
                };
                let mut sessions = Vec::new();
                for _ in 0..6 {
                    let mut uow = database.begin().await.expect("begins");
                    let session: String =
                        sqlx::query_scalar(AssertSqlSafe($session.to_owned()))
                            .fetch_one(&mut **uow.inner())
                            .await
                            .expect("reads session");
                    uow.rollback().await.expect("rolls back");
                    sessions.push(session);
                }
                let distinct = sessions.iter().collect::<std::collections::BTreeSet<_>>().len();
                assert!(
                    distinct <= CAPACITY as usize,
                    "rollback opened a new server session instead of reusing one: {sessions:?}"
                );
            }
        }
    };
}

suite!(
    postgres,
    "db-postgres",
    renvor_sqlx::PostgresDatabase,
    renvor_sqlx::connect_postgres,
    support::POSTGRES_URL,
    "SELECT pg_sleep(10)",
    "INSERT INTO rv_cancel (id) VALUES ($1)",
    "SELECT pg_backend_pid()::text"
);

suite!(
    mysql,
    "db-mysql",
    renvor_sqlx::MySqlDatabase,
    renvor_sqlx::connect_mysql,
    support::MYSQL_URL,
    "SELECT SLEEP(10)",
    "INSERT INTO rv_cancel (id) VALUES (?)",
    "SELECT CAST(CONNECTION_ID() AS CHAR)"
);
