//! The direct-SQLx rows of the four-row contract matrix.
//!
//! Every assertion lives in [`renvor_testkit::persistence`]. `renvor-seaorm/tests/contract.rs`
//! calls the **same functions**, which is what makes *"both SeaORM rows pass the same application
//! contracts as direct SQLx"* checkable rather than asserted.
//!
//! `tests/contract.rs` in this crate remains as it was: it covers direct-SQLx specifics — the
//! repository shape, the executor abstraction, the SQL — that have no SeaORM counterpart and
//! therefore cannot be shared. This file adds only the shared half.

mod support;

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
use std::time::Duration;

/// Small enough that a lost slot is unmistakable, and the number the harness is told about.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
const CAPACITY: u32 = 3;

macro_rules! row {
    ($module:ident, $feature:literal, $alias:ty, $connect:path, $url:expr, $kind:expr, $insert:literal, $table:literal) => {
        #[cfg(feature = $feature)]
        mod $module {
            use super::*;
            use renvor_database::{DatabaseError, PoolSettings};
            use renvor_testkit::persistence::{self, PersistenceFixture};
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
                    // A BOUND value, on the transaction's own connection.
                    sqlx::query($insert)
                        .bind(id)
                        .execute(&mut **unit.inner())
                        .await
                        .map(|_| ())
                        .map_err(|error| renvor_sqlx::error::classify_error(&error))
                }

                async fn count_within(
                    &self,
                    unit: &mut <Self::Database as renvor_database::Database>::UnitOfWork<'_>,
                ) -> i64 {
                    // Through the unit's OWN connection — see the harness.
                    sqlx::query_scalar(AssertSqlSafe(
                        concat!("SELECT COUNT(*) FROM ", $table).to_owned(),
                    ))
                    .fetch_one(&mut **unit.inner())
                    .await
                    .expect("counts")
                }

                async fn count(&self) -> i64 {
                    // On the POOL, so outside any open transaction.
                    sqlx::query_scalar(AssertSqlSafe(
                        concat!("SELECT COUNT(*) FROM ", $table).to_owned(),
                    ))
                    .fetch_one(self.database.pool())
                    .await
                    .expect("counts")
                }

                async fn seed(
                    &self,
                    scope: renvor_database::SeedScope,
                    seeds: &[renvor_database::SqlSeed],
                ) -> Result<renvor_database::SeedReport, DatabaseError> {
                    renvor_sqlx::seed::run(&self.database, scope, seeds).await
                }

                async fn reset_seed_ledger(&self) {
                    for statement in [
                        "DROP TABLE IF EXISTS _renvor_seeds",
                        "DROP TABLE IF EXISTS rv_seed_probe",
                        "CREATE TABLE rv_seed_probe (id BIGINT PRIMARY KEY)",
                    ] {
                        let _ = sqlx::query(AssertSqlSafe(statement.to_owned()))
                            .execute(self.database.pool())
                            .await;
                    }
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
    renvor_sqlx::PostgresDatabase,
    renvor_sqlx::connect_postgres,
    support::POSTGRES_URL,
    renvor_database::DatabaseKind::Postgres,
    "INSERT INTO rv_shared_contract (id) VALUES ($1)",
    "rv_shared_contract"
);

row!(
    mysql,
    "db-mysql",
    renvor_sqlx::MySqlDatabase,
    renvor_sqlx::connect_mysql,
    support::MYSQL_URL,
    renvor_database::DatabaseKind::MySql,
    "INSERT INTO rv_shared_contract (id) VALUES (?)",
    "rv_shared_contract"
);
