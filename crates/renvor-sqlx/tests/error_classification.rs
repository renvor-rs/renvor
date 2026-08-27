//! Every constraint violation this adapter claims to distinguish, provoked against a real server.
//!
//! # Why these are real-database tests and not unit tests
//!
//! The classification under test reads `sqlx::error::DatabaseError::kind()`, which each driver
//! populates from the **server's own** error number. A unit test would have to fabricate a driver
//! error object, and the value it fabricated would be the test author's belief about what the
//! server sends rather than what the server sends. That belief is the one thing these tests exist
//! to check.
//!
//! # SQLSTATE is not the key, and that was measured
//!
//! PostgreSQL gives each condition its own SQLSTATE inside class 23 — `23505`, `23503`, `23502`,
//! `23514`. MySQL collapses unique, foreign-key and not-null onto the single generic `23000`, and
//! reports a check violation as error `3819` with SQLSTATE **`HY000`**, outside the integrity class
//! altogether. A classifier keyed on SQLSTATE would be unable to tell three of these apart on
//! MySQL and would miss the fourth completely, which is why classification reads the driver's error
//! *number* by way of `kind()`.

mod support;

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use std::time::Duration;

/// One engine's rows, parameterised by the SQL that engine actually accepts.
///
/// The DDL genuinely differs: MySQL ignores an inline `REFERENCES` clause in a column definition
/// and needs a table-level `FOREIGN KEY`, so one portable schema string would silently create a
/// table with **no foreign key** on MySQL — and its "foreign-key violation" test would then pass by
/// accident while measuring nothing.
macro_rules! engine {
    (
        $module:ident,
        $feature:literal,
        $alias:ty,
        $driver:ty,
        $connect:path,
        $url:expr,
        $child_ddl:expr,
        $dup:expr,
        $bad_fk:expr,
        $null_name:expr,
        $bad_check:expr
    ) => {
        #[cfg(feature = $feature)]
        mod $module {
            use super::*;
            use renvor_database::{DatabaseErrorKind, PoolSettings};
            use sqlx::AssertSqlSafe;

            async fn fixture() -> Option<($alias, tokio::sync::MutexGuard<'static, ()>)> {
                let guard = support::SHARED_FIXTURE.lock().await;
                let dsn = support::url($url)?;
                let settings = PoolSettings::default()
                    .with_max_connections(4)
                    .expect("bounded")
                    .with_acquire_timeout(Duration::from_secs(5))
                    .expect("bounded");
                let database = $connect(&dsn, &settings).await.expect("connects");

                for statement in [
                    "DROP TABLE IF EXISTS rv_ec_child",
                    "DROP TABLE IF EXISTS rv_ec_parent",
                    "CREATE TABLE rv_ec_parent (id BIGINT PRIMARY KEY)",
                    $child_ddl,
                    "INSERT INTO rv_ec_parent (id) VALUES (1)",
                ] {
                    sqlx::query(AssertSqlSafe(statement.to_owned()))
                        .execute(database.pool())
                        .await
                        .unwrap_or_else(|error| panic!("fixture statement failed: {error}"));
                }
                // The row every violation below collides with, references, or contrasts against.
                sqlx::query(AssertSqlSafe(
                    "INSERT INTO rv_ec_child (id, parent_id, name, qty) VALUES (1, 1, 'seed', 5)"
                        .to_owned(),
                ))
                .execute(database.pool())
                .await
                .expect("seeds");

                Some((database, guard))
            }

            /// Runs a statement that MUST be refused, and returns the classified kind.
            async fn refused(database: &$alias, sql: &str) -> DatabaseErrorKind {
                let error = sqlx::query(AssertSqlSafe(sql.to_owned()))
                    .execute(database.pool())
                    .await
                    .expect_err("the statement must be refused, or this test measures nothing");
                renvor_sqlx::error::classify_error(&error).kind()
            }

            /// The four constraint violations, each distinguished from the others.
            ///
            /// One test rather than four, because the interesting failure is a mapping that
            /// collapses several conditions onto one kind — and that is only visible when the
            /// results are compared with each other.
            #[tokio::test]
            async fn each_constraint_violation_is_classified_as_itself() {
                let Some((database, _guard)) = fixture().await else {
                    return;
                };

                assert_eq!(
                    refused(&database, $dup).await,
                    DatabaseErrorKind::UniqueViolation,
                    "a duplicate key must classify as a unique violation"
                );
                assert_eq!(
                    refused(&database, $bad_fk).await,
                    DatabaseErrorKind::ForeignKeyViolation,
                    "an absent parent must classify as a foreign-key violation"
                );
                assert_eq!(
                    refused(&database, $null_name).await,
                    DatabaseErrorKind::NotNullViolation,
                    "a NULL in a NOT NULL column must classify as a not-null violation, not as a \
                     generic rejection: both drivers already recover this from the server's error \
                     number"
                );
                assert_eq!(
                    refused(&database, $bad_check).await,
                    DatabaseErrorKind::CheckViolation,
                    "a failed CHECK must classify as a check violation. On MySQL this arrives as \
                     error 3819 with SQLSTATE HY000 — outside the integrity class — so a \
                     SQLSTATE-keyed classifier would miss it entirely"
                );
            }

            /// The four kinds are genuinely distinct, not four names for one outcome.
            ///
            /// A CONTROL for the test above. Without it, a mapping that returned the same kind for
            /// everything would still have to be wrong four times in exactly the right way to fail
            /// — but a mapping that returned `UniqueViolation` for all four would pass one
            /// assertion and fail three, which is a weaker signal than stating the property
            /// directly.
            #[tokio::test]
            async fn the_four_violations_do_not_collapse_onto_one_kind() {
                let Some((database, _guard)) = fixture().await else {
                    return;
                };

                let mut kinds = vec![
                    refused(&database, $dup).await,
                    refused(&database, $bad_fk).await,
                    refused(&database, $null_name).await,
                    refused(&database, $bad_check).await,
                ];
                let observed = kinds.len();
                kinds.sort_unstable();
                kinds.dedup();
                assert_eq!(
                    kinds.len(),
                    observed,
                    "the four conditions produced {} distinct kinds instead of {observed}: {:?}. \
                     Distinguishing them is the entire point of the mapping",
                    kinds.len(),
                    kinds
                );
            }

            /// A transaction that loses a concurrency conflict is retryable, not rejected.
            ///
            /// # No sleeps: the interleaving is forced with a barrier
            ///
            /// A deadlock needs two sessions to take the same two locks in opposite orders. Timing
            /// that with sleeps would make the test a race that usually works. Two
            /// [`tokio::sync::Barrier`] rendezvous make it deterministic: each session takes its
            /// first lock, both wait until BOTH first locks are held, and only then does each
            /// reach for the other's. The cycle exists before either second statement runs.
            ///
            /// Either session may be chosen as the victim — that is the server's decision, not
            /// ours — so the assertion is on the pair, not on a particular task.
            #[tokio::test]
            async fn a_lost_conflict_is_retryable_rather_than_a_rejection() {
                use std::sync::Arc;

                let Some((database, _guard)) = fixture().await else {
                    return;
                };
                for statement in [
                    "DROP TABLE IF EXISTS rv_ec_lock",
                    "CREATE TABLE rv_ec_lock (id BIGINT PRIMARY KEY, v BIGINT NOT NULL)",
                    "INSERT INTO rv_ec_lock (id, v) VALUES (1, 0), (2, 0)",
                ] {
                    sqlx::query(AssertSqlSafe(statement.to_owned()))
                        .execute(database.pool())
                        .await
                        .expect("lock fixture");
                }

                let gate = Arc::new(tokio::sync::Barrier::new(2));
                let pool = database.pool().clone();

                async fn cross(
                    pool: sqlx::Pool<$driver>,
                    gate: Arc<tokio::sync::Barrier>,
                    first: i64,
                    second: i64,
                ) -> Result<(), sqlx::Error> {
                    let mut tx = pool.begin().await?;
                    sqlx::query(AssertSqlSafe(format!(
                        "UPDATE rv_ec_lock SET v = v + 1 WHERE id = {first}"
                    )))
                    .execute(&mut *tx)
                    .await?;
                    // BOTH first locks are held before EITHER second statement runs.
                    gate.wait().await;
                    sqlx::query(AssertSqlSafe(format!(
                        "UPDATE rv_ec_lock SET v = v + 1 WHERE id = {second}"
                    )))
                    .execute(&mut *tx)
                    .await?;
                    tx.commit().await
                }

                let a = tokio::spawn(cross(pool.clone(), Arc::clone(&gate), 1, 2));
                let b = tokio::spawn(cross(pool, gate, 2, 1));
                let (ra, rb) = (a.await.expect("task a"), b.await.expect("task b"));

                let kinds: Vec<_> = [ra, rb]
                    .into_iter()
                    .filter_map(|r| r.err())
                    .map(|e| renvor_sqlx::error::classify_error(&e).kind())
                    .collect();

                assert_eq!(
                    kinds.len(),
                    1,
                    "exactly one session must lose the conflict; observed {kinds:?}"
                );
                assert_eq!(
                    kinds[0],
                    DatabaseErrorKind::TransactionConflict,
                    "a lost conflict must classify as a retryable conflict rather than a generic                      rejection: the server itself says `try restarting transaction`"
                );
                assert!(
                    kinds[0].is_transient(),
                    "a lost conflict must report as transient, because retrying it unchanged is                      exactly the correct response"
                );
            }

            /// No classified kind carries the server's text.
            ///
            /// The server's message for these violations contains the offending value, the
            /// constraint name, and the table — `'seed'`, `rv_ec_child`, and the constraint
            /// identifier all appear in it. None may survive translation.
            #[tokio::test]
            async fn a_violation_never_carries_the_server_text() {
                let Some((database, _guard)) = fixture().await else {
                    return;
                };

                for sql in [$dup, $bad_fk, $null_name, $bad_check] {
                    let error = sqlx::query(AssertSqlSafe(sql.to_owned()))
                        .execute(database.pool())
                        .await
                        .expect_err("refused");
                    let translated = renvor_sqlx::error::classify_error(&error);
                    let rendered = format!("{translated} {translated:?}");
                    for leak in ["rv_ec_child", "rv_ec_parent", "seed", "qty"] {
                        assert!(
                            !rendered.contains(leak),
                            "the translated error leaked `{leak}`: {rendered}"
                        );
                    }
                }
            }
        }
    };
}

