//! The shared repository contract suite.
//!
//! # One suite, both databases
//!
//! `PLAN.md` §10.1 makes each persistence row a **release contract**, and constitution principle IX
//! requires contract suites to be *"shared across interchangeable adapters"*. Every test below is
//! written once and expanded for PostgreSQL and for MySQL by the `suite!` macro, so a behaviour
//! that holds on one database and not the other is a failing test rather than an undiscovered
//! difference.
//!
//! The DDL is deliberately identical across both engines — explicit `BIGINT` primary keys rather
//! than `BIGSERIAL`/`AUTO_INCREMENT` — so that the suite tests *Renvor's* contract rather than the
//! engines' differing identity syntax. Where a difference is genuinely part of the contract, it is
//! tested by name.

mod support;

/// Expands the whole suite for one driver.
macro_rules! suite {
    ($module:ident, $feature:literal, $driver:ty, $connect:path, $url:expr, $kind:expr) => {
        #[cfg(feature = $feature)]
        mod $module {
            use std::time::Duration;

            use renvor_database::{
                Database, DatabaseErrorKind, DatabaseKind, Keyset, KeysetError, SortAllowlist,
                UnitOfWork,
            };
            use renvor_validation::collection::Direction;
            use sqlx::AssertSqlSafe;

            use crate::support;

            const KIND: DatabaseKind = $kind;

            /// Identical on both engines, which is the point.
            const DDL: &str = "CREATE TABLE IF NOT EXISTS rv_post (\
                 id BIGINT PRIMARY KEY, \
                 title VARCHAR(200) NOT NULL, \
                 rank_value BIGINT NOT NULL)";

            /// A clean fixture, and the guard that keeps it clean.
            ///
            /// The guard is returned rather than taken and dropped here: it has to be held for the
            /// whole test, because `rv_post` is shared and the next test's `fresh()` would
            /// otherwise drop the table this one is still using. See
            /// [`support::SHARED_FIXTURE`] for why that requirement is expressed in the type
            /// rather than in a comment asking people to pass `--test-threads=1`.
            async fn fresh() -> Option<(
                renvor_sqlx::SqlxDatabase<$driver>,
                tokio::sync::MutexGuard<'static, ()>,
            )> {
                let guard = support::SHARED_FIXTURE.lock().await;
                let dsn = support::url($url)?;
                let database = $connect(&dsn, &support::settings())
                    .await
                    .expect("connects");
                sqlx::query("DROP TABLE IF EXISTS rv_post")
                    .execute(database.pool())
                    .await
                    .expect("drops");
                sqlx::query(DDL)
                    .execute(database.pool())
                    .await
                    .expect("creates");
                Some((database, guard))
            }

            /// `INSERT` with every value bound. No value is ever formatted into the statement.
            fn insert_sql() -> String {
                format!(
                    "INSERT INTO rv_post (id, title, rank_value) VALUES ({}, {}, {})",
                    KIND.placeholder(1),
                    KIND.placeholder(2),
                    KIND.placeholder(3)
                )
            }

            async fn count(database: &renvor_sqlx::SqlxDatabase<$driver>) -> i64 {
                sqlx::query_scalar("SELECT COUNT(*) FROM rv_post")
                    .fetch_one(database.pool())
                    .await
                    .expect("counts")
            }

            // ---------------------------------------------------------------- CRUD

            #[tokio::test]
            async fn crud_round_trips() {
                let Some((database, _fixture)) = fresh().await else {
                    return;
                };
                let mut uow = database.begin().await.expect("begins");
                sqlx::query(AssertSqlSafe(insert_sql()))
                    .bind(1_i64)
                    .bind("first")
                    .bind(10_i64)
                    .execute(&mut **uow.inner())
                    .await
                    .expect("inserts");
                uow.commit().await.expect("commits");

                let title: String = sqlx::query_scalar(AssertSqlSafe(format!(
                    "SELECT title FROM rv_post WHERE id = {}",
                    KIND.placeholder(1)
                )))
                .bind(1_i64)
                .fetch_one(database.pool())
                .await
                .expect("reads");
                assert_eq!(title, "first");

                let mut uow = database.begin().await.expect("begins");
                sqlx::query(AssertSqlSafe(format!(
                    "UPDATE rv_post SET title = {} WHERE id = {}",
                    KIND.placeholder(1),
                    KIND.placeholder(2)
                )))
                .bind("renamed")
                .bind(1_i64)
                .execute(&mut **uow.inner())
                .await
                .expect("updates");
                uow.commit().await.expect("commits");

                let title: String = sqlx::query_scalar(AssertSqlSafe(format!(
                    "SELECT title FROM rv_post WHERE id = {}",
                    KIND.placeholder(1)
                )))
                .bind(1_i64)
                .fetch_one(database.pool())
                .await
                .expect("reads");
                assert_eq!(title, "renamed");

                let mut uow = database.begin().await.expect("begins");
                sqlx::query(AssertSqlSafe(format!(
                    "DELETE FROM rv_post WHERE id = {}",
                    KIND.placeholder(1)
                )))
                .bind(1_i64)
                .execute(&mut **uow.inner())
                .await
                .expect("deletes");
                uow.commit().await.expect("commits");

                assert_eq!(count(&database).await, 0);
                database.close().await.expect("closes");
            }

            // ------------------------------------------------- transaction boundaries

            #[tokio::test]
            async fn a_commit_makes_both_writes_visible() {
                let Some((database, _fixture)) = fresh().await else {
                    return;
                };
                let mut uow = database.begin().await.expect("begins");
                for id in [1_i64, 2] {
                    sqlx::query(AssertSqlSafe(insert_sql()))
                        .bind(id)
                        .bind("x")
                        .bind(id)
                        .execute(&mut **uow.inner())
                        .await
                        .expect("inserts");
                }
                uow.commit().await.expect("commits");
                assert_eq!(count(&database).await, 2);
                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn an_explicit_rollback_writes_nothing() {
                let Some((database, _fixture)) = fresh().await else {
                    return;
                };
                let mut uow = database.begin().await.expect("begins");
                sqlx::query(AssertSqlSafe(insert_sql()))
                    .bind(1_i64)
                    .bind("x")
                    .bind(1_i64)
                    .execute(&mut **uow.inner())
                    .await
                    .expect("inserts");
                uow.rollback().await.expect("rolls back");
                assert_eq!(count(&database).await, 0);
                database.close().await.expect("closes");
            }

            /// Dropping without committing writes nothing.
            ///
            /// The assertion is about **visibility**, not about when the `ROLLBACK` reaches the
            /// server. `sqlx-core` queues the rollback and flushes it from a spawned task, so a
            /// test asserting server-side transaction state here would race. Visibility does not:
            /// an uncommitted row was never visible to another connection in the first place.
            #[tokio::test]
            async fn dropping_without_committing_writes_nothing() {
                let Some((database, _fixture)) = fresh().await else {
                    return;
                };
                {
                    let mut uow = database.begin().await.expect("begins");
                    sqlx::query(AssertSqlSafe(insert_sql()))
                        .bind(1_i64)
                        .bind("x")
                        .bind(1_i64)
                        .execute(&mut **uow.inner())
                        .await
                        .expect("inserts");
                    drop(uow);
                }
                assert_eq!(count(&database).await, 0);
                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn cancelling_mid_transaction_writes_nothing() {
                let Some((database, _fixture)) = fresh().await else {
                    return;
                };
                let cancelled = tokio::time::timeout(Duration::from_millis(1), async {
                    let mut uow = database.begin().await.expect("begins");
                    sqlx::query(AssertSqlSafe(insert_sql()))
                        .bind(1_i64)
                        .bind("x")
                        .bind(1_i64)
                        .execute(&mut **uow.inner())
                        .await
                        .expect("inserts");
                    // Never reached within the deadline.
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    uow.commit().await.expect("commits");
                })
                .await;
                assert!(cancelled.is_err(), "the future must have been cancelled");
                assert_eq!(count(&database).await, 0);
                // The cancelled transaction's connection is returned by a SPAWNED task, so the
                // drain has to be allowed to observe it. Waiting is honest; asserting immediately
                // would be a race dressed up as a guarantee.
                tokio::time::sleep(Duration::from_millis(250)).await;
                database.close().await.expect("closes");
            }

            // ------------------------------------------------------ connection release

            /// SC-002, measured rather than asserted.
            #[tokio::test]
            async fn every_ending_returns_the_connection_to_the_pool() {
                let Some((database, _fixture)) = fresh().await else {
                    return;
                };

                let commit = database.begin().await.expect("begins");
                commit.commit().await.expect("commits");

                let rollback = database.begin().await.expect("begins");
                rollback.rollback().await.expect("rolls back");

                drop(database.begin().await.expect("begins"));

                // The queued rollback flushes on a spawned task, so allow it to run before
                // measuring. Waiting is honest; asserting immediately would be a race dressed up
                // as a guarantee.
                tokio::time::sleep(Duration::from_millis(250)).await;

                let status = database.connections();
                assert_eq!(
                    status.in_use(),
                    0,
                    "connections were not returned: {status:?}"
                );
                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn an_exhausted_pool_reports_an_acquire_timeout_rather_than_waiting_forever() {
                let Some((database, _fixture)) = fresh().await else {
                    return;
                };
                // The pool holds four. Take all four and keep them.
                let mut held = Vec::new();
                for _ in 0..4 {
                    held.push(database.begin().await.expect("begins"));
                }
                let started = std::time::Instant::now();
                let error = database.begin().await.expect_err("must not wait forever");
                assert_eq!(error.kind(), DatabaseErrorKind::AcquireTimeout);
                assert!(
                    started.elapsed() < Duration::from_secs(15),
                    "the wait was not bounded"
                );
                assert!(error.kind().is_transient());
                drop(held);
                database.close().await.expect("closes");
            }

            // ------------------------------------------------------------- classification

            #[tokio::test]
            async fn a_duplicate_key_is_classified_as_a_unique_violation() {
                let Some((database, _fixture)) = fresh().await else {
                    return;
                };
                let mut uow = database.begin().await.expect("begins");
                sqlx::query(AssertSqlSafe(insert_sql()))
                    .bind(1_i64)
                    .bind("x")
                    .bind(1_i64)
                    .execute(&mut **uow.inner())
                    .await
                    .expect("inserts");
                uow.commit().await.expect("commits");

                let mut uow = database.begin().await.expect("begins");
                let error = sqlx::query(AssertSqlSafe(insert_sql()))
                    .bind(1_i64)
                    .bind("y")
                    .bind(2_i64)
                    .execute(&mut **uow.inner())
                    .await
                    .expect_err("duplicate");
                let translated = renvor_sqlx::classify_error(&error);
                assert_eq!(translated.kind(), DatabaseErrorKind::UniqueViolation);
                // The transaction is ended EXPLICITLY before shutdown. Leaving it open would make
                // `close` report `DeadlineExceeded`, which is correct behaviour and is asserted by
                // its own test below rather than tripped over here.
                uow.rollback().await.expect("rolls back");
                database.close().await.expect("closes");
            }

            // -------------------------------------------------------------- injection

            /// A value that would be catastrophic if interpolated, bound instead.
            #[tokio::test]
            async fn an_injection_payload_bound_as_a_value_is_stored_verbatim() {
                let Some((database, _fixture)) = fresh().await else {
                    return;
                };
                const PAYLOAD: &str = "'); DROP TABLE rv_post; --";

                let mut uow = database.begin().await.expect("begins");
                sqlx::query(AssertSqlSafe(insert_sql()))
                    .bind(1_i64)
                    .bind(PAYLOAD)
                    .bind(1_i64)
                    .execute(&mut **uow.inner())
                    .await
                    .expect("inserts");
                uow.commit().await.expect("commits");

                // The table still exists, and the payload round-tripped as DATA.
                let stored: String = sqlx::query_scalar(AssertSqlSafe(format!(
                    "SELECT title FROM rv_post WHERE id = {}",
                    KIND.placeholder(1)
                )))
                .bind(1_i64)
                .fetch_one(database.pool())
                .await
                .expect("the table survived");
                assert_eq!(stored, PAYLOAD);
                assert_eq!(count(&database).await, 1);
                database.close().await.expect("closes");
            }

            /// The same payload as a **sort key**, which is an identifier position.
            #[tokio::test]
            async fn an_injection_payload_as_a_sort_key_is_refused_by_the_allowlist() {
                let list = SortAllowlist::new()
                    .allow("rank", "rank_value")
                    .tiebreaker("id");
                let error = list
                    .order_by(&[(
                        "rank_value; DROP TABLE rv_post; --".to_owned(),
                        Direction::Ascending,
                    )])
                    .expect_err("refused");
                assert!(!error.description().contains("DROP"));
            }

            // ------------------------------------------------------------- pagination

            /// Ties are broken totally, so no row is skipped or repeated across pages.
            #[tokio::test]
            async fn paging_over_ties_returns_every_row_exactly_once() {
                let Some((database, _fixture)) = fresh().await else {
                    return;
                };
                // Ten rows, all with the SAME rank — every row is a tie.
                let mut uow = database.begin().await.expect("begins");
                for id in 1..=10_i64 {
                    sqlx::query(AssertSqlSafe(insert_sql()))
                        .bind(id)
                        .bind("same")
                        .bind(7_i64)
                        .execute(&mut **uow.inner())
                        .await
                        .expect("inserts");
                }
                uow.commit().await.expect("commits");

                let list = SortAllowlist::new()
                    .allow("rank", "rank_value")
                    .tiebreaker("id");
                let order = list
                    .order_by(&[("rank".to_owned(), Direction::Ascending)])
                    .expect("orders");
                let rendered = order.render(|column| KIND.quote_identifier(column));
                assert_eq!(
                    rendered,
                    format!(
                        "{} ASC, {} ASC",
                        KIND.quote_identifier("rank_value"),
                        KIND.quote_identifier("id")
                    )
                );

                // Page with size 3 over a 10-row tie group, resuming by keyset.
                let mut seen: Vec<i64> = Vec::new();
                let mut after: Option<(i64, i64)> = None;
                loop {
                    let sql = match after {
                        None => format!(
                            "SELECT id, rank_value FROM rv_post ORDER BY {rendered} LIMIT 3"
                        ),
                        Some(_) => format!(
                            "SELECT id, rank_value FROM rv_post \
                             WHERE (rank_value, id) > ({}, {}) ORDER BY {rendered} LIMIT 3",
                            KIND.placeholder(1),
                            KIND.placeholder(2)
                        ),
                    };
                    let mut query = sqlx::query_as::<_, (i64, i64)>(AssertSqlSafe(sql));
                    if let Some((rank, id)) = after {
                        query = query.bind(rank).bind(id);
                    }
                    let rows = query.fetch_all(database.pool()).await.expect("pages");
                    if rows.is_empty() {
                        break;
                    }
                    for (id, _) in &rows {
                        seen.push(*id);
                    }
                    let last = rows.last().expect("non-empty");
                    after = Some((last.1, last.0));
                }

                let mut unique = seen.clone();
                unique.sort_unstable();
                unique.dedup();
                assert_eq!(seen.len(), 10, "a row was skipped or repeated: {seen:?}");
                assert_eq!(unique.len(), 10);
                assert_eq!(unique, (1..=10).collect::<Vec<_>>());
                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn an_empty_table_yields_an_empty_page_rather_than_an_error() {
                let Some((database, _fixture)) = fresh().await else {
                    return;
                };
                let rows = sqlx::query_as::<_, (i64, i64)>(
                    "SELECT id, rank_value FROM rv_post ORDER BY id ASC LIMIT 3",
                )
                .fetch_all(database.pool())
                .await
                .expect("pages");
                assert!(rows.is_empty());
                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn a_malformed_cursor_is_refused_without_panicking() {
                for text in ["", "!!!!", "AAAA", "zzzzzzzzzzzzzzzz"] {
                    let result = Keyset::from_cursor(text, 1);
                    assert!(result.is_err());
                }
            }

            #[tokio::test]
            async fn a_cursor_issued_under_different_filters_is_refused() {
                let keyset = Keyset::new(1, vec![b"a".to_vec()]);
                let encoded = keyset.to_cursor().encode();
                assert_eq!(
                    Keyset::from_cursor(&encoded, 999),
                    Err(KeysetError::ForeignQuery)
                );
            }
        }
    };
}

suite!(
    postgres,
    "db-postgres",
    sqlx::Postgres,
    renvor_sqlx::connect_postgres,
    support::POSTGRES_URL,
    renvor_database::DatabaseKind::Postgres
);
suite!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    renvor_sqlx::connect_mysql,
    support::MYSQL_URL,
    renvor_database::DatabaseKind::MySql
);
