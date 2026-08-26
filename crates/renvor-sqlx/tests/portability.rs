//! The direct-SQLx side of the cross-database portability contract.
//!
//! Every assertion lives in [`renvor_testkit::portability`]. What this file supplies is the SQL,
//! which is the part that genuinely differs — and differs *more* here than anywhere else in the
//! suite, because the differences are the subject.
//!
//! Each probe is written to report **what the server did**, never to return the answer the
//! contract expects. A probe that hard-coded `Refused` for PostgreSQL would keep passing after
//! PostgreSQL stopped refusing.

mod support;

/// One engine's probes.
macro_rules! engine {
    (
        $module:ident,
        $feature:literal,
        $alias:ty,
        $driver:ty,
        $connect:path,
        $url:expr,
        $nulls:literal,
        $ddl_exists:literal,
        $ts_column:literal,
        $ts_precision:literal,
        $upsert:literal,
        $long_exists:literal,
        $json_ok:literal,
        $json_bad:expr
    ) => {
        #[cfg(feature = $feature)]
        mod $module {
            use renvor_database::{Database as _, PoolSettings, UnitOfWork as _};
            use renvor_testkit::portability::{
                self, OversizedIdentifier, PortabilityFixture, UnnamedKeyUpsert,
            };
            use sqlx::AssertSqlSafe;

            use super::support;

            /// A name one character past MySQL's 64-character limit, so both engines are asked the
            /// same question. The `rv_pt_` prefix survives PostgreSQL's truncation to 63, which is
            /// what lets the catalogue query find whatever was actually created.
            fn oversized_name() -> String {
                format!("rv_pt_{}", "a".repeat(59))
            }

            struct Fixture {
                database: $alias,
            }

            impl Fixture {
                async fn run(&self, sql: String) {
                    sqlx::query(AssertSqlSafe(sql))
                        .execute(self.database.pool())
                        .await
                        .expect("the probe's own setup must succeed");
                }

                async fn scalar(&self, sql: &str) -> i64 {
                    sqlx::query_scalar(AssertSqlSafe(sql.to_owned()))
                        .fetch_one(self.database.pool())
                        .await
                        .expect("the probe's own query must be readable")
                }
            }

            impl PortabilityFixture for Fixture {
                type Database = $alias;

                fn database(&self) -> &Self::Database {
                    &self.database
                }

                async fn nulls_ascending(&self) -> Vec<Option<i64>> {
                    sqlx::query_scalar(AssertSqlSafe($nulls.to_owned()))
                        .fetch_all(self.database.pool())
                        .await
                        .expect("the ordering probe must run")
                }

                async fn ddl_survives_rollback(&self) -> bool {
                    self.run("DROP TABLE IF EXISTS rv_pt_ddl".to_owned()).await;

                    // The DDL goes inside an explicit transaction which is then rolled back. On
                    // one engine that undoes it; on the other an implicit commit already made it
                    // permanent before the rollback was even sent.
                    let mut tx = self.database.pool().begin().await.expect("begins");
                    sqlx::query(AssertSqlSafe("CREATE TABLE rv_pt_ddl (x INT)".to_owned()))
                        .execute(&mut *tx)
                        .await
                        .expect("creates");
                    tx.rollback().await.expect("rolls back");

                    let survived = self.scalar($ddl_exists).await == 1;
                    self.run("DROP TABLE IF EXISTS rv_pt_ddl".to_owned()).await;
                    survived
                }

                async fn default_timestamp_digits(&self) -> u32 {
                    self.run("DROP TABLE IF EXISTS rv_pt_ts".to_owned()).await;
                    // Declared with NO precision, which is the whole question.
                    self.run(format!("CREATE TABLE rv_pt_ts (a {})", $ts_column))
                        .await;
                    let digits = self.scalar($ts_precision).await;
                    self.run("DROP TABLE IF EXISTS rv_pt_ts".to_owned()).await;
                    u32::try_from(digits).expect("a precision is not negative")
                }

                async fn upsert_on_unnamed_key(&self) -> UnnamedKeyUpsert {
                    self.run("DROP TABLE IF EXISTS rv_pt_upsert".to_owned())
                        .await;
                    self.run(
                        "CREATE TABLE rv_pt_upsert (id BIGINT PRIMARY KEY, \
                         tag VARCHAR(20) UNIQUE, v BIGINT)"
                            .to_owned(),
                    )
                    .await;
                    self.run("INSERT INTO rv_pt_upsert VALUES (1, 'x', 1)".to_owned())
                        .await;

                    // Inserts id 2 with tag 'x'. The conflict is on `tag`, and the statement
                    // scopes itself to `id` wherever scoping is possible at all.
                    let attempt = sqlx::query(AssertSqlSafe($upsert.to_owned()))
                        .execute(self.database.pool())
                        .await;

                    let outcome = match attempt {
                        Err(error) => UnnamedKeyUpsert::Refused(
                            renvor_sqlx::error::classify_error(&error).kind(),
                        ),
                        // It was accepted. WHICH row now carries the new value is the finding.
                        Ok(_) => UnnamedKeyUpsert::RewroteAnotherRow(
                            self.scalar("SELECT id FROM rv_pt_upsert WHERE v = 9").await,
                        ),
                    };
                    self.run("DROP TABLE IF EXISTS rv_pt_upsert".to_owned())
                        .await;
                    outcome
                }

                async fn repeated_read_sees_concurrent_commit(&self) -> bool {
                    self.run("DROP TABLE IF EXISTS rv_pt_iso".to_owned()).await;
                    self.run("CREATE TABLE rv_pt_iso (id BIGINT PRIMARY KEY)".to_owned())
                        .await;

                    let mut reader = self.database.begin().await.expect("begins");
                    // This FIRST read is load-bearing: a repeatable-read snapshot is taken here,
                    // not at `BEGIN`. Without it the transaction would take its snapshot after
                    // the concurrent commit and both engines would agree.
                    let before: i64 = sqlx::query_scalar(AssertSqlSafe(
                        "SELECT COUNT(*) FROM rv_pt_iso".to_owned(),
                    ))
                    .fetch_one(&mut **reader.inner())
                    .await
                    .expect("reads");

                    // A different session entirely, committed while the reader is still open.
                    self.run("INSERT INTO rv_pt_iso VALUES (1)".to_owned())
                        .await;

                    let after: i64 = sqlx::query_scalar(AssertSqlSafe(
                        "SELECT COUNT(*) FROM rv_pt_iso".to_owned(),
                    ))
                    .fetch_one(&mut **reader.inner())
                    .await
                    .expect("reads");
                    reader.rollback().await.expect("rolls back");

                    self.run("DROP TABLE IF EXISTS rv_pt_iso".to_owned()).await;
                    assert_eq!(
                        before, 0,
                        "the isolation probe did not start from an empty table"
                    );
                    after > before
                }

                async fn oversized_identifier(&self) -> OversizedIdentifier {
                    let name = oversized_name();
                    let attempt =
                        sqlx::query(AssertSqlSafe(format!("CREATE TABLE {name} (x INT)")))
                            .execute(self.database.pool())
                            .await;

                    match attempt {
                        Err(_) => OversizedIdentifier::Refused,
                        Ok(_) => {
                            // Accepted — but under WHICH name? Asked of the catalogue rather than
                            // assumed, because the whole point is that it may not be the one sent.
                            let created: String =
                                sqlx::query_scalar(AssertSqlSafe($long_exists.to_owned()))
                                    .fetch_one(self.database.pool())
                                    .await
                                    .expect("the catalogue must name what was created");
                            self.run(format!("DROP TABLE IF EXISTS {created}")).await;
                            OversizedIdentifier::Truncated(created)
                        }
                    }
                }

                async fn json_round_trip(&self) -> String {
                    sqlx::query_scalar(AssertSqlSafe($json_ok.to_owned()))
                        .fetch_one(self.database.pool())
                        .await
                        .expect("the JSON probe must run")
                }

                async fn json_round_trip_unadvised(&self) -> Option<String> {
                    let sql: Option<&str> = $json_bad;
                    let sql = sql?;
                    Some(
                        sqlx::query_scalar(AssertSqlSafe(sql.to_owned()))
                            .fetch_one(self.database.pool())
                            .await
                            .expect("the control probe must run"),
                    )
                }
            }

            async fn fixture() -> Option<(Fixture, tokio::sync::MutexGuard<'static, ()>)> {
                let guard = support::SHARED_FIXTURE.lock().await;
                let dsn = support::url($url)?;
                let settings = PoolSettings::default()
                    .with_max_connections(4)
                    .expect("bounded")
                    .with_acquire_timeout(core::time::Duration::from_secs(5))
                    .expect("bounded");
                let database = $connect(&dsn, &settings).await.expect("connects");
                Some((Fixture { database }, guard))
            }

            #[tokio::test]
            async fn the_portability_contract_holds() {
                let Some((fixture, _guard)) = fixture().await else {
                    return;
                };
                portability::run_every_portability_assertion(&fixture).await;
            }
        }
    };
}

