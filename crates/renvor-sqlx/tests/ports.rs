//! The ports hold: one repository implementation, two execution contexts, and a test double.
//!
//! # What FR-001 and FR-008 actually require
//!
//! Not that a `Repository<T>` trait exists — Renvor deliberately publishes none, because a blanket
//! `find`/`insert`/`update`/`delete` trait is an ORM boundary. What they require is that an
//! application can write **one** repository against a narrow capability and run it both inside a
//! transaction and outside one, and that a service generic over the port can be tested with a
//! double that names no driver.
//!
//! Both are asserted here rather than described.

mod support;

/// A test double for [`renvor_database::Executor`], naming no driver.
///
/// This compiling at all is the FR-001 evidence: the port hands out no statement type, no row type
/// and no connection, so a double needs none either. If the port ever leaked a driver handle, this
/// type would stop compiling without `sqlx` — which is exactly the drift worth catching.
struct FakeExecutor {
    kind: renvor_database::DatabaseKind,
    in_transaction: bool,
}

impl renvor_database::Executor for FakeExecutor {
    fn kind(&self) -> renvor_database::DatabaseKind {
        self.kind
    }

    fn in_transaction(&self) -> bool {
        self.in_transaction
    }
}

/// An application service generic over the port, with no driver anywhere in its signature.
fn requires_a_transaction<E: renvor_database::Executor>(
    executor: &E,
) -> Result<&'static str, &'static str> {
    if executor.in_transaction() {
        Ok(executor.kind().as_str())
    } else {
        Err("this operation requires an explicit transaction")
    }
}

#[test]
fn a_service_generic_over_the_port_needs_no_database_to_test() {
    let inside = FakeExecutor {
        kind: renvor_database::DatabaseKind::Postgres,
        in_transaction: true,
    };
    let outside = FakeExecutor {
        kind: renvor_database::DatabaseKind::MySql,
        in_transaction: false,
    };
    assert_eq!(requires_a_transaction(&inside), Ok("postgres"));
    assert!(requires_a_transaction(&outside).is_err());
}

