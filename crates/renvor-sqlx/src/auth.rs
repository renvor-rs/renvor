//! The direct-SQLx implementations of `renvor-auth`'s persistence ports.
//!
//! # Why the SQL is built rather than written once
//!
//! PostgreSQL numbers its placeholders (`$1`) and MySQL does not (`?`). That is not a formatting
//! difference — it changes the *text* of every statement with a bound parameter, so one literal
//! cannot serve both engines. `renvor_database::DatabaseKind::placeholder` is the single place that
//! knows the rule, and every statement below composes its text from it.
//!
//! **Nothing untrusted reaches a statement.** Every value is bound; the only interpolation is the
//! placeholder itself, which comes from a closed enum and cannot carry caller input.
//!
//! # A duplicate registration is an outcome, not an error
//!
//! `register` lets the **database** decide. `contracts/database-portability.md` §3 forbids relying
//! on the default isolation level and the two engines differ — PostgreSQL `READ COMMITTED`, MySQL
//! `REPEATABLE READ` — so a check-then-insert produces different answers on different rows. The
//! unique constraint on `email` makes the answer the same everywhere, and a `UniqueViolation` is
//! translated into [`Registration::AlreadyRegistered`] rather than propagated.

use chrono::{DateTime, Utc};
use renvor_auth::opaque::SecretDigest;
use renvor_auth::password::PasswordHash;
use renvor_auth::repository::{
    CredentialRecord, CredentialRepository, Registration, SingleUseTokenRepository, UserRecord,
    UserRepository,
};
use renvor_auth::subject::UserId;
use renvor_database::{DatabaseError, DatabaseErrorKind, DatabaseKind};

use crate::error::classify_error;

/// Which single-use token table a repository addresses.
///
/// A closed set naming the two tables, so a caller cannot point a reset consumer at the
/// verification table. The table name reaches the SQL from **this enum only** — never from a
/// caller — which is what keeps the composed statements free of untrusted text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum TokenTable {
    /// `rv_auth_verification`.
    Verification,
    /// `rv_auth_password_reset`.
    PasswordReset,
}

impl TokenTable {
    /// The table this addresses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verification => "rv_auth_verification",
            Self::PasswordReset => "rv_auth_password_reset",
        }
    }
}

