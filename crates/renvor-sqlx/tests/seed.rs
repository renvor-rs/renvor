//! Seeds against a real database: scope, idempotence, atomicity, and redaction.

mod support;

macro_rules! suite {
    ($module:ident, $feature:literal, $alias:ty, $connect:path, $url:expr) => {
        #[cfg(feature = $feature)]
        mod $module {
            use super::*;
            use renvor_database::{Idempotence, SeedDeclaration, SeedScope};
            use renvor_sqlx::seed::{SqlSeed, run};
            use sqlx::AssertSqlSafe;

            async fn database() -> Option<$alias> {
                let dsn = support::url($url)?;
                let database: $alias = $connect(&dsn, &support::settings()).await.expect("connects");
                for statement in [
                    "DROP TABLE IF EXISTS _renvor_seeds",
                    "DROP TABLE IF EXISTS rv_seeded",
                    "CREATE TABLE rv_seeded (id BIGINT PRIMARY KEY)",
                ] {
                    sqlx::query(AssertSqlSafe(statement.to_owned()))
                        .execute(database.pool())
                        .await
                        .expect("prepares");
                }
                Some(database)
            }

            async fn count(database: &$alias) -> i64 {
                sqlx::query_scalar(AssertSqlSafe("SELECT COUNT(*) FROM rv_seeded".to_owned()))
                    .fetch_one(database.pool())
                    .await
                    .expect("counts")
            }

            fn once(name: &str, id: i64) -> SqlSeed {
                SqlSeed::new(
                    SeedDeclaration::new(name, SeedScope::Test, Idempotence::RunOnce),
                    vec![format!("INSERT INTO rv_seeded (id) VALUES ({id})")],
                )
            }

            /// FR-035: a `RunOnce` seed runs once, however many times the runner is invoked.
            #[tokio::test]
            async fn a_run_once_seed_is_not_applied_twice() {
                let Some(database) = database().await else {
                    return;
                };
                let seeds = [once("insert-one", 1)];

                let first = run(&database, SeedScope::Test, &seeds).await.expect("runs");
                assert_eq!(first.applied(), ["insert-one"]);
                assert_eq!(count(&database).await, 1);

                let second = run(&database, SeedScope::Test, &seeds).await.expect("runs");
                assert!(second.applied().is_empty(), "it ran twice");
                assert_eq!(second.skipped_already_applied(), ["insert-one"]);
                assert_eq!(count(&database).await, 1, "a second run inserted again");
            }

            /// An `Idempotent` seed runs every time, because its author said that is safe.
            #[tokio::test]
            async fn an_idempotent_seed_runs_every_time() {
                let Some(database) = database().await else {
                    return;
                };
                // `DELETE` then `INSERT` — genuinely repeatable, which is what the declaration
                // claims. A seed declared idempotent that is not would corrupt data, so the
                // declaration is the author's assertion and this is a seed that honours it.
                let seeds = [SqlSeed::new(
                    SeedDeclaration::new("reset", SeedScope::Test, Idempotence::Idempotent),
                    vec![
                        "DELETE FROM rv_seeded".to_owned(),
                        "INSERT INTO rv_seeded (id) VALUES (42)".to_owned(),
                    ],
                )];

                for round in 0..3 {
                    let report = run(&database, SeedScope::Test, &seeds).await.expect("runs");
                    assert_eq!(report.applied(), ["reset"], "round {round}");
                    assert_eq!(count(&database).await, 1, "round {round}");
                }
            }

            /// FR-033: a seed declared for another scope does not run, and the report says why.
            #[tokio::test]
            async fn an_out_of_scope_seed_is_skipped_and_named() {
                let Some(database) = database().await else {
                    return;
                };
                let seeds = [SqlSeed::new(
                    SeedDeclaration::new("dev-only", SeedScope::Development, Idempotence::RunOnce),
                    vec!["INSERT INTO rv_seeded (id) VALUES (7)".to_owned()],
                )];
                let report = run(&database, SeedScope::Test, &seeds).await.expect("runs");
                assert!(report.applied().is_empty());
                assert_eq!(report.skipped_out_of_scope(), ["dev-only"]);
                assert_eq!(count(&database).await, 0);
            }

            /// A failing seed applies nothing and is NOT recorded as applied.
            ///
            /// The ledger row is written in the same transaction as the statements, so this is a
            /// property of the construction rather than of ordering the writes carefully.
            #[tokio::test]
            async fn a_failing_seed_leaves_no_trace() {
                let Some(database) = database().await else {
                    return;
                };
                let seeds = [SqlSeed::new(
                    SeedDeclaration::new("broken", SeedScope::Test, Idempotence::RunOnce),
                    vec![
                        "INSERT INTO rv_seeded (id) VALUES (5)".to_owned(),
                        "INSERT INTO rv_seeded (id) VALUES (5)".to_owned(),
                    ],
                )];
                let error = run(&database, SeedScope::Test, &seeds)
                    .await
                    .expect_err("the duplicate key fails");
                assert!(!format!("{error:?}").contains("rv_seeded"), "the SQL leaked");
                assert_eq!(count(&database).await, 0, "a failed seed left a row behind");

                // And it may be retried, because it was never recorded.
                let retried = run(
                    &database,
                    SeedScope::Test,
                    &[once("broken", 5)],
                )
                .await
                .expect("runs");
                assert_eq!(retried.applied(), ["broken"]);
            }

            /// FR-036: nothing a seed run reports can carry a credential.
            #[tokio::test]
            async fn a_seed_report_carries_no_credential() {
                let Some(database) = database().await else {
                    return;
                };
                let seeds = [SqlSeed::new(
                    SeedDeclaration::new("safe", SeedScope::Test, Idempotence::RunOnce),
                    vec![format!(
                        "INSERT INTO rv_seeded (id) VALUES ({})",
                        support::CREDENTIAL_CANARY.len()
                    )],
                )];
                let report = run(&database, SeedScope::Test, &seeds).await.expect("runs");
                let rendered = format!("{report:?}");
                assert!(!rendered.contains(support::CREDENTIAL_CANARY));
                // The report names seeds, never their SQL: a statement is where a literal would be.
                assert!(!rendered.contains("INSERT"));
            }

            /// Seeds are applied in the order given, not in some order the database chose.
            #[tokio::test]
            async fn seeds_apply_in_declaration_order() {
                let Some(database) = database().await else {
                    return;
                };
                let seeds = [once("c", 3), once("a", 1), once("b", 2)];
                let report = run(&database, SeedScope::Test, &seeds).await.expect("runs");
                assert_eq!(report.applied(), ["c", "a", "b"]);
            }
        }
    };
}

suite!(
    postgres,
    "db-postgres",
    renvor_sqlx::PostgresDatabase,
    renvor_sqlx::connect_postgres,
    support::POSTGRES_URL
);
suite!(
    mysql,
    "db-mysql",
    renvor_sqlx::MySqlDatabase,
    renvor_sqlx::connect_mysql,
    support::MYSQL_URL
);
