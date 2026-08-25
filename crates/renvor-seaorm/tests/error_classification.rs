//! The SeaORM rows of the error-classification matrix.
//!
//! # Why this is not the same file as the direct-SQLx one
//!
//! The two adapters translate **different vocabularies**. `renvor-sqlx` classifies a
//! `sqlx::Error`; this one classifies a [`sea_orm::DbErr`], whose `sql_err()` classifier runs first
//! and recognises only unique and foreign-key violations. A not-null or check violation therefore
//! reaches the driver-level mapping by a different route on this side, and the whole point of these
//! tests is that it arrives at the **same kind** regardless.
//!
//! An application that swaps `renvor-sqlx` for `renvor-seaorm` must not have to rewrite its error
//! handling. These tests are what makes that checkable rather than asserted.

mod support;

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use std::time::Duration;

macro_rules! engine {
    (
        $module:ident,
        $feature:literal,
        $alias:ty,
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
            use renvor_seaorm::error::classify_db_error;
            use sea_orm::ConnectionTrait as _;
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
                    "DROP TABLE IF EXISTS rv_sea_ec_child",
                    "DROP TABLE IF EXISTS rv_sea_ec_parent",
                    "CREATE TABLE rv_sea_ec_parent (id BIGINT PRIMARY KEY)",
                    $child_ddl,
                    "INSERT INTO rv_sea_ec_parent (id) VALUES (1)",
                    "INSERT INTO rv_sea_ec_child (id, parent_id, name, qty) \
                     VALUES (1, 1, 'seed', 5)",
                ] {
                    sqlx::query(AssertSqlSafe(statement.to_owned()))
                        .execute(database.pool())
                        .await
                        .unwrap_or_else(|error| panic!("fixture statement failed: {error}"));
                }
                Some((database, guard))
            }

            /// Runs a statement through SeaORM that MUST be refused, and classifies the `DbErr`.
            ///
            /// Deliberately the idiomatic SeaORM path — `Statement` executed through
            /// `ConnectionTrait` — so the error travels the route an application's would, including
            /// `DbErr::sql_err()`'s first pass.
            async fn refused(database: &$alias, sql: &str) -> DatabaseErrorKind {
                let connection = database.acquire().await.expect("acquires");
                let statement = sea_orm::Statement::from_string(
                    connection.get_database_backend(),
                    sql.to_owned(),
                );
                let error = connection
                    .execute_raw(statement)
                    .await
                    .expect_err("the statement must be refused, or this test measures nothing");
                classify_db_error(&error).kind()
            }

            /// The four constraint violations, each classified as itself.
            ///
            /// The kinds asserted here are **the same constants** the direct-SQLx rows assert in
            /// `renvor-sqlx/tests/error_classification.rs`. That is the parity claim: not that both
            /// adapters classify sensibly, but that they classify identically.
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
                    "a NULL in a NOT NULL column must classify as a not-null violation. \
                     `SqlErr` has no variant for this, so it must reach the driver-level mapping \
                     through `RuntimeErr::SqlxError` and land where `renvor-sqlx` lands"
                );
                assert_eq!(
                    refused(&database, $bad_check).await,
                    DatabaseErrorKind::CheckViolation,
                    "a failed CHECK must classify as a check violation, by the same route"
                );
            }

            /// The four kinds are genuinely distinct.
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
                    "the four conditions produced {} distinct kinds instead of {observed}: {:?}",
                    kinds.len(),
                    kinds
                );
            }

            /// No classified kind carries SeaORM's text.
            ///
            /// SeaORM's message is **worse** than the driver's for this purpose: it routinely
            /// contains the generated SQL as well as the value and table. None may survive.
            #[tokio::test]
            async fn a_violation_never_carries_the_seaorm_text() {
                let Some((database, _guard)) = fixture().await else {
                    return;
                };
                for sql in [$dup, $bad_fk, $null_name, $bad_check] {
                    let connection = database.acquire().await.expect("acquires");
                    let statement = sea_orm::Statement::from_string(
                        connection.get_database_backend(),
                        sql.to_owned(),
                    );
                    let error = connection.execute_raw(statement).await.expect_err("refused");
                    let translated = classify_db_error(&error);
                    let rendered = format!("{translated} {translated:?}");
                    for leak in ["rv_sea_ec_child", "rv_sea_ec_parent", "seed", "INSERT"] {
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
    renvor_seaorm::PostgresDatabase,
    renvor_seaorm::connect_postgres,
    support::POSTGRES_URL,
    "CREATE TABLE rv_sea_ec_child (\
       id BIGINT PRIMARY KEY, \
       parent_id BIGINT REFERENCES rv_sea_ec_parent(id), \
       name VARCHAR(50) NOT NULL UNIQUE, \
       qty INT CHECK (qty > 0))",
    "INSERT INTO rv_sea_ec_child (id, parent_id, name, qty) VALUES (2, 1, 'seed', 5)",
    "INSERT INTO rv_sea_ec_child (id, parent_id, name, qty) VALUES (3, 999, 'b', 5)",
    "INSERT INTO rv_sea_ec_child (id, parent_id, name, qty) VALUES (4, 1, NULL, 5)",
    "INSERT INTO rv_sea_ec_child (id, parent_id, name, qty) VALUES (5, 1, 'c', 0)"
);

engine!(
    mysql,
    "db-mysql",
    renvor_seaorm::MySqlDatabase,
    renvor_seaorm::connect_mysql,
    support::MYSQL_URL,
    "CREATE TABLE rv_sea_ec_child (\
       id BIGINT PRIMARY KEY, \
       parent_id BIGINT, \
       name VARCHAR(50) NOT NULL UNIQUE, \
       qty INT, \
       CONSTRAINT rv_sea_ec_fk FOREIGN KEY (parent_id) REFERENCES rv_sea_ec_parent(id), \
       CONSTRAINT rv_sea_ec_chk CHECK (qty > 0)) ENGINE=InnoDB",
    "INSERT INTO rv_sea_ec_child (id, parent_id, name, qty) VALUES (2, 1, 'seed', 5)",
    "INSERT INTO rv_sea_ec_child (id, parent_id, name, qty) VALUES (3, 999, 'b', 5)",
    "INSERT INTO rv_sea_ec_child (id, parent_id, name, qty) VALUES (4, 1, NULL, 5)",
    "INSERT INTO rv_sea_ec_child (id, parent_id, name, qty) VALUES (5, 1, 'c', 0)"
);
