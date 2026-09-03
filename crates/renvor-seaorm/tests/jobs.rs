//! The four-row job-store suite: the SHARED contract (`renvor_testkit::jobs`) over this adapter
//! on both engines, plus the migration set that creates `rv_job` (FR-023, FR-040).
//!
//! Every assertion lives in the testkit; this file supplies the fixture and nothing else, so the
//! memory substitute and all four rows are held to one text.

mod support;

macro_rules! jobs_suite {
    ($module:ident, $feature:literal, $driver:ty, $connect:path, $run:ident, $url:expr, $engine:literal) => {
        #[cfg(feature = $feature)]
        mod $module {
            use std::path::{Path, PathBuf};
            use std::sync::Arc;

            use renvor_core::observe::OsEntropy;
            use renvor_database::MigrationSettings;
            use renvor_jobs::JobBounds;
            use renvor_seaorm::jobs::$module::SeaOrmJobStore;
            use renvor_seaorm::migrate::Migrations;
            use renvor_testkit::jobs::JobsFixture;
            use sea_orm::ConnectionTrait as _;

            use crate::support;

            fn job_set() -> PathBuf {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("renvor-jobs")
                    .join("migrations")
                    .join($engine)
            }

            /// A migrated, empty `rv_job`, plus the guard that keeps the suite serial.
            async fn migrated() -> Option<(
                Arc<renvor_seaorm::SeaOrmDatabase<$driver>>,
                tokio::sync::MutexGuard<'static, ()>,
            )> {
                let guard = support::SHARED_FIXTURE.lock().await;
                let dsn = support::url($url)?;
                let database = $connect(&dsn, &support::settings())
                    .await
                    .expect("connects");
                {
                    let connection = database.acquire().await.expect("acquires");
                    for table in ["rv_job", "_sqlx_migrations"] {
                        connection
                            .execute_unprepared(&format!("DROP TABLE IF EXISTS {table}"))
                            .await
                            .expect("cleans");
                    }
                }
                Migrations::load(&job_set(), MigrationSettings::default())
                    .await
                    .expect("loads the job migration set")
                    .$run(&database)
                    .await
                    .expect("applies the job migration set");
                Some((Arc::new(database), guard))
            }

            struct Fixture {
                database: Arc<renvor_seaorm::SeaOrmDatabase<$driver>>,
                store: Arc<SeaOrmJobStore>,
            }

            impl JobsFixture for Fixture {
                type Store = SeaOrmJobStore;

                fn store(&self) -> Arc<Self::Store> {
                    Arc::clone(&self.store)
                }

                fn bounds(&self) -> JobBounds {
                    *self.store.bounds()
                }

                async fn reset(&self) {
                    let connection = self.database.acquire().await.expect("acquires");
                    connection
                        .execute_unprepared("DELETE FROM rv_job")
                        .await
                        .expect("clears");
                }
            }

            /// FR-040: the contract, unchanged, on this row. Multi-threaded so the two barrier
            /// races contend on the server, not on one thread's turn order.
            #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
            async fn the_shared_jobs_contract_holds() {
                let Some((database, _guard)) = migrated().await else {
                    return;
                };
                let bounds = JobBounds::new().with_max_queue_depth(3).unwrap();
                let store = Arc::new(SeaOrmJobStore::new(
                    Arc::clone(&database),
                    bounds,
                    Arc::new(OsEntropy::new()),
                ));
                let fixture = Fixture { database, store };
                renvor_testkit::jobs::the_shared_jobs_contract_holds(&fixture).await;
                drop(fixture);
            }

            /// `contracts/database-portability.md` §7: exactly one schema statement per file, so a
            /// MySQL implicit commit can never leave a half-applied file behind.
            #[test]
            fn every_job_migration_holds_exactly_one_schema_statement() {
                let mut seen = 0;
                for entry in std::fs::read_dir(job_set()).expect("the set exists") {
                    let path = entry.expect("an entry").path();
                    if path.extension().is_some_and(|e| e == "sql") {
                        let body = std::fs::read_to_string(&path).expect("readable");
                        let count = body.matches(';').count();
                        assert_eq!(count, 1, "a migration file holds more than one statement");
                        seen += 1;
                    }
                }
                assert_eq!(seen, 8, "four up and four down files");
            }

            /// FR-023: the set applies, is idempotent on a second run, and leaves the table.
            #[tokio::test]
            async fn the_job_schema_applies_and_a_second_run_changes_nothing() {
                let Some((database, _guard)) = migrated().await else {
                    return;
                };
                Migrations::load(&job_set(), MigrationSettings::default())
                    .await
                    .expect("loads")
                    .$run(&database)
                    .await
                    .expect("a second run is a no-op");
                let connection = database.acquire().await.expect("acquires");
                let row = connection
                    .query_one_raw(sea_orm::Statement::from_string(
                        connection.get_database_backend(),
                        "SELECT COUNT(*) FROM rv_job",
                    ))
                    .await
                    .expect("the table exists after migrating")
                    .expect("one row");
                let count: i64 = row.try_get_by_index(0).expect("a count");
                assert_eq!(count, 0);
            }
        }
    };
}

jobs_suite!(
    postgres,
    "db-postgres",
    sqlx::Postgres,
    renvor_seaorm::connect_postgres,
    run_postgres,
    support::POSTGRES_URL,
    "postgres"
);
jobs_suite!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    renvor_seaorm::connect_mysql,
    run_mysql,
    support::MYSQL_URL,
    "mysql"
);
