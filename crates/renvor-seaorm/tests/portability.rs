//! The SeaORM side of the cross-database portability contract.
//!
//! The assertions are [`renvor_testkit::portability`]'s, the same functions the direct-SQLx rows
//! call. What differs is the route: every statement here goes through [`sea_orm::Statement`] and
//! [`sea_orm::ConnectionTrait`], as an application using SeaORM would issue it.
//!
//! # What running this on the ORM axis does and does not add
//!
//! It does **not** add engine facts — MySQL resolves `ON DUPLICATE KEY` the same way whoever
//! formatted the statement. What it adds is the check that these facts are *reachable* through
//! SeaORM at all, and that its raw-statement path returns them unchanged. A SeaORM row that could
//! not observe the difference would be a row that could not defend against it.

mod support;

/// One engine's probes, through SeaORM.
macro_rules! engine {
    (
        $module:ident,
        $feature:literal,
        $alias:ty,
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
            use renvor_seaorm::error::classify_db_error;
            use renvor_testkit::portability::{
                self, OversizedIdentifier, PortabilityFixture, UnnamedKeyUpsert,
            };
            use sea_orm::ConnectionTrait as _;

            use super::support;

            /// One character past MySQL's 64-character limit. See the direct-SQLx twin.
            fn oversized_name() -> String {
                format!("rv_pt_{}", "a".repeat(59))
            }

            struct Fixture {
                database: $alias,
            }

            impl Fixture {
                /// Runs a statement that must succeed, through SeaORM.
                async fn run(&self, sql: &str) {
                    let connection = self.database.acquire().await.expect("acquires");
                    let statement = sea_orm::Statement::from_string(
                        connection.get_database_backend(),
                        sql.to_owned(),
                    );
                    connection
                        .execute_raw(statement)
                        .await
                        .unwrap_or_else(|error| panic!("the probe's own setup failed: {error}"));
                }

                /// Reads one aliased integer column.
                async fn scalar(&self, sql: &str, column: &str) -> i64 {
                    let connection = self.database.acquire().await.expect("acquires");
                    let statement = sea_orm::Statement::from_string(
                        connection.get_database_backend(),
                        sql.to_owned(),
                    );
                    connection
                        .query_one_raw(statement)
                        .await
                        .expect("the probe's own query must be readable")
                        .expect("the probe's own query must return a row")
                        .try_get("", column)
                        .expect("decodes")
                }

                /// Reads one aliased text column.
                async fn text(&self, sql: &str, column: &str) -> String {
                    let connection = self.database.acquire().await.expect("acquires");
                    let statement = sea_orm::Statement::from_string(
                        connection.get_database_backend(),
                        sql.to_owned(),
                    );
                    connection
                        .query_one_raw(statement)
                        .await
                        .expect("readable")
                        .expect("a row")
                        .try_get("", column)
                        .expect("decodes")
                }
            }

            impl PortabilityFixture for Fixture {
                type Database = $alias;

                fn database(&self) -> &Self::Database {
                    &self.database
                }

                async fn nulls_ascending(&self) -> Vec<Option<i64>> {
                    let connection = self.database.acquire().await.expect("acquires");
                    let statement = sea_orm::Statement::from_string(
                        connection.get_database_backend(),
                        $nulls.to_owned(),
                    );
                    connection
                        .query_all_raw(statement)
                        .await
                        .expect("the ordering probe must run")
                        .into_iter()
                        .map(|row| row.try_get("", "v").expect("decodes"))
                        .collect()
                }

                async fn ddl_survives_rollback(&self) -> bool {
                    self.run("DROP TABLE IF EXISTS rv_pt_ddl").await;

                    // Through Renvor's own unit of work, so this measures the transaction an
                    // application would actually have open around a migration step.
                    // Not `mut`: SeaORM's `ConnectionTrait` takes `&self`, so a unit of work
                    // executes without a unique borrow. `rollback` still consumes it.
                    let unit = self.database.begin().await.expect("begins");
                    let statement = sea_orm::Statement::from_string(
                        unit.get_database_backend(),
                        "CREATE TABLE rv_pt_ddl (x INT)".to_owned(),
                    );
                    unit.execute_raw(statement).await.expect("creates");
                    unit.rollback().await.expect("rolls back");

                    let survived = self.scalar($ddl_exists, "present").await == 1;
                    self.run("DROP TABLE IF EXISTS rv_pt_ddl").await;
                    survived
                }

                async fn default_timestamp_digits(&self) -> u32 {
                    self.run("DROP TABLE IF EXISTS rv_pt_ts").await;
                    self.run(&format!("CREATE TABLE rv_pt_ts (a {})", $ts_column))
                        .await;
                    let digits = self.scalar($ts_precision, "digits").await;
                    self.run("DROP TABLE IF EXISTS rv_pt_ts").await;
                    u32::try_from(digits).expect("a precision is not negative")
                }

                async fn upsert_on_unnamed_key(&self) -> UnnamedKeyUpsert {
                    self.run("DROP TABLE IF EXISTS rv_pt_upsert").await;
                    self.run(
                        "CREATE TABLE rv_pt_upsert (id BIGINT PRIMARY KEY, \
                         tag VARCHAR(20) UNIQUE, v BIGINT)",
                    )
                    .await;
                    self.run("INSERT INTO rv_pt_upsert VALUES (1, 'x', 1)")
                        .await;

                    let connection = self.database.acquire().await.expect("acquires");
                    let statement = sea_orm::Statement::from_string(
                        connection.get_database_backend(),
                        $upsert.to_owned(),
                    );
                    let outcome = match connection.execute_raw(statement).await {
                        Err(error) => UnnamedKeyUpsert::Refused(classify_db_error(&error).kind()),
                        Ok(_) => UnnamedKeyUpsert::RewroteAnotherRow(
                            self.scalar("SELECT id AS id FROM rv_pt_upsert WHERE v = 9", "id")
                                .await,
                        ),
                    };
                    self.run("DROP TABLE IF EXISTS rv_pt_upsert").await;
                    outcome
                }

                async fn repeated_read_sees_concurrent_commit(&self) -> bool {
                    self.run("DROP TABLE IF EXISTS rv_pt_iso").await;
                    self.run("CREATE TABLE rv_pt_iso (id BIGINT PRIMARY KEY)")
                        .await;

                    let mut reader = self.database.begin().await.expect("begins");
                    let count =
                        |unit: &mut <$alias as renvor_database::Database>::UnitOfWork<'_>| {
                            sea_orm::Statement::from_string(
                                unit.get_database_backend(),
                                "SELECT COUNT(*) AS n FROM rv_pt_iso".to_owned(),
                            )
                        };

                    // The snapshot is taken by this first read, not by `BEGIN`.
                    let statement = count(&mut reader);
                    let before: i64 = reader
                        .query_one_raw(statement)
                        .await
                        .expect("reads")
                        .expect("a row")
                        .try_get("", "n")
                        .expect("decodes");

                    self.run("INSERT INTO rv_pt_iso VALUES (1)").await;

                    let statement = count(&mut reader);
                    let after: i64 = reader
                        .query_one_raw(statement)
                        .await
                        .expect("reads")
                        .expect("a row")
                        .try_get("", "n")
                        .expect("decodes");
                    reader.rollback().await.expect("rolls back");

                    self.run("DROP TABLE IF EXISTS rv_pt_iso").await;
                    assert_eq!(
                        before, 0,
                        "the isolation probe did not start from an empty table"
                    );
                    after > before
                }

                async fn oversized_identifier(&self) -> OversizedIdentifier {
                    let name = oversized_name();
                    let connection = self.database.acquire().await.expect("acquires");
                    let statement = sea_orm::Statement::from_string(
                        connection.get_database_backend(),
                        format!("CREATE TABLE {name} (x INT)"),
                    );
                    match connection.execute_raw(statement).await {
                        Err(_) => OversizedIdentifier::Refused,
                        Ok(_) => {
                            let created = self.text($long_exists, "name").await;
                            self.run(&format!("DROP TABLE IF EXISTS {created}")).await;
                            OversizedIdentifier::Truncated(created)
                        }
                    }
                }

                async fn json_round_trip(&self) -> String {
                    self.text($json_ok, "doc").await
                }

                async fn json_round_trip_unadvised(&self) -> Option<String> {
                    let sql: Option<&str> = $json_bad;
                    Some(self.text(sql?, "doc").await)
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
    renvor_seaorm::PostgresDatabase,
    renvor_seaorm::connect_postgres,
    support::POSTGRES_URL,
    "SELECT v FROM (VALUES (2::bigint), (NULL::bigint), (1::bigint)) t(v) ORDER BY v ASC",
    "SELECT (CASE WHEN to_regclass('rv_pt_ddl') IS NULL THEN 0 ELSE 1 END)::bigint AS present",
    "TIMESTAMP",
    "SELECT datetime_precision::bigint AS digits FROM information_schema.columns \
     WHERE table_name = 'rv_pt_ts' AND column_name = 'a'",
    "INSERT INTO rv_pt_upsert VALUES (2, 'x', 9) ON CONFLICT (id) DO UPDATE SET v = EXCLUDED.v",
    "SELECT relname::text AS name FROM pg_class WHERE relname LIKE 'rv_pt_a%' LIMIT 1",
    r#"SELECT ('{"b":1,"a":2,"a":3}'::jsonb)::text AS doc"#,
    Some(r#"SELECT ('{"b":1,"a":2,"a":3}'::json)::text AS doc"#)
);

engine!(
    mysql,
    "db-mysql",
    renvor_seaorm::MySqlDatabase,
    renvor_seaorm::connect_mysql,
    support::MYSQL_URL,
    "SELECT v FROM (SELECT CAST(2 AS SIGNED) AS v UNION ALL SELECT CAST(NULL AS SIGNED) \
     UNION ALL SELECT CAST(1 AS SIGNED)) t ORDER BY v ASC",
    "SELECT CAST(COUNT(*) AS SIGNED) AS present FROM information_schema.tables \
     WHERE table_schema = DATABASE() AND table_name = 'rv_pt_ddl'",
    "DATETIME",
    "SELECT CAST(datetime_precision AS SIGNED) AS digits FROM information_schema.columns \
     WHERE table_schema = DATABASE() AND table_name = 'rv_pt_ts' AND column_name = 'a'",
    "INSERT INTO rv_pt_upsert VALUES (2, 'x', 9) ON DUPLICATE KEY UPDATE v = 9",
    "SELECT table_name AS name FROM information_schema.tables \
     WHERE table_schema = DATABASE() AND table_name LIKE 'rv_pt_a%' LIMIT 1",
    r#"SELECT CAST(CAST('{"b":1,"a":2,"a":3}' AS JSON) AS CHAR) AS doc"#,
    None
);