/// Generates the repositories for one engine.
macro_rules! auth_repositories {
    ($module:ident, $feature:literal, $driver:ty, $kind:expr, $engine_doc:literal) => {
        #[cfg(feature = $feature)]
        #[doc = $engine_doc]
        pub mod $module {
            use super::{
                CredentialRecord, CredentialRepository, DatabaseError, DatabaseErrorKind,
                DatabaseKind, DateTime, PasswordHash, Registration, SecretDigest,
                SingleUseTokenRepository, TokenTable, UserId, UserRecord, UserRepository, Utc,
                classify_error, into_user, rand_bytes, user_id_from,
            };

            /// This engine's placeholder rule.
            const KIND: DatabaseKind = $kind;

            /// Builds `n` placeholders, one-based, joined for a `VALUES` list.
            fn placeholders(n: usize) -> String {
                (1..=n)
                    .map(|index| KIND.placeholder(index))
                    .collect::<Vec<_>>()
                    .join(", ")
            }

            /// Reads and writes `rv_auth_user`.
            #[derive(Clone, Debug)]
            pub struct SqlxUserRepository {
                pool: sqlx::Pool<$driver>,
            }

            impl SqlxUserRepository {
                /// Wraps a pool.
                #[must_use]
                pub const fn new(pool: sqlx::Pool<$driver>) -> Self {
                    Self { pool }
                }
            }

            impl UserRepository for SqlxUserRepository {
                async fn register(
                    &self,
                    email: &str,
                    now: DateTime<Utc>,
                ) -> Result<Registration, DatabaseError> {
                    // The identity is generated here rather than by the database: a sequence would
                    // encode signup order, which is a fact about the user nobody decided to publish.
                    let id = UserId::from_bytes(rand_bytes()?);
                    let statement = format!(
                        "INSERT INTO rv_auth_user (id, email, email_verified_at, created_at, updated_at) VALUES ({}, {}, NULL, {}, {})",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                        KIND.placeholder(4),
                    );
                    let outcome = sqlx::query(sqlx::AssertSqlSafe(statement))
                        .bind(id.as_bytes().as_slice())
                        .bind(email)
                        .bind(now)
                        .bind(now)
                        .execute(&self.pool)
                        .await;

                    match outcome {
                        Ok(_) => Ok(Registration::Created(id)),
                        Err(error) => {
                            let classified = classify_error(&error);
                            // AN EXPECTED OUTCOME, NOT A FAULT. The constraint is what makes the
                            // answer identical on both engines.
                            if classified.kind() == DatabaseErrorKind::UniqueViolation {
                                Ok(Registration::AlreadyRegistered)
                            } else {
                                Err(classified)
                            }
                        }
                    }
                }

                async fn find_by_email(
                    &self,
                    email: &str,
                ) -> Result<Option<UserRecord>, DatabaseError> {
                    let statement = format!(
                        "SELECT id, email, email_verified_at FROM rv_auth_user WHERE email = {}",
                        KIND.placeholder(1)
                    );
                    let row: Option<(Vec<u8>, String, Option<DateTime<Utc>>)> =
                        sqlx::query_as(sqlx::AssertSqlSafe(statement))
                            .bind(email)
                            .fetch_optional(&self.pool)
                            .await
                            .map_err(|error| classify_error(&error))?;
                    row.map(into_user).transpose()
                }

                async fn find_by_id(&self, id: UserId) -> Result<Option<UserRecord>, DatabaseError> {
                    let statement = format!(
                        "SELECT id, email, email_verified_at FROM rv_auth_user WHERE id = {}",
                        KIND.placeholder(1)
                    );
                    let row: Option<(Vec<u8>, String, Option<DateTime<Utc>>)> =
                        sqlx::query_as(sqlx::AssertSqlSafe(statement))
                            .bind(id.as_bytes().as_slice())
                            .fetch_optional(&self.pool)
                            .await
                            .map_err(|error| classify_error(&error))?;
                    row.map(into_user).transpose()
                }
            }

            /// Reads and writes `rv_auth_credential`.
            #[derive(Clone, Debug)]
            pub struct SqlxCredentialRepository {
                pool: sqlx::Pool<$driver>,
            }

            impl SqlxCredentialRepository {
                /// Wraps a pool.
                #[must_use]
                pub const fn new(pool: sqlx::Pool<$driver>) -> Self {
                    Self { pool }
                }
            }

            impl CredentialRepository for SqlxCredentialRepository {
                async fn upsert(
                    &self,
                    user_id: UserId,
                    hash: &PasswordHash,
                    must_change: bool,
                    now: DateTime<Utc>,
                ) -> Result<(), DatabaseError> {
                    // DELETE-then-INSERT rather than an engine-specific upsert. C-16 §4 permits a
                    // portable upsert only on a table whose sole unique key is its primary key —
                    // which this is — but the two engines spell it differently
                    // (`ON CONFLICT` / `ON DUPLICATE KEY`). Two statements in one transaction say
                    // the same thing in one dialect.
                    let mut transaction = self.pool.begin().await.map_err(|e| classify_error(&e))?;

                    let delete = format!(
                        "DELETE FROM rv_auth_credential WHERE user_id = {}",
                        KIND.placeholder(1)
                    );
                    sqlx::query(sqlx::AssertSqlSafe(delete))
                        .bind(user_id.as_bytes().as_slice())
                        .execute(&mut *transaction)
                        .await
                        .map_err(|e| classify_error(&e))?;

                    let insert = format!(
                        "INSERT INTO rv_auth_credential (user_id, password_hash, must_change, updated_at) VALUES ({})",
                        placeholders(4)
                    );
                    sqlx::query(sqlx::AssertSqlSafe(insert))
                        .bind(user_id.as_bytes().as_slice())
                        .bind(hash.as_phc())
                        .bind(must_change)
                        .bind(now)
                        .execute(&mut *transaction)
                        .await
                        .map_err(|e| classify_error(&e))?;

                    transaction.commit().await.map_err(|e| classify_error(&e))
                }

                async fn find(
                    &self,
                    user_id: UserId,
                ) -> Result<Option<CredentialRecord>, DatabaseError> {
                    let statement = format!(
                        "SELECT password_hash, must_change FROM rv_auth_credential WHERE user_id = {}",
                        KIND.placeholder(1)
                    );
                    let row: Option<(String, bool)> =
                        sqlx::query_as(sqlx::AssertSqlSafe(statement))
                            .bind(user_id.as_bytes().as_slice())
                            .fetch_optional(&self.pool)
                            .await
                            .map_err(|error| classify_error(&error))?;
                    Ok(row.map(|(phc, must_change)| CredentialRecord {
                        user_id,
                        password_hash: PasswordHash::from_phc(phc),
                        must_change,
                    }))
                }
            }

            /// Reads and writes one of the single-use token tables.
            #[derive(Clone, Debug)]
            pub struct SqlxSingleUseTokenRepository {
                pool: sqlx::Pool<$driver>,
                table: TokenTable,
            }

            impl SqlxSingleUseTokenRepository {
                /// Wraps a pool, addressing `table`.
                #[must_use]
                pub const fn new(pool: sqlx::Pool<$driver>, table: TokenTable) -> Self {
                    Self { pool, table }
                }
            }

            impl SingleUseTokenRepository for SqlxSingleUseTokenRepository {
                async fn issue(
                    &self,
                    user_id: UserId,
                    digest: &SecretDigest,
                    expires_at: DateTime<Utc>,
                ) -> Result<(), DatabaseError> {
                    let id = UserId::from_bytes(rand_bytes()?);
                    let statement = format!(
                        "INSERT INTO {} (id, user_id, token_hash, expires_at, consumed_at) VALUES ({}, {}, {}, {}, NULL)",
                        self.table.as_str(),
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                        KIND.placeholder(4),
                    );
                    sqlx::query(sqlx::AssertSqlSafe(statement))
                        .bind(id.as_bytes().as_slice())
                        .bind(user_id.as_bytes().as_slice())
                        .bind(digest.as_bytes().as_slice())
                        .bind(expires_at)
                        .execute(&self.pool)
                        .await
                        .map(|_| ())
                        .map_err(|error| classify_error(&error))
                }

                async fn consume(
                    &self,
                    digest: &SecretDigest,
                    now: DateTime<Utc>,
                ) -> Result<Option<UserId>, DatabaseError> {
                    // ONE STATEMENT. The preconditions live in the WHERE clause, so two concurrent
                    // consumers cannot both satisfy it — whatever the isolation level, which
                    // C-16 §3 forbids depending on and which differs between the engines.
                    let update = format!(
                        "UPDATE {} SET consumed_at = {} WHERE token_hash = {} AND consumed_at IS NULL AND expires_at > {}",
                        self.table.as_str(),
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                    );
                    let affected = sqlx::query(sqlx::AssertSqlSafe(update))
                        .bind(now)
                        .bind(digest.as_bytes().as_slice())
                        .bind(now)
                        .execute(&self.pool)
                        .await
                        .map_err(|error| classify_error(&error))?
                        .rows_affected();

                    if affected == 0 {
                        // Unknown, already consumed, or expired — ONE answer for all three.
                        return Ok(None);
                    }

                    let owner = format!(
                        "SELECT user_id FROM {} WHERE token_hash = {}",
                        self.table.as_str(),
                        KIND.placeholder(1)
                    );
                    let row: Option<(Vec<u8>,)> = sqlx::query_as(sqlx::AssertSqlSafe(owner))
                        .bind(digest.as_bytes().as_slice())
                        .fetch_optional(&self.pool)
                        .await
                        .map_err(|error| classify_error(&error))?;
                    row.map(|(bytes,)| user_id_from(&bytes)).transpose()
                }
            }
        }
    };
}

