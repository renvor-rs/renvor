//! The SeaORM rows of the four-row contract matrix.
//!
//! Every assertion in here lives in [`renvor_testkit::persistence`] and is compiled once. The
//! direct-SQLx rows call the same functions from `renvor-sqlx/tests/shared_contract.rs`, so
//! *"both SeaORM rows pass the same application contracts as direct SQLx"* is a fact about the
//! build rather than two suites that happen to agree today.
//!
//! What this file contributes is the four driver-specific operations the harness cannot supply:
//! how to insert, how to count from outside the transaction, and how to reset.

mod support;

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use std::time::Duration;

/// Small enough that a lost slot is unmistakable, and the number the harness is told about.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
const CAPACITY: u32 = 3;

macro_rules! row {
    ($module:ident, $feature:literal, $alias:ty, $connect:path, $url:expr, $kind:expr, $table:literal, $insert:literal) => {
        #[cfg(feature = $feature)]
        mod $module {
            use super::*;
            use renvor_database::{DatabaseError, PoolSettings};
            use renvor_seaorm::error::classify_db_error;
            use renvor_testkit::persistence::{self, PersistenceFixture};
            use sea_orm::ConnectionTrait as _;
            use sqlx::AssertSqlSafe;

            struct Fixture {
                database: $alias,
            }

            impl PersistenceFixture for Fixture {
                type Database = $alias;

                fn database(&self) -> &Self::Database {
                    &self.database
                }

                async fn insert(
                    &self,
                    unit: &mut <Self::Database as renvor_database::Database>::UnitOfWork<'_>,
                    id: i64,
                ) -> Result<(), DatabaseError> {
                    // Through `ConnectionTrait` with a BOUND value — the idiomatic SeaORM path,
                    // and the one an application uses. FR-037: the identifier is a literal in this
                    // source, the value is a parameter.
                    //
                    // The placeholder is per-backend and comes from the macro. `from_sql_and_values`
                    // passes the SQL through to the driver rather than rewriting it, so `?` on
                    // PostgreSQL is rejected by the server — which is what the four-row matrix
                    // caught, and why the two rows do not share one string.
                    let statement = sea_orm::Statement::from_sql_and_values(
                        unit.get_database_backend(),
                        concat!("INSERT INTO ", $table, " (id) VALUES ", $insert),
                        [id.into()],
                    );
                    unit.execute_raw(statement)
                        .await
                        .map(|_| ())
                        .map_err(|error| classify_db_error(&error))
                }

                async fn count(&self) -> i64 {
                    // On the POOL, so outside any open transaction. Visibility is the property
                    // under test, and a count taken inside would see uncommitted writes.
                    sqlx::query_scalar(AssertSqlSafe(
                        concat!("SELECT COUNT(*) FROM ", $table).to_owned(),
                    ))
                    .fetch_one(self.database.pool())
                    .await
                    .expect("counts")
                }

                async fn reset(&self) {
                    sqlx::query(AssertSqlSafe(concat!("DELETE FROM ", $table).to_owned()))
                        .execute(self.database.pool())
                        .await
                        .expect("clears");
                }
            }

            async fn fixture() -> Option<(Fixture, tokio::sync::MutexGuard<'static, ()>)> {
                let guard = support::SHARED_FIXTURE.lock().await;
                let dsn = support::url($url)?;
                let settings = PoolSettings::default()
                    .with_max_connections(CAPACITY)
                    .expect("bounded")
                    .with_acquire_timeout(Duration::from_millis(750))
                    .expect("bounded");
                let database = $connect(&dsn, &settings).await.expect("connects");
                sqlx::query(AssertSqlSafe(
                    concat!(
                        "CREATE TABLE IF NOT EXISTS ",
                        $table,
                        " (id BIGINT PRIMARY KEY)"
                    )
                    .to_owned(),
                ))
                .execute(database.pool())
                .await
                .expect("creates");
                Some((Fixture { database }, guard))
            }

            #[tokio::test]
            async fn the_shared_persistence_contract_holds() {
                let Some((fixture, _guard)) = fixture().await else {
                    return;
                };
                persistence::run_every_shared_assertion(&fixture, $kind, CAPACITY as usize).await;
            }

            #[tokio::test]
            async fn a_closed_pool_refuses_rather_than_hangs() {
                let Some((fixture, _guard)) = fixture().await else {
                    return;
                };
                persistence::a_closed_pool_refuses_rather_than_hangs(&fixture).await;
            }
        }
    };
}

row!(
    postgres,
    "db-postgres",
    renvor_seaorm::PostgresDatabase,
    renvor_seaorm::connect_postgres,
    support::POSTGRES_URL,
    renvor_database::DatabaseKind::Postgres,
    "rv_sea_contract",
    "($1)"
);

row!(
    mysql,
    "db-mysql",
    renvor_seaorm::MySqlDatabase,
    renvor_seaorm::connect_mysql,
    support::MYSQL_URL,
    renvor_database::DatabaseKind::MySql,
    "rv_sea_contract",
    "(?)"
);
