//! The SeaORM rows of the shared domain example.
//!
//! Every assertion lives in [`renvor_testkit::domain`] and is compiled once. The direct-SQLx rows
//! call the same functions from `renvor-sqlx/tests/domain.rs`, so *"one domain example passes
//! against all four rows"* is a fact about the build rather than two suites that happen to agree.
//!
//! What this file contributes is the driver-specific half, expressed the way an application using
//! SeaORM would: a [`sea_orm::Statement`] with bound values, executed through
//! [`sea_orm::ConnectionTrait`].

mod support;

/// One engine's row of the domain example.
macro_rules! row {
    (
        $module:ident,
        $feature:literal,
        $alias:ty,
        $connect:path,
        $run:ident,
        $url:expr,
        $insert:literal,
        $find:literal,
        $set_rank:literal,
        $remove:literal,
        $page:literal,
        $rank_column:literal
    ) => {
        #[cfg(feature = $feature)]
        mod $module {
            use std::path::{Path, PathBuf};

            use renvor_database::{DatabaseError, MigrationSettings};
            use renvor_seaorm::error::classify_db_error;
            use renvor_seaorm::migrate::Migrations;
            use renvor_testkit::concurrency;
            use renvor_testkit::domain::{self, Widget, WidgetFixture};
            use renvor_testkit::upgrade;
            use sea_orm::ConnectionTrait as _;
            use sqlx::AssertSqlSafe;

            use super::support;

            struct Fixture {
                database: $alias,
            }

            fn migrations_dir() -> PathBuf {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests")
                    .join("migrations")
            }

            fn base_migrations_dir() -> PathBuf {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests")
                    .join("migrations-upgrade-base")
            }

            impl WidgetFixture for Fixture {
                type Database = $alias;

                fn database(&self) -> &Self::Database {
                    &self.database
                }

                async fn migrate(&self) -> Result<usize, DatabaseError> {
                    // ADR-0022: both ORM choices migrate on SQLx's engine, so a project has exactly
                    // one migration history. This is that path, not a SeaORM-specific one.
                    let migrations =
                        Migrations::load(&migrations_dir(), MigrationSettings::default()).await?;
                    migrations.$run(&self.database).await.map(|r| r.applied())
                }

                async fn migrate_base(&self) -> Result<usize, DatabaseError> {
                    // The SAME runner over the previous release's directory. A different code
                    // path here would make the upgrade suite measure something no deployment does.
                    let migrations =
                        Migrations::load(&base_migrations_dir(), MigrationSettings::default())
                            .await?;
                    migrations.$run(&self.database).await.map(|r| r.applied())
                }

                async fn drop_schema(&self) {
                    for statement in [
                        "DROP TABLE IF EXISTS rv_widget",
                        "DROP TABLE IF EXISTS _sqlx_migrations",
                    ] {
                        sqlx::query(AssertSqlSafe(statement.to_owned()))
                            .execute(self.database.pool())
                            .await
                            .expect("cleans");
                    }
                }

                async fn rank_column_exists(&self) -> bool {
                    let count: i64 = sqlx::query_scalar(AssertSqlSafe($rank_column.to_owned()))
                        .fetch_one(self.database.pool())
                        .await
                        .expect("reads the catalogue");
                    count == 1
                }

                async fn insert(
                    &self,
                    unit: &mut <Self::Database as renvor_database::Database>::UnitOfWork<'_>,
                    id: i64,
                    name: &str,
                ) -> Result<(), DatabaseError> {
                    let statement = sea_orm::Statement::from_sql_and_values(
                        unit.get_database_backend(),
                        $insert,
                        [id.into(), name.into()],
                    );
                    unit.execute_raw(statement)
                        .await
                        .map(|_| ())
                        .map_err(|error| classify_db_error(&error))
                }

                async fn find(
                    &self,
                    unit: &mut <Self::Database as renvor_database::Database>::UnitOfWork<'_>,
                    id: i64,
                ) -> Option<Widget> {
                    let statement = sea_orm::Statement::from_sql_and_values(
                        unit.get_database_backend(),
                        $find,
                        [id.into()],
                    );
                    let row = unit.query_one_raw(statement).await.expect("reads")?;
                    Some(Widget {
                        name: row.try_get("", "name").expect("decodes name"),
                        rank: row.try_get("", "rank_value").expect("decodes rank"),
                    })
                }

                async fn set_rank(
                    &self,
                    unit: &mut <Self::Database as renvor_database::Database>::UnitOfWork<'_>,
                    id: i64,
                    rank: i64,
                ) -> Result<u64, DatabaseError> {
                    let statement = sea_orm::Statement::from_sql_and_values(
                        unit.get_database_backend(),
                        $set_rank,
                        [rank.into(), id.into()],
                    );
                    unit.execute_raw(statement)
                        .await
                        .map(|done| done.rows_affected())
                        .map_err(|error| classify_db_error(&error))
                }

                async fn remove(
                    &self,
                    unit: &mut <Self::Database as renvor_database::Database>::UnitOfWork<'_>,
                    id: i64,
                ) -> Result<u64, DatabaseError> {
                    let statement = sea_orm::Statement::from_sql_and_values(
                        unit.get_database_backend(),
                        $remove,
                        [id.into()],
                    );
                    unit.execute_raw(statement)
                        .await
                        .map(|done| done.rows_affected())
                        .map_err(|error| classify_db_error(&error))
                }

                async fn ids_after(
                    &self,
                    unit: &mut <Self::Database as renvor_database::Database>::UnitOfWork<'_>,
                    after: i64,
                    limit: i64,
                ) -> Vec<i64> {
                    let statement = sea_orm::Statement::from_sql_and_values(
                        unit.get_database_backend(),
                        $page,
                        [after.into(), limit.into()],
                    );
                    unit.query_all_raw(statement)
                        .await
                        .expect("pages")
                        .into_iter()
                        .map(|row| row.try_get("", "id").expect("decodes id"))
                        .collect()
                }
            }

            async fn fixture() -> Option<(Fixture, tokio::sync::MutexGuard<'static, ()>)> {
                let guard = support::SHARED_FIXTURE.lock().await;
                let dsn = support::url($url)?;
                let database = $connect(&dsn, &support::settings())
                    .await
                    .expect("connects");
                let fixture = Fixture { database };
                fixture.drop_schema().await;
                fixture.migrate().await.expect("migrates");
                Some((fixture, guard))
            }

            #[tokio::test]
            async fn the_shared_domain_example_holds() {
                let Some((fixture, _guard)) = fixture().await else {
                    return;
                };
                domain::run_every_domain_assertion(&fixture).await;
            }

            /// The concurrency and idempotency contract, on this row.
            ///
            /// Capacity is read back from the very settings the pool was built with, so a change
            /// to `support::settings()` makes the race fail loudly rather than quietly turn into a
            /// queue on the pool.
            #[tokio::test]
            async fn the_shared_concurrency_contract_holds() {
                let Some((fixture, _guard)) = fixture().await else {
                    return;
                };
                let capacity = support::settings().max_connections() as usize;
                concurrency::run_every_concurrency_assertion(&fixture, capacity).await;
            }

            /// The upgrade path, on this row.
            #[tokio::test]
            async fn the_upgrade_path_holds() {
                let Some((fixture, _guard)) = fixture().await else {
                    return;
                };
                upgrade::run_every_upgrade_assertion(&fixture).await;
            }
        }
    };
}

