//! The Phase 009 authentication schema, applied against real servers on both engines.
//!
//! # Why this lives in `renvor-sqlx` rather than in `renvor-auth`
//!
//! `renvor-auth` names no driver — that absence is asserted by `xtask`'s crate-DAG gate — so it
//! cannot open a connection. The migrations are **SQL files**, and this suite reaches them by path,
//! which needs no dependency between the two crates.
//!
//! Both adapters migrate on SQLx's engine (ADR-0022), so applying the set here covers the SeaORM
//! rows' migration behaviour too. What it does **not** cover is the SeaORM repositories, which get
//! their own suite.
//!
//! # Two directories, not one
//!
//! ADR-0025. There is no portable spelling for the two column shapes authentication needs: an
//! instant with microsecond precision that is valid past 2038, and sixteen opaque bytes. MySQL's
//! `TIMESTAMP` — the one instant type both engines name — **ends at 2038-01-19**, and `expires_at`
//! is exactly a column that may hold a later value.

mod support;

macro_rules! auth_migration_suite {
    ($module:ident, $feature:literal, $driver:ty, $connect:path, $run:ident, $url:expr, $engine:literal, $table_exists:literal) => {
        #[cfg(feature = $feature)]
        mod $module {
            use std::path::{Path, PathBuf};

            use renvor_database::{Database as _, MigrationOutcome, MigrationSettings};
            use renvor_sqlx::Migrations;
            use sqlx::AssertSqlSafe;

            use crate::support;

            /// Every table this phase creates, newest first — which is the order they must be
            /// dropped in, because each has a foreign key to `rv_auth_user`.
            const AUTH_TABLES: [&str; 7] = [
                "rv_auth_attempt",
                "rv_auth_refresh",
                "rv_auth_password_reset",
                "rv_auth_verification",
                "rv_auth_session",
                "rv_auth_credential",
                "rv_auth_user",
            ];

            /// The migration set for this engine.
            fn auth_set() -> PathBuf {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("renvor-auth")
                    .join("migrations")
                    .join($engine)
            }

            async fn blank() -> Option<(
                renvor_sqlx::SqlxDatabase<$driver>,
                tokio::sync::MutexGuard<'static, ()>,
            )> {
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
                Some((database, guard))
            }

            #[tokio::test]
            async fn every_migration_holds_exactly_one_schema_statement() {
                // `contracts/database-portability.md` §7, normative: "A portable migration must
                // contain exactly one schema statement. On MySQL that is the only way to guarantee
                // it has no partial state to be in."
                //
                // Not a style rule. MySQL forces an implicit commit on DDL, so a two-statement file
                // that fails halfway leaves the first statement committed, SQLx records the version
                // dirty, and EVERY LATER RUN IS REFUSED — there is no "run the rest".
                let mut checked = 0_usize;
                for entry in std::fs::read_dir(auth_set()).expect("the migration directory exists")
                {
                    let path = entry.expect("readable").path();
                    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    if !name.ends_with(".sql") {
                        continue;
                    }
                    let body = std::fs::read_to_string(&path).expect("readable");
                    let statements = body.matches(';').count();
                    assert_eq!(
                        statements, 1,
                        "{name} contains {statements} statements; the contract permits exactly one"
                    );
                    checked += 1;
                }
                // POSITIVE CONTROL: without it, a directory that could not be read would pass this
                // test having examined nothing — the vacuity failure this project has recorded twice.
                assert_eq!(
                    checked, 16,
                    "expected 16 migration files for {}, found {checked}",
                    $engine
                );
            }

            #[tokio::test]
            async fn the_whole_auth_schema_applies_and_is_idempotent() {
                let Some((database, _fixture)) = blank().await else {
                    return;
                };
                let migrations = Migrations::load(&auth_set(), MigrationSettings::default())
                    .await
                    .expect("the auth migration set loads");

                assert!(migrations.is_ordered(), "versions must strictly increase");
                assert_eq!(migrations.versions().len(), 8, "seven tables and one index");

                let first = migrations.$run(&database).await.expect("applies");
                assert_eq!(first.applied(), 8, "every migration should be new");
                assert!(
                    first
                        .steps()
                        .iter()
                        .all(|s| s.outcome() == MigrationOutcome::Applied)
                );

                // Idempotence is a property, not a hope.
                let second = migrations.$run(&database).await.expect("re-runs");
                assert_eq!(second.applied(), 0);

                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn every_auth_table_exists_after_migrating() {
                // The control that keeps the test above from passing on a set that applied
                // nothing: a migration runner reporting "8 applied" proves it ran, not that it
                // built the schema this phase needs.
                let Some((database, _fixture)) = blank().await else {
                    return;
                };
                let migrations = Migrations::load(&auth_set(), MigrationSettings::default())
                    .await
                    .expect("loads");
                migrations.$run(&database).await.expect("applies");

                for table in AUTH_TABLES {
                    let present: i64 =
                        sqlx::query_scalar(AssertSqlSafe($table_exists.replace("{table}", table)))
                            .fetch_one(database.pool())
                            .await
                            .expect("queries the catalogue");
                    assert_eq!(present, 1, "{table} was not created on {}", $engine);
                }

                // POSITIVE CONTROL: the catalogue query can report absence, so the sevens above
                // are facts about the schema rather than about a query that always answers 1.
                let absent: i64 = sqlx::query_scalar(AssertSqlSafe(
                    $table_exists.replace("{table}", "rv_auth_not_a_table"),
                ))
                .fetch_one(database.pool())
                .await
                .expect("queries the catalogue");
                assert_eq!(absent, 0, "the catalogue query cannot detect absence");

                database.close().await.expect("closes");
            }
        }
    };
}

auth_migration_suite!(
    postgres,
    "db-postgres",
    sqlx::Postgres,
    renvor_sqlx::connect_postgres,
    run_postgres,
    support::POSTGRES_URL,
    "postgres",
    "SELECT (CASE WHEN to_regclass('{table}') IS NULL THEN 0 ELSE 1 END)::bigint"
);
auth_migration_suite!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    renvor_sqlx::connect_mysql,
    run_mysql,
    support::MYSQL_URL,
    "mysql",
    "SELECT CAST(COUNT(*) AS SIGNED) FROM information_schema.tables \
     WHERE table_schema = DATABASE() AND table_name = '{table}'"
);
