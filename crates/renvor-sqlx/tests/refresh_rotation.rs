//! The direct-SQLx rows of the refresh-rotation contract.
//!
//! Every assertion lives in [`renvor_testkit::refresh`]. `renvor-seaorm/tests/refresh_rotation.rs`
//! calls the **same functions**, which is what makes *"the behaviour is identical across both
//! engines and both adapters"* checkable rather than asserted.
//!
//! # Why this suite exists at all
//!
//! The refresh design it measures replaced one that shipped with a green unit suite and a
//! confirmed HIGH race: a winner could insert its successor into a family a concurrent replay had
//! already revoked. The unit test that was supposed to catch it raced two rotations against a fake
//! store whose `async fn`s contain no `.await`, so nothing ever interleaved. **A property of a
//! transaction can only be measured against a server**, and that is the whole reason for this file.

mod support;

macro_rules! refresh_suite {
    ($module:ident, $feature:literal, $driver:ty, $connect:path, $run:ident, $url:expr, $engine:literal, $kind:expr) => {
        #[cfg(all(feature = $feature, feature = "tokens"))]
        mod $module {
            use std::path::{Path, PathBuf};

            use chrono::{DateTime, Utc};
            use renvor_auth::opaque::SecretDigest;
            use renvor_auth::refresh::{FamilyId, RefreshTokenId};
            use renvor_auth::subject::UserId;
            use renvor_database::{Database as _, DatabaseKind, MigrationSettings};
            use renvor_sqlx::Migrations;
            use renvor_sqlx::auth::$module::SqlxRefreshTokenRepository;
            use renvor_testkit::refresh::{RefreshFixture, StoredRefreshToken};
            use sqlx::AssertSqlSafe;

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
                database: renvor_sqlx::SqlxDatabase<$driver>,
                repository: SqlxRefreshTokenRepository,
            }

            impl RefreshFixture for Fixture {
                type Repository = SqlxRefreshTokenRepository;

                fn repository(&self) -> &Self::Repository {
                    &self.repository
                }

                async fn reset(&self) {
                    // The family cascade takes the token rows with it, so there is one statement
                    // per table rather than an ordering to get wrong.
                    for table in ["rv_auth_refresh_family", "rv_auth_user"] {
                        sqlx::query(AssertSqlSafe(format!("DELETE FROM {table}")))
                            .execute(self.database.pool())
                            .await
                            .expect("clears");
                    }
                }

                async fn create_user(&self) -> UserId {
                    let id = UserId::from_bytes(support::rand16());
                    let statement = format!(
                        "INSERT INTO rv_auth_user (id, email, email_verified_at, created_at, updated_at) VALUES ({}, {}, NULL, {}, {})",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                        KIND.placeholder(4),
                    );
                    let now = DateTime::from_timestamp(1_800_000_000, 0).expect("representable");
                    sqlx::query(AssertSqlSafe(statement))
                        .bind(id.as_bytes().as_slice())
                        .bind(format!("{id}@refresh.test"))
                        .bind(now)
                        .bind(now)
                        .execute(self.database.pool())
                        .await
                        .expect("creates a user");
                    id
                }

                async fn stored_token_hashes(&self) -> Vec<Vec<u8>> {
                    sqlx::query_scalar(AssertSqlSafe(
                        "SELECT token_hash FROM rv_auth_refresh".to_owned(),
                    ))
                    .fetch_all(self.database.pool())
                    .await
                    .expect("reads the stored digests")
                }

                async fn tokens_in(&self, family: FamilyId) -> Vec<StoredRefreshToken> {
                    let select = format!(
                        "SELECT token_hash, expires_at, consumed_at, revoked_at, replaced_by FROM rv_auth_refresh WHERE family_id = {} ORDER BY issued_at ASC, id ASC",
                        KIND.placeholder(1)
                    );
                    let rows: Vec<(
                        Vec<u8>,
                        DateTime<Utc>,
                        Option<DateTime<Utc>>,
                        Option<DateTime<Utc>>,
                        Option<Vec<u8>>,
                    )> = sqlx::query_as(AssertSqlSafe(select))
                        .bind(family.as_bytes().as_slice())
                        .fetch_all(self.database.pool())
                        .await
                        .expect("reads the family's tokens");
                    rows.into_iter()
                        .map(
                            |(hash, expires_at, consumed_at, revoked_at, replaced_by)| {
                                StoredRefreshToken {
                                    digest: SecretDigest::from_bytes(
                                        hash.try_into().expect("a 32-byte digest"),
                                    ),
                                    expires_at,
                                    consumed_at,
                                    revoked_at,
                                    replaced_by: replaced_by.map(|bytes| {
                                        RefreshTokenId::from_bytes(
                                            bytes.try_into().expect("a 16-byte identifier"),
                                        )
                                    }),
                                }
                            },
                        )
                        .collect()
                }

                async fn family_revoked_at(&self, family: FamilyId) -> Option<DateTime<Utc>> {
                    let select = format!(
                        "SELECT revoked_at FROM rv_auth_refresh_family WHERE id = {}",
                        KIND.placeholder(1)
                    );
                    let row: Option<(Option<DateTime<Utc>>,)> =
                        sqlx::query_as(AssertSqlSafe(select))
                            .bind(family.as_bytes().as_slice())
                            .fetch_optional(self.database.pool())
                            .await
                            .expect("reads the tombstone");
                    row.and_then(|(revoked_at,)| revoked_at)
                }

                async fn family_scope_claim(&self, family: FamilyId) -> Option<String> {
                    let select = format!(
                        "SELECT scopes FROM rv_auth_refresh_family WHERE id = {}",
                        KIND.placeholder(1)
                    );
                    sqlx::query_scalar(AssertSqlSafe(select))
                        .bind(family.as_bytes().as_slice())
                        .fetch_optional(self.database.pool())
                        .await
                        .expect("reads the stored claim")
                }

                async fn family_user(&self, family: FamilyId) -> Option<UserId> {
                    let select = format!(
                        "SELECT user_id FROM rv_auth_refresh_family WHERE id = {}",
                        KIND.placeholder(1)
                    );
                    let row: Option<(Vec<u8>,)> = sqlx::query_as(AssertSqlSafe(select))
                        .bind(family.as_bytes().as_slice())
                        .fetch_optional(self.database.pool())
                        .await
                        .expect("reads the stored subject");
                    row.map(|(bytes,)| {
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
                let repository = SqlxRefreshTokenRepository::new(database.pool().clone());
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
    renvor_sqlx::connect_postgres,
    run_postgres,
    support::POSTGRES_URL,
    "postgres",
    renvor_database::DatabaseKind::Postgres
);
refresh_suite!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    renvor_sqlx::connect_mysql,
    run_mysql,
    support::MYSQL_URL,
    "mysql",
    renvor_database::DatabaseKind::MySql
);
