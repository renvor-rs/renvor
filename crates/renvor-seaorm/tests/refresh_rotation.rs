//! The SeaORM rows of the refresh-rotation contract.
//!
//! Every assertion lives in [`renvor_testkit::refresh`] — the **same functions**
//! `renvor-sqlx/tests/refresh_rotation.rs` calls. That is what makes *"the behaviour is identical
//! across both engines and both adapters"* a property of the build rather than of two files that
//! agree by inspection.
//!
//! # What is different here, and what is not
//!
//! Different: the programming model. Statements go through `sea_orm::ConnectionTrait` and the
//! transaction is a `SeaOrmUnitOfWork`.
//!
//! Not different: the SQL's meaning, the lock order, or a single assertion. The two adapters take
//! the family lock before the token lock and write both halves of a rotation in one transaction,
//! because that is what the port requires — and this file cannot express a weaker version of it.

mod support;

macro_rules! refresh_suite {
    ($module:ident, $feature:literal, $driver:ty, $connect:path, $run:ident, $url:expr, $engine:literal, $kind:expr) => {
        #[cfg(all(feature = $feature, feature = "tokens"))]
        mod $module {
            use std::path::{Path, PathBuf};
            use std::sync::Arc;

            use chrono::{DateTime, Utc};
            use renvor_auth::opaque::SecretDigest;
            use renvor_auth::refresh::{FamilyId, RefreshTokenId};
            use renvor_auth::subject::UserId;
            use renvor_database::{Database as _, DatabaseKind, MigrationSettings};
            use renvor_seaorm::migrate::Migrations;
            use renvor_seaorm::auth::$module::SeaOrmRefreshTokenRepository;
            use renvor_testkit::refresh::{RefreshFixture, StoredRefreshToken};
            use sea_orm::ConnectionTrait as _;

            use crate::support;

            /// This engine's placeholder rule — the same single source the adapter composes from.
            const KIND: DatabaseKind = $kind;

            /// Newest first, which is the order they must be dropped in: `rv_auth_refresh` has a
            /// foreign key to `rv_auth_refresh_family`, which has one to `rv_auth_user`.
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
                repository: SeaOrmRefreshTokenRepository,
            }

            impl Fixture {
                /// One bound statement, on a pooled connection outside any transaction.
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
            }

            impl RefreshFixture for Fixture {
                type Repository = SeaOrmRefreshTokenRepository;

                fn repository(&self) -> &Self::Repository {
                    &self.repository
                }

                async fn reset(&self) {
                    let connection = self.database.acquire().await.expect("acquires");
                    // The family cascade takes the token rows with it, so there is one statement
                    // per table rather than an ordering to get wrong.
                    for table in ["rv_auth_refresh_family", "rv_auth_user"] {
                        connection
                            .execute_unprepared(&format!("DELETE FROM {table}"))
                            .await
                            .expect("clears");
                    }
                }

                async fn create_user(&self) -> UserId {
                    let id = UserId::from_bytes(support::rand16());
                    let now = DateTime::from_timestamp(1_800_000_000, 0).expect("representable");
                    let connection = self.database.acquire().await.expect("acquires");
                    let statement = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &format!(
                            "INSERT INTO rv_auth_user (id, email, email_verified_at, created_at, updated_at) VALUES ({}, {}, NULL, {}, {})",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                            KIND.placeholder(3),
                            KIND.placeholder(4),
                        ),
                        [
                            id.as_bytes().to_vec().into(),
                            format!("{id}@refresh.test").into(),
                            now.into(),
                            now.into(),
                        ],
                    );
                    connection
                        .execute_raw(statement)
                        .await
                        .expect("creates a user");
                    id
                }

                async fn stored_token_hashes(&self) -> Vec<Vec<u8>> {
                    self.query("SELECT token_hash FROM rv_auth_refresh".to_owned(), vec![])
                        .await
                        .into_iter()
                        .map(|row| row.try_get("", "token_hash").expect("a digest column"))
                        .collect()
                }

                async fn tokens_in(&self, family: FamilyId) -> Vec<StoredRefreshToken> {
                    self.query(
                        format!(
                            "SELECT token_hash, expires_at, consumed_at, revoked_at, replaced_by FROM rv_auth_refresh WHERE family_id = {} ORDER BY issued_at ASC, id ASC",
                            KIND.placeholder(1)
                        ),
                        vec![family.as_bytes().to_vec().into()],
                    )
                    .await
                    .into_iter()
                    .map(|row| {
                        let hash: Vec<u8> = row.try_get("", "token_hash").expect("a digest");
                        let replaced_by: Option<Vec<u8>> =
                            row.try_get("", "replaced_by").expect("a nullable identifier");
                        StoredRefreshToken {
                            digest: SecretDigest::from_bytes(
                                hash.try_into().expect("a 32-byte digest"),
                            ),
                            expires_at: row.try_get("", "expires_at").expect("an instant"),
                            consumed_at: row.try_get("", "consumed_at").expect("an instant"),
                            revoked_at: row.try_get("", "revoked_at").expect("an instant"),
                            replaced_by: replaced_by.map(|bytes| {
                                RefreshTokenId::from_bytes(
                                    bytes.try_into().expect("a 16-byte identifier"),
                                )
                            }),
                        }
                    })
                    .collect()
                }

                async fn family_revoked_at(&self, family: FamilyId) -> Option<DateTime<Utc>> {
                    self.query(
                        format!(
                            "SELECT revoked_at FROM rv_auth_refresh_family WHERE id = {}",
                            KIND.placeholder(1)
                        ),
                        vec![family.as_bytes().to_vec().into()],
                    )
                    .await
                    .first()
                    .and_then(|row| row.try_get("", "revoked_at").expect("an instant"))
                }

                async fn family_scope_claim(&self, family: FamilyId) -> Option<String> {
                    self.query(
                        format!(
                            "SELECT scopes FROM rv_auth_refresh_family WHERE id = {}",
                            KIND.placeholder(1)
                        ),
                        vec![family.as_bytes().to_vec().into()],
                    )
                    .await
                    .first()
                    .map(|row| row.try_get("", "scopes").expect("a claim"))
                }

                async fn family_user(&self, family: FamilyId) -> Option<UserId> {
                    self.query(
                        format!(
                            "SELECT user_id FROM rv_auth_refresh_family WHERE id = {}",
                            KIND.placeholder(1)
                        ),
                        vec![family.as_bytes().to_vec().into()],
                    )
                    .await
                    .first()
                    .map(|row| {
                        let bytes: Vec<u8> = row.try_get("", "user_id").expect("an identifier");
                        UserId::from_bytes(bytes.try_into().expect("a 16-byte identifier"))
                    })
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
                let repository = SeaOrmRefreshTokenRepository::new(Arc::clone(&database));
                Some((
                    Fixture {
                        database,
                        repository,
                    },
                    guard,
                ))
            }

            #[tokio::test]
            async fn the_shared_refresh_contract_holds() {
                let Some((fixture, _guard)) = migrated().await else {
                    return;
                };
                renvor_testkit::refresh::run_every_refresh_assertion(&fixture).await;
                fixture.database.close().await.expect("closes");
            }
        }
    };
}

refresh_suite!(
    postgres,
    "db-postgres",
    sqlx::Postgres,
    renvor_seaorm::connect_postgres,
    run_postgres,
    support::POSTGRES_URL,
    "postgres",
    renvor_database::DatabaseKind::Postgres
);
refresh_suite!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    renvor_seaorm::connect_mysql,
    run_mysql,
    support::MYSQL_URL,
    "mysql",
    renvor_database::DatabaseKind::MySql
);
