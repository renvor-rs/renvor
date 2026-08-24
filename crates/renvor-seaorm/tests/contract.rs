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
    ($module:ident, $feature:literal, $alias:ty, $connect:path, $url:expr, $kind:expr, $table:literal, $insert:literal, $seed_run:ident) => {
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

                async fn count_within(
                    &self,
                    unit: &mut <Self::Database as renvor_database::Database>::UnitOfWork<'_>,
                ) -> i64 {
                    // Through the unit's OWN connection, which is the whole point — see the
                    // harness. `query_one_raw` is the idiomatic SeaORM read path.
                    let statement = sea_orm::Statement::from_string(
                        unit.get_database_backend(),
                        concat!("SELECT COUNT(*) AS c FROM ", $table),
                    );
                    unit.query_one_raw(statement)
                        .await
                        .expect("counts")
                        .expect("a row")
                        .try_get("", "c")
                        .expect("decodes")
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

                async fn seed(
                    &self,
                    scope: renvor_database::SeedScope,
                    seeds: &[renvor_database::SqlSeed],
                ) -> Result<renvor_database::SeedReport, DatabaseError> {
                    renvor_seaorm::seed::$seed_run(&self.database, scope, seeds).await
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

            /// **FR-006.** Every `PoolSettings` field reaches the pool, not just the one a
            /// capacity test happens to exercise.
            ///
            /// A review found six of the seven mapped and none of the six asserted — deleting
            /// `.min_connections(..)` or `.idle_timeout(..)` from `connect` would have failed
            /// nothing. `PoolOptions` exposes getters, so this reads back what was configured
            /// rather than inferring it from behaviour.
            ///
            /// `connect_timeout` and `drain_timeout` are absent from `PoolOptions` by design —
            /// SQLx has nowhere to put them, which is why this adapter applies them itself — so
            /// they are asserted where they are applied, not here.
            #[tokio::test]
            async fn every_pool_setting_reaches_the_pool() {
                let Some((fixture, _guard)) = fixture().await else {
                    return;
                };
                let settings = PoolSettings::default()
                    .with_max_connections(CAPACITY)
                    .expect("bounded")
                    .with_acquire_timeout(Duration::from_millis(750))
                    .expect("bounded");
                let options = fixture.database().pool().options();

                assert_eq!(
                    options.get_max_connections(),
                    settings.max_connections(),
                    "max_connections did not reach the pool"
                );
                assert_eq!(
                    options.get_min_connections(),
                    settings.min_connections(),
                    "min_connections did not reach the pool"
                );
                assert_eq!(
                    options.get_acquire_timeout(),
                    settings.acquire_timeout(),
                    "acquire_timeout did not reach the pool"
                );
                assert_eq!(
                    options.get_idle_timeout(),
                    Some(settings.idle_timeout()),
                    "idle_timeout did not reach the pool"
                );
                assert_eq!(
                    options.get_max_lifetime(),
                    Some(settings.max_lifetime()),
                    "max_lifetime did not reach the pool"
                );
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
    "($1)",
    run_postgres
);

row!(
    mysql,
    "db-mysql",
    renvor_seaorm::MySqlDatabase,
    renvor_seaorm::connect_mysql,
    support::MYSQL_URL,
    renvor_database::DatabaseKind::MySql,
    "rv_sea_contract",
    "(?)",
    run_mysql
);