/// Sixteen bytes from the operating system's CSPRNG.
///
/// Goes through `renvor-core`'s entropy port for the reason that port exists: there is **no
/// fallback**, so an identity is never generated from a weaker source.
pub(crate) fn rand_bytes() -> Result<[u8; 16], DatabaseError> {
    use renvor_core::observe::entropy::{EntropySource as _, OsEntropy};
    let mut bytes = [0_u8; 16];
    OsEntropy::new()
        .fill(&mut bytes)
        .map_err(|_| crate::error::record(DatabaseErrorKind::StatementRejected))?;
    Ok(bytes)
}

/// Rebuilds a `UserId` from the stored bytes.
pub(crate) fn user_id_from(bytes: &[u8]) -> Result<UserId, DatabaseError> {
    let sized: [u8; 16] = bytes
        .try_into()
        .map_err(|_| crate::error::record(DatabaseErrorKind::StatementRejected))?;
    Ok(UserId::from_bytes(sized))
}

/// Builds a `UserRecord` from a row.
pub(crate) fn into_user(
    row: (Vec<u8>, String, Option<DateTime<Utc>>),
) -> Result<UserRecord, DatabaseError> {
    let (id, email, email_verified_at) = row;
    Ok(UserRecord {
        id: user_id_from(&id)?,
        email,
        email_verified_at,
    })
}

auth_repositories!(
    postgres,
    "db-postgres",
    sqlx::Postgres,
    DatabaseKind::Postgres,
    "The PostgreSQL repositories. Placeholders are numbered (`$1`), which is why the statements are composed rather than written once."
);
auth_repositories!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    DatabaseKind::MySql,
    "The MySQL repositories. Placeholders are positional (`?`), which is why the statements are composed rather than written once."
);