row!(
    postgres,
    "db-postgres",
    renvor_seaorm::PostgresDatabase,
    renvor_seaorm::connect_postgres,
    run_postgres,
    support::POSTGRES_URL,
    "INSERT INTO rv_widget (id, name) VALUES ($1, $2)",
    "SELECT name, rank_value FROM rv_widget WHERE id = $1",
    "UPDATE rv_widget SET rank_value = $1 WHERE id = $2",
    "DELETE FROM rv_widget WHERE id = $1",
    "SELECT id FROM rv_widget WHERE id > $1 ORDER BY id ASC LIMIT $2",
    "SELECT COUNT(*) FROM information_schema.columns \
     WHERE table_name = 'rv_widget' AND column_name = 'rank_value'"
);

row!(
    mysql,
    "db-mysql",
    renvor_seaorm::MySqlDatabase,
    renvor_seaorm::connect_mysql,
    run_mysql,
    support::MYSQL_URL,
    "INSERT INTO rv_widget (id, name) VALUES (?, ?)",
    "SELECT name, rank_value FROM rv_widget WHERE id = ?",
    "UPDATE rv_widget SET rank_value = ? WHERE id = ?",
    "DELETE FROM rv_widget WHERE id = ?",
    "SELECT id FROM rv_widget WHERE id > ? ORDER BY id ASC LIMIT ?",
    "SELECT COUNT(*) FROM information_schema.columns \
     WHERE table_schema = DATABASE() AND table_name = 'rv_widget' \
     AND column_name = 'rank_value'"
);
