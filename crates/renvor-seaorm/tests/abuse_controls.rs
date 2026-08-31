//! The SeaORM rows of the bounded abuse-control contract.
//!
//! Every assertion lives in [`renvor_testkit::abuse`] — the **same functions**
//! `renvor-sqlx/tests/abuse_controls.rs` calls. That is what makes *"the behaviour is identical
//! across both engines and both adapters"* a property of the build rather than of two files that
//! agree by inspection.
//!
//! # What is different here, and what is not
//!
//! Different: the programming model. Statements go through `sea_orm::ConnectionTrait` and the
//! transaction is a `SeaOrmUnitOfWork`.
//!
//! Not different: the three statements, their order, or a single assertion. Both adapters ensure
//! the row, lock it, and write counts computed in Rust — and this file cannot express a weaker
//! version of that, because it does not contain the assertions.

mod support;

macro_rules! abuse_suite {
    ($module:ident, $feature:literal, $driver:ty, $connect:path, $run:ident, $url:expr, $engine:literal, $kind:expr) => {
        #[cfg(feature = $feature)]
        mod $module {
            use std::path::{Path, PathBuf};
            use std::sync::Arc;

            use chrono::{DateTime, Utc};
            use renvor_auth::abuse::AttemptDimension;
            use renvor_database::{Database as _, DatabaseKind, MigrationSettings};
            use renvor_seaorm::auth::$module::SeaOrmAttemptRepository;
            use renvor_seaorm::migrate::Migrations;
            use renvor_testkit::abuse::{AbuseFixture, StoredAttempt};
            use sea_orm::ConnectionTrait as _;

            use crate::support;

            /// This engine's placeholder rule — the same single source the adapter composes from.
            const KIND: DatabaseKind = $kind;

            /// Newest first, which is the order they must be dropped in.
            const AUTH_TABLES: [&str; 8] = [
                "rv_auth_attempt",
                "rv_auth_refresh",
                "rv_auth_refresh_family",
                "rv_auth_password_reset",
                "rv_auth_verification",
                "rv_auth_session",
                "rv_auth_credential",
                "rv_auth_user",
            ];

            fn auth_set() -> PathBuf {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("renvor-auth")
                    .join("migrations")
                    .join($engine)
            }

            struct Fixture {
                database: Arc<renvor_seaorm::SeaOrmDatabase<$driver>>,
                repository: SeaOrmAttemptRepository,
            }

            impl Fixture {
                async fn query(
                    &self,
                    sql: String,
                    values: Vec<sea_orm::Value>,
                ) -> Vec<sea_orm::QueryResult> {
                    let connection = self.database.acquire().await.expect("acquires");
                    let statement = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &sql,
                        values,
                    );
                    connection
                        .query_all_raw(statement)
                        .await
                        .expect("the observation query runs")
                }

                async fn exec(&self, sql: String, values: Vec<sea_orm::Value>) {
                    let connection = self.database.acquire().await.expect("acquires");
                    let statement = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &sql,
                        values,
                    );
                    connection
                        .execute_raw(statement)
                        .await
                        .expect("the fixture statement runs");
                }
            }

            impl AbuseFixture for Fixture {
                type Repository = SeaOrmAttemptRepository;

                fn repository(&self) -> &Self::Repository {
                    &self.repository
                }

                async fn reset(&self) {
                    self.exec("DELETE FROM rv_auth_attempt".to_owned(), vec![])
                        .await;
                }

                async fn row_count(&self) -> u64 {
                    let rows = self
                        .query(
                            "SELECT COUNT(*) AS total FROM rv_auth_attempt".to_owned(),
                            vec![],
                        )
                        .await;
                    let total: i64 = rows
                        .first()
                        .expect("a count row")
                        .try_get("", "total")
                        .expect("a count column");
                    u64::try_from(total).expect("a non-negative count")
                }

                async fn row(
                    &self,
                    dimension: AttemptDimension,
                    bucket: u32,
                ) -> Option<StoredAttempt> {
                    let rows = self
                        .query(
                            format!(
                                "SELECT window_start, current_count, previous_count, expires_at FROM rv_auth_attempt WHERE dimension = {} AND bucket = {}",
                                KIND.placeholder(1),
                                KIND.placeholder(2),
                            ),
                            vec![
                                i32::from(dimension.code()).into(),
                                i32::try_from(bucket).expect("a representable bucket").into(),
                            ],
                        )
                        .await;
                    let row = rows.first()?;
                    let current: i64 = row.try_get("", "current_count").expect("a count column");
                    let previous: i64 = row.try_get("", "previous_count").expect("a count column");
                    Some(StoredAttempt {
                        window_start: row
                            .try_get::<DateTime<Utc>>("", "window_start")
                            .expect("an instant column"),
                        current: u64::try_from(current).expect("a non-negative count"),
                        previous: u64::try_from(previous).expect("a non-negative count"),
                        expires_at: row
                            .try_get::<DateTime<Utc>>("", "expires_at")
                            .expect("an instant column"),
                    })
                }

                async fn seed(&self, dimension: AttemptDimension, bucket: u32, row: StoredAttempt) {
                    // DELETE-then-INSERT rather than an upsert: this is a test fixture writing a
                    // known state, and an upsert here would be a second implementation of the
                    // thing under test.
                    let bucket = i32::try_from(bucket).expect("a representable bucket");
                    self.exec(
                        format!(
                            "DELETE FROM rv_auth_attempt WHERE dimension = {} AND bucket = {}",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                        ),
                        vec![i32::from(dimension.code()).into(), bucket.into()],
                    )
                    .await;
                    self.exec(
                        format!(
                            "INSERT INTO rv_auth_attempt (dimension, bucket, window_start, current_count, previous_count, expires_at) VALUES ({}, {}, {}, {}, {}, {})",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                            KIND.placeholder(3),
                            KIND.placeholder(4),
                            KIND.placeholder(5),
                            KIND.placeholder(6),
                        ),
                        vec![
                            i32::from(dimension.code()).into(),
                            bucket.into(),
                            row.window_start.into(),
                            i64::try_from(row.current).expect("representable").into(),
                            i64::try_from(row.previous).expect("representable").into(),
                            row.expires_at.into(),
                        ],
                    )
                    .await;
                }

                async fn dump(&self) -> Vec<String> {
                    self.query(
                        "SELECT dimension, bucket, window_start, current_count, previous_count, expires_at FROM rv_auth_attempt".to_owned(),
                        vec![],
                    )
                    .await
                    .into_iter()
                    .map(|row| {
                        let dimension: i16 = row.try_get("", "dimension").expect("a code column");
                        let bucket: i32 = row.try_get("", "bucket").expect("a bucket column");
                        let window: DateTime<Utc> =
                            row.try_get("", "window_start").expect("an instant column");
                        let current: i64 =
                            row.try_get("", "current_count").expect("a count column");
                        let previous: i64 =
                            row.try_get("", "previous_count").expect("a count column");
                        let expires: DateTime<Utc> =
                            row.try_get("", "expires_at").expect("an instant column");
                        format!("{dimension}|{bucket}|{window}|{current}|{previous}|{expires}")
                    })
                    .collect()
                }
            }

            async fn migrated() -> Option<(Fixture, tokio::sync::MutexGuard<'static, ()>)> {
                let guard = support::SHARED_FIXTURE.lock().await;
                let dsn = support::url($url)?;
                let database = $connect(&dsn, &support::settings())
                    .await
                    .expect("connects");
                {
                    let connection = database.acquire().await.expect("acquires");
                    for table in AUTH_TABLES {
                        connection
                            .execute_unprepared(&format!("DROP TABLE IF EXISTS {table}"))
                            .await
                            .expect("cleans");
                    }
                    connection
                        .execute_unprepared("DROP TABLE IF EXISTS _sqlx_migrations")
                        .await
                        .expect("cleans");
                }
                Migrations::load(&auth_set(), MigrationSettings::default())
                    .await
                    .expect("loads")
                    .$run(&database)
                    .await
                    .expect("migrates");
                let database = Arc::new(database);
                let repository = SeaOrmAttemptRepository::new(Arc::clone(&database));
                Some((
                    Fixture {
                        database,
                        repository,
                    },
                    guard,
                ))
            }

            #[tokio::test]
            async fn the_shared_abuse_contract_holds() {
                let Some((fixture, _guard)) = migrated().await else {
                    return;
                };
                renvor_testkit::abuse::run_every_abuse_assertion(&fixture).await;
                fixture.database.close().await.expect("closes");
            }
        }
    };
}

abuse_suite!(
    postgres,
    "db-postgres",
    sqlx::Postgres,
    renvor_seaorm::connect_postgres,
    run_postgres,
    support::POSTGRES_URL,
    "postgres",
    renvor_database::DatabaseKind::Postgres
);
abuse_suite!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    renvor_seaorm::connect_mysql,
    run_mysql,
    support::MYSQL_URL,
    "mysql",
    renvor_database::DatabaseKind::MySql
);
