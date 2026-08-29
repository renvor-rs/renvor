//! The direct-SQLx rows of the bounded abuse-control contract.
//!
//! Every assertion lives in [`renvor_testkit::abuse`]. `renvor-seaorm/tests/abuse_controls.rs`
//! calls the **same functions**, which is what makes *"the behaviour is identical across both
//! engines and both adapters"* checkable rather than asserted.
//!
//! # Why this suite exists at all
//!
//! Two claims cannot be measured anywhere else.
//!
//! **Atomicity.** `observe` promises to increment and report the resulting count as one step. A
//! fake with no suspension point cannot fail that promise — batch G2 shipped a HIGH race under
//! exactly that mistake — so the racing assertions need real pooled connections and a real row
//! lock.
//!
//! **The row bound.** `max_rows = |AttemptDimension| × buckets` is a claim about a primary key's
//! domain. An in-memory map would satisfy it trivially by being a map; only the table can show
//! that 400 distinct identifiers leave at most 256 rows behind, with `prune` never called.

mod support;

macro_rules! abuse_suite {
    ($module:ident, $feature:literal, $driver:ty, $connect:path, $run:ident, $url:expr, $engine:literal, $kind:expr) => {
        #[cfg(feature = $feature)]
        mod $module {
            use std::path::{Path, PathBuf};

            use chrono::{DateTime, Utc};
            use renvor_auth::abuse::AttemptDimension;
            use renvor_database::{Database as _, DatabaseKind, MigrationSettings};
            use renvor_sqlx::Migrations;
            use renvor_sqlx::auth::$module::SqlxAttemptRepository;
            use renvor_testkit::abuse::{AbuseFixture, StoredAttempt};
            use sqlx::AssertSqlSafe;

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
                database: renvor_sqlx::SqlxDatabase<$driver>,
                repository: SqlxAttemptRepository,
            }

            impl AbuseFixture for Fixture {
                type Repository = SqlxAttemptRepository;

                fn repository(&self) -> &Self::Repository {
                    &self.repository
                }

                async fn reset(&self) {
                    sqlx::query(AssertSqlSafe("DELETE FROM rv_auth_attempt".to_owned()))
                        .execute(self.database.pool())
                        .await
                        .expect("clears");
                }

                async fn row_count(&self) -> u64 {
                    let count: i64 = sqlx::query_scalar(AssertSqlSafe(
                        "SELECT COUNT(*) FROM rv_auth_attempt".to_owned(),
                    ))
                    .fetch_one(self.database.pool())
                    .await
                    .expect("counts");
                    u64::try_from(count).expect("a non-negative count")
                }

                async fn row(
                    &self,
                    dimension: AttemptDimension,
                    bucket: u32,
                ) -> Option<StoredAttempt> {
                    let select = format!(
                        "SELECT window_start, current_count, previous_count, expires_at FROM rv_auth_attempt WHERE dimension = {} AND bucket = {}",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                    );
                    let row: Option<(DateTime<Utc>, i64, i64, DateTime<Utc>)> =
                        sqlx::query_as(AssertSqlSafe(select))
                            .bind(dimension.code())
                            .bind(i32::try_from(bucket).expect("a representable bucket"))
                            .fetch_optional(self.database.pool())
                            .await
                            .expect("reads the row");
                    row.map(|(window_start, current, previous, expires_at)| StoredAttempt {
                        window_start,
                        current: u64::try_from(current).expect("a non-negative count"),
                        previous: u64::try_from(previous).expect("a non-negative count"),
                        expires_at,
                    })
                }

                async fn seed(
                    &self,
                    dimension: AttemptDimension,
                    bucket: u32,
                    row: StoredAttempt,
                ) {
                    // DELETE-then-INSERT rather than an upsert: this is a test fixture writing a
                    // known state, and an upsert here would be a second implementation of the
                    // thing under test.
                    let delete = format!(
                        "DELETE FROM rv_auth_attempt WHERE dimension = {} AND bucket = {}",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                    );
                    sqlx::query(AssertSqlSafe(delete))
                        .bind(dimension.code())
                        .bind(i32::try_from(bucket).expect("a representable bucket"))
                        .execute(self.database.pool())
                        .await
                        .expect("clears the row");
                    let insert = format!(
                        "INSERT INTO rv_auth_attempt (dimension, bucket, window_start, current_count, previous_count, expires_at) VALUES ({}, {}, {}, {}, {}, {})",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                        KIND.placeholder(4),
                        KIND.placeholder(5),
                        KIND.placeholder(6),
                    );
                    sqlx::query(AssertSqlSafe(insert))
                        .bind(dimension.code())
                        .bind(i32::try_from(bucket).expect("a representable bucket"))
                        .bind(row.window_start)
                        .bind(i64::try_from(row.current).expect("representable"))
                        .bind(i64::try_from(row.previous).expect("representable"))
                        .bind(row.expires_at)
                        .execute(self.database.pool())
                        .await
                        .expect("seeds the row");
                }

                async fn dump(&self) -> Vec<String> {
                    let rows: Vec<(i16, i32, DateTime<Utc>, i64, i64, DateTime<Utc>)> =
                        sqlx::query_as(AssertSqlSafe(
                            "SELECT dimension, bucket, window_start, current_count, previous_count, expires_at FROM rv_auth_attempt".to_owned(),
                        ))
                        .fetch_all(self.database.pool())
                        .await
                        .expect("reads every row");
                    rows.into_iter()
                        .map(|(dimension, bucket, window, current, previous, expires)| {
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
                for table in AUTH_TABLES {
                    sqlx::query(AssertSqlSafe(format!("DROP TABLE IF EXISTS {table}")))
                        .execute(database.pool())
                        .await
                        .expect("cleans");
                }
                sqlx::query(AssertSqlSafe(
                    "DROP TABLE IF EXISTS _sqlx_migrations".to_owned(),
                ))
                .execute(database.pool())
                .await
                .expect("cleans");
                Migrations::load(&auth_set(), MigrationSettings::default())
                    .await
                    .expect("loads the auth migration set")
                    .$run(&database)
                    .await
                    .expect("applies the auth migration set");
                let repository = SqlxAttemptRepository::new(database.pool().clone());
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
    renvor_sqlx::connect_postgres,
    run_postgres,
    support::POSTGRES_URL,
    "postgres",
    renvor_database::DatabaseKind::Postgres
);
abuse_suite!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    renvor_sqlx::connect_mysql,
    run_mysql,
    support::MYSQL_URL,
    "mysql",
    renvor_database::DatabaseKind::MySql
);