engine!(
    postgres,
    "db-postgres",
    renvor_sqlx::PostgresDatabase,
    sqlx::Postgres,
    renvor_sqlx::connect_postgres,
    support::POSTGRES_URL,
    "SELECT v FROM (VALUES (2::bigint), (NULL::bigint), (1::bigint)) t(v) ORDER BY v ASC",
    "SELECT (CASE WHEN to_regclass('rv_pt_ddl') IS NULL THEN 0 ELSE 1 END)::bigint",
    "TIMESTAMP",
    "SELECT datetime_precision::bigint FROM information_schema.columns \
     WHERE table_name = 'rv_pt_ts' AND column_name = 'a'",
    "INSERT INTO rv_pt_upsert VALUES (2, 'x', 9) ON CONFLICT (id) DO UPDATE SET v = EXCLUDED.v",
    "SELECT relname::text FROM pg_class WHERE relname LIKE 'rv_pt_a%' LIMIT 1",
    r#"SELECT ('{"b":1,"a":2,"a":3}'::jsonb)::text"#,
    // PostgreSQL is the only engine here with a second, NON-portable JSON type.
    Some(r#"SELECT ('{"b":1,"a":2,"a":3}'::json)::text"#)
);

engine!(
    mysql,
    "db-mysql",
    renvor_sqlx::MySqlDatabase,
    sqlx::MySql,
    renvor_sqlx::connect_mysql,
    support::MYSQL_URL,
    "SELECT v FROM (SELECT CAST(2 AS SIGNED) AS v UNION ALL SELECT CAST(NULL AS SIGNED) \
     UNION ALL SELECT CAST(1 AS SIGNED)) t ORDER BY v ASC",
    "SELECT CAST(COUNT(*) AS SIGNED) FROM information_schema.tables \
     WHERE table_schema = DATABASE() AND table_name = 'rv_pt_ddl'",
    "DATETIME",
    "SELECT CAST(datetime_precision AS SIGNED) FROM information_schema.columns \
     WHERE table_schema = DATABASE() AND table_name = 'rv_pt_ts' AND column_name = 'a'",
    "INSERT INTO rv_pt_upsert VALUES (2, 'x', 9) ON DUPLICATE KEY UPDATE v = 9",
    "SELECT table_name FROM information_schema.tables \
     WHERE table_schema = DATABASE() AND table_name LIKE 'rv_pt_a%' LIMIT 1",
    r#"SELECT CAST(CAST('{"b":1,"a":2,"a":3}' AS JSON) AS CHAR)"#,
    // MySQL has one JSON type, so there is no unadvised alternative to control against.
    None
);