engine!(
    postgres,
    "db-postgres",
    renvor_sqlx::PostgresDatabase,
    sqlx::Postgres,
    renvor_sqlx::connect_postgres,
    support::POSTGRES_URL,
    "CREATE TABLE rv_ec_child (\
       id BIGINT PRIMARY KEY, \
       parent_id BIGINT REFERENCES rv_ec_parent(id), \
       name VARCHAR(50) NOT NULL UNIQUE, \
       qty INT CHECK (qty > 0))",
    "INSERT INTO rv_ec_child (id, parent_id, name, qty) VALUES (2, 1, 'seed', 5)",
    "INSERT INTO rv_ec_child (id, parent_id, name, qty) VALUES (3, 999, 'b', 5)",
    "INSERT INTO rv_ec_child (id, parent_id, name, qty) VALUES (4, 1, NULL, 5)",
    "INSERT INTO rv_ec_child (id, parent_id, name, qty) VALUES (5, 1, 'c', 0)"
);

engine!(
    mysql,
    "db-mysql",
    renvor_sqlx::MySqlDatabase,
    sqlx::MySql,
    renvor_sqlx::connect_mysql,
    support::MYSQL_URL,
    "CREATE TABLE rv_ec_child (\
       id BIGINT PRIMARY KEY, \
       parent_id BIGINT, \
       name VARCHAR(50) NOT NULL UNIQUE, \
       qty INT, \
       CONSTRAINT rv_ec_fk FOREIGN KEY (parent_id) REFERENCES rv_ec_parent(id), \
       CONSTRAINT rv_ec_chk CHECK (qty > 0)) ENGINE=InnoDB",
    "INSERT INTO rv_ec_child (id, parent_id, name, qty) VALUES (2, 1, 'seed', 5)",
    "INSERT INTO rv_ec_child (id, parent_id, name, qty) VALUES (3, 999, 'b', 5)",
    "INSERT INTO rv_ec_child (id, parent_id, name, qty) VALUES (4, 1, NULL, 5)",
    "INSERT INTO rv_ec_child (id, parent_id, name, qty) VALUES (5, 1, 'c', 0)"
);