macro_rules! suite {
    ($module:ident, $feature:literal, $driver:ty, $alias:ty, $connect:path, $url:expr, $insert:literal, $session:literal) => {
        #[cfg(feature = $feature)]
        mod $module {
            use super::*;
            use renvor_database::{Database, Executor as _, UnitOfWork};
            use renvor_sqlx::SqlxExecutor;
            use sqlx::AssertSqlSafe;

            /// The application's own repository, written ONCE.
            ///
            /// It is generic over [`SqlxExecutor`], so the same body serves a pooled connection and
            /// an open transaction. Note what it never mentions: a pool, a transaction, or how the
            /// connection it runs on was obtained.
            async fn insert_widget<E>(executor: &mut E, id: i64) -> Result<(), sqlx::Error>
            where
                E: SqlxExecutor<Database = $driver>,
            {
                sqlx::query(AssertSqlSafe($insert.to_owned()))
                    .bind(id)
                    .execute(executor.connection())
                    .await
                    .map(|_| ())
            }

            /// A clean fixture, and the guard that keeps it clean for the test's duration.
            ///
            /// Every test in this file shares one table, which this drops or clears. Two running at
            /// once therefore delete each other's rows, and the suite passed only under
            /// `--test-threads=1` — a requirement nothing in the code stated, and one an ordinary
            /// `cargo test` breaks with assertion failures that look like flakiness. Returning the
            /// guard puts the requirement in the type. See [`support::SHARED_FIXTURE`].
            async fn database() -> Option<($alias, tokio::sync::MutexGuard<'static, ()>)> {
                let guard = support::SHARED_FIXTURE.lock().await;
                let dsn = support::url($url)?;
                let database = $connect(&dsn, &support::settings())
                    .await
                    .expect("connects");
                sqlx::query(AssertSqlSafe(
                    "CREATE TABLE IF NOT EXISTS rv_ports (id BIGINT PRIMARY KEY)".to_owned(),
                ))
                .execute(database.pool())
                .await
                .expect("creates");
                sqlx::query(AssertSqlSafe("DELETE FROM rv_ports".to_owned()))
                    .execute(database.pool())
                    .await
                    .expect("clears");
                Some((database, guard))
            }

            #[tokio::test]
            async fn one_repository_runs_both_inside_and_outside_a_transaction() {
                let Some((database, _fixture)) = database().await else {
                    return;
                };

                // Outside a transaction, on a pooled connection.
                let mut connection = database.acquire().await.expect("acquires");
                assert!(!connection.in_transaction());
                insert_widget(&mut connection, 1).await.expect("inserts");
                drop(connection);

                // Inside one, through the same function.
                let mut uow = database.begin().await.expect("begins");
                assert!(uow.in_transaction());
                insert_widget(&mut uow, 2).await.expect("inserts");
                uow.commit().await.expect("commits");

                let total: i64 =
                    sqlx::query_scalar(AssertSqlSafe("SELECT COUNT(*) FROM rv_ports".to_owned()))
                        .fetch_one(database.pool())
                        .await
                        .expect("counts");
                assert_eq!(total, 2, "both execution contexts must have written");
            }

            /// A rolled-back transaction writes nothing, through that same repository.
            #[tokio::test]
            async fn a_rolled_back_transaction_leaves_no_row() {
                let Some((database, _fixture)) = database().await else {
                    return;
                };
                let mut uow = database.begin().await.expect("begins");
                insert_widget(&mut uow, 99).await.expect("inserts");
                uow.rollback().await.expect("rolls back");

                let total: i64 =
                    sqlx::query_scalar(AssertSqlSafe("SELECT COUNT(*) FROM rv_ports".to_owned()))
                        .fetch_one(database.pool())
                        .await
                        .expect("counts");
                assert_eq!(total, 0);
            }

            /// A second `begin` is a SEPARATE transaction, never a nested one.
            ///
            /// Asserted two ways, because either alone would be weak: the two units of work sit on
            /// different server sessions, and one's uncommitted write is invisible to the other. A
            /// nested begin flattened into the outer transaction would fail the second assertion.
            #[tokio::test]
            async fn a_second_begin_is_separate_rather_than_nested() {
                let Some((database, _fixture)) = database().await else {
                    return;
                };
                let mut outer = database.begin().await.expect("begins");
                let outer_session: String = sqlx::query_scalar(AssertSqlSafe($session.to_owned()))
                    .fetch_one(outer.connection())
                    .await
                    .expect("reads session");
                insert_widget(&mut outer, 7).await.expect("inserts");

                let mut inner = database.begin().await.expect("begins again");
                let inner_session: String = sqlx::query_scalar(AssertSqlSafe($session.to_owned()))
                    .fetch_one(inner.connection())
                    .await
                    .expect("reads session");
                let seen: i64 =
                    sqlx::query_scalar(AssertSqlSafe("SELECT COUNT(*) FROM rv_ports".to_owned()))
                        .fetch_one(inner.connection())
                        .await
                        .expect("counts");

                assert_ne!(
                    outer_session, inner_session,
                    "a second begin must take a different connection"
                );
                assert_eq!(
                    seen, 0,
                    "the outer transaction's uncommitted write must be invisible to the second"
                );

                let _ = inner.rollback().await;
                let _ = outer.rollback().await;
            }

            /// Both executors report the kind they were built for.
            #[tokio::test]
            async fn every_executor_reports_its_kind() {
                let Some((database, _fixture)) = database().await else {
                    return;
                };
                let connection = database.acquire().await.expect("acquires");
                let uow = database.begin().await.expect("begins");
                assert_eq!(connection.kind(), database.kind());
                assert_eq!(uow.kind(), database.kind());
                let _ = uow.rollback().await;
            }
        }
    };
}

suite!(
    postgres,
    "db-postgres",
    sqlx::Postgres,
    renvor_sqlx::PostgresDatabase,
    renvor_sqlx::connect_postgres,
    support::POSTGRES_URL,
    "INSERT INTO rv_ports (id) VALUES ($1)",
    "SELECT pg_backend_pid()::text"
);

suite!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    renvor_sqlx::MySqlDatabase,
    renvor_sqlx::connect_mysql,
    support::MYSQL_URL,
    "INSERT INTO rv_ports (id) VALUES (?)",
    "SELECT CAST(CONNECTION_ID() AS CHAR)"
);
