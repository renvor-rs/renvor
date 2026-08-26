//! The direct-SQLx rows of the shared domain example.
//!
//! Every assertion lives in [`renvor_testkit::domain`]. `renvor-seaorm/tests/domain.rs` calls the
//! **same functions**, which is what makes *"one domain example passes against all four rows"*
//! checkable rather than asserted.
//!
//! What this file contributes is only the driver-specific half: the statements, which differ
//! between the engines exactly twice — in placeholder syntax, and in how `information_schema` is
//! scoped.

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
            use renvor_sqlx::Migrations;
            use renvor_testkit::domain::{self, Widget, WidgetFixture};
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

            impl WidgetFixture for Fixture {
                type Database = $alias;

                fn database(&self) -> &Self::Database {
                    &self.database
                }

                async fn migrate(&self) -> Result<usize, DatabaseError> {
                    // The adapter's REAL migration runner, over the same fixture directory the
                    // migration suite uses. Raw DDL here would make "migration" a setup step the
                    // example skipped past rather than one of the operations it proves.
                    let migrations =
                        Migrations::load(&migrations_dir(), MigrationSettings::default()).await?;
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
                    // Asked of the server's own catalogue. A runner that recorded a version
                    // without executing its SQL would pass any check derived from the run report.
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
                    // BOUND values, on the transaction's own connection.
                    sqlx::query($insert)
                        .bind(id)
                        .bind(name)
                        .execute(&mut **unit.inner())
                        .await
                        .map(|_| ())
                        .map_err(|error| renvor_sqlx::error::classify_error(&error))
                }

                async fn find(
                    &self,
                    unit: &mut <Self::Database as renvor_database::Database>::UnitOfWork<'_>,
                    id: i64,
                ) -> Option<Widget> {
                    sqlx::query_as::<_, (String, i64)>($find)
                        .bind(id)
                        .fetch_optional(&mut **unit.inner())
                        .await
                        .expect("reads")
                        .map(|(name, rank)| Widget { name, rank })
                }

                async fn set_rank(
                    &self,
                    unit: &mut <Self::Database as renvor_database::Database>::UnitOfWork<'_>,
                    id: i64,
                    rank: i64,
                ) -> Result<u64, DatabaseError> {
                    sqlx::query($set_rank)
                        .bind(rank)
                        .bind(id)
                        .execute(&mut **unit.inner())
                        .await
                        .map(|done| done.rows_affected())
                        .map_err(|error| renvor_sqlx::error::classify_error(&error))
                }

                async fn remove(
                    &self,
                    unit: &mut <Self::Database as renvor_database::Database>::UnitOfWork<'_>,
                    id: i64,
                ) -> Result<u64, DatabaseError> {
                    sqlx::query($remove)
                        .bind(id)
                        .execute(&mut **unit.inner())
                        .await
                        .map(|done| done.rows_affected())
                        .map_err(|error| renvor_sqlx::error::classify_error(&error))
                }

                async fn ids_after(
                    &self,
                    unit: &mut <Self::Database as renvor_database::Database>::UnitOfWork<'_>,
                    after: i64,
                    limit: i64,
                ) -> Vec<i64> {
                    sqlx::query_scalar::<_, i64>($page)
                        .bind(after)
                        .bind(limit)
                        .fetch_all(&mut **unit.inner())
                        .await
                        .expect("pages")
                }
            }

            async fn fixture() -> Option<(Fixture, tokio::sync::MutexGuard<'static, ()>)> {
                let guard = support::SHARED_FIXTURE.lock().await;
                let dsn = support::url($url)?;
                let database = $connect(&dsn, &support::settings())
                    .await
                    .expect("connects");
                let fixture = Fixture { database };
                // Each test starts from a schema this run created, so the migration assertion is
                // measuring its own run rather than a previous one.
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
        }
    };
}

row!(
    postgres,
    "db-postgres",
    renvor_sqlx::PostgresDatabase,
    renvor_sqlx::connect_postgres,
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
    renvor_sqlx::MySqlDatabase,
    renvor_sqlx::connect_mysql,
    run_mysql,
    support::MYSQL_URL,
    "INSERT INTO rv_widget (id, name) VALUES (?, ?)",
    "SELECT name, rank_value FROM rv_widget WHERE id = ?",
    "UPDATE rv_widget SET rank_value = ? WHERE id = ?",
    "DELETE FROM rv_widget WHERE id = ?",
    "SELECT id FROM rv_widget WHERE id > ? ORDER BY id ASC LIMIT ?",
    // `table_schema = DATABASE()` matters here and not on PostgreSQL: MySQL's
    // `information_schema` spans every schema on the server, so an unscoped count would
    // also match an `rv_widget` left behind in another database.
    "SELECT COUNT(*) FROM information_schema.columns \
     WHERE table_schema = DATABASE() AND table_name = 'rv_widget' \
     AND column_name = 'rank_value'"
);
