//! The SeaORM implementations of `renvor-auth`'s persistence ports.
//!
//! # The same ports, deliberately
//!
//! Every trait implemented here is the one `renvor-sqlx` implements, and the four-row auth suite
//! calls them through the trait rather than through either adapter's own types. That is what makes
//! *"an application swapping `renvor-sqlx` for `renvor-seaorm` does not rewrite its auth code"* a
//! property of the build instead of two suites that happen to agree.
//!
//! # The placeholder rule is a macro argument, and that is not cosmetic
//!
//! `sea_orm::Statement::from_sql_and_values` **passes the SQL through to the driver and rewrites
//! nothing**, so PostgreSQL's `$1` and MySQL's `?` are the caller's problem. `seed.rs` records what
//! happens when this is a `#[cfg]`-selected constant instead: with **both** driver features
//! enabled the PostgreSQL arm won, and the MySQL runner issued `$1`, which MySQL rejects. The
//! four-row matrix caught it twice in one phase.
//!
//! So the rule arrives per-module from `renvor_database::DatabaseKind::placeholder`, the same
//! single source `renvor-sqlx` composes from. **Nothing untrusted reaches a statement**: every
//! value is bound, and the only interpolation is the placeholder and a table name that comes from a
//! closed enum.
//!
//! # Why `acquire` rather than `begin`
//!
//! `SeaOrmConnection` exists *"so that a repository written against `sea_orm::ConnectionTrait`
//! serves both transactional and non-transactional callers without a second implementation"*.
//! These operations are single statements — including the single-use consume, deliberately — so
//! none of them needs a transaction. The one that writes twice, `upsert`, takes one.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use renvor_auth::opaque::SecretDigest;
use renvor_auth::password::PasswordHash;
use renvor_auth::repository::{
    CredentialRecord, CredentialRepository, Registration, SingleUseTokenRepository, UserRecord,
    UserRepository,
};
use renvor_auth::subject::UserId;
use renvor_database::{DatabaseError, DatabaseErrorKind, DatabaseKind};

use crate::SeaOrmDatabase;
use crate::error::classify_db_error;

/// Which single-use token table a repository addresses.
///
/// A closed set. The table name reaches the SQL from **this enum only**, never from a caller, which
/// is what keeps the composed statements free of untrusted text — and it stops a reset consumer
/// being pointed at the verification table.
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

/// Sixteen bytes from the operating system's CSPRNG.
///
/// Through `renvor-core`'s entropy port, for the reason that port exists: there is **no fallback**,
/// so an identity is never generated from a weaker source.
pub(crate) fn rand_bytes() -> Result<[u8; 16], DatabaseError> {
    use renvor_core::observe::entropy::{EntropySource as _, OsEntropy};
    let mut bytes = [0_u8; 16];
    OsEntropy::new()
        .fill(&mut bytes)
        .map_err(|_| crate::error::record(DatabaseErrorKind::StatementRejected))?;
    Ok(bytes)
}

/// Rebuilds a `UserId` from stored bytes.
pub(crate) fn user_id_from(bytes: &[u8]) -> Result<UserId, DatabaseError> {
    let sized: [u8; 16] = bytes
        .try_into()
        .map_err(|_| crate::error::record(DatabaseErrorKind::StatementRejected))?;
    Ok(UserId::from_bytes(sized))
}

macro_rules! auth_repositories {
    ($module:ident, $feature:literal, $driver:ty, $kind:expr, $engine_doc:literal) => {
        #[cfg(feature = $feature)]
        #[doc = $engine_doc]
        pub mod $module {
            use super::{
                Arc, CredentialRecord, CredentialRepository, DatabaseError, DatabaseErrorKind,
                DatabaseKind, DateTime, PasswordHash, Registration, SeaOrmDatabase, SecretDigest,
                SingleUseTokenRepository, TokenTable, UserId, UserRecord, UserRepository, Utc,
                classify_db_error, rand_bytes, user_id_from,
            };
            use sea_orm::ConnectionTrait as _;

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
            pub struct SeaOrmUserRepository {
                database: Arc<SeaOrmDatabase<$driver>>,
            }

            impl SeaOrmUserRepository {
                /// Wraps a database handle.
                #[must_use]
                pub const fn new(database: Arc<SeaOrmDatabase<$driver>>) -> Self {
                    Self { database }
                }
            }

            impl UserRepository for SeaOrmUserRepository {
                async fn register(
                    &self,
                    email: &str,
                    now: DateTime<Utc>,
                ) -> Result<Registration, DatabaseError> {
                    // Generated here rather than by a sequence: a sequence encodes signup order,
                    // which is a fact about the user nobody decided to publish.
                    let id = UserId::from_bytes(rand_bytes()?);
                    let connection = self.database.acquire().await?;
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
                            email.into(),
                            now.into(),
                            now.into(),
                        ],
                    );

                    match connection.execute_raw(statement).await {
                        Ok(_) => Ok(Registration::Created(id)),
                        Err(error) => {
                            let classified = classify_db_error(&error);
                            // AN EXPECTED OUTCOME, NOT A FAULT. The unique constraint is what makes
                            // the answer identical on both engines — C-16 §3 forbids depending on
                            // the isolation level, and these two engines default differently.
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
                    let connection = self.database.acquire().await?;
                    let statement = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &format!(
                            "SELECT id, email, email_verified_at FROM rv_auth_user WHERE email = {}",
                            KIND.placeholder(1)
                        ),
                        [email.into()],
                    );
                    let row = connection
                        .query_one_raw(statement)
                        .await
                        .map_err(|error| classify_db_error(&error))?;
                    row.map(read_user).transpose()
                }

                async fn find_by_id(&self, id: UserId) -> Result<Option<UserRecord>, DatabaseError> {
                    let connection = self.database.acquire().await?;
                    let statement = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &format!(
                            "SELECT id, email, email_verified_at FROM rv_auth_user WHERE id = {}",
                            KIND.placeholder(1)
                        ),
                        [id.as_bytes().to_vec().into()],
                    );
                    let row = connection
                        .query_one_raw(statement)
                        .await
                        .map_err(|error| classify_db_error(&error))?;
                    row.map(read_user).transpose()
                }
            }

            /// Builds a `UserRecord` from a row.
            fn read_user(row: sea_orm::QueryResult) -> Result<UserRecord, DatabaseError> {
                let id: Vec<u8> = row
                    .try_get("", "id")
                    .map_err(|error| classify_db_error(&error))?;
                let email: String = row
                    .try_get("", "email")
                    .map_err(|error| classify_db_error(&error))?;
                let email_verified_at: Option<DateTime<Utc>> = row
                    .try_get("", "email_verified_at")
                    .map_err(|error| classify_db_error(&error))?;
                Ok(UserRecord {
                    id: user_id_from(&id)?,
                    email,
                    email_verified_at,
                })
            }

            /// Reads and writes `rv_auth_credential`.
            #[derive(Clone, Debug)]
            pub struct SeaOrmCredentialRepository {
                database: Arc<SeaOrmDatabase<$driver>>,
            }

            impl SeaOrmCredentialRepository {
                /// Wraps a database handle.
                #[must_use]
                pub const fn new(database: Arc<SeaOrmDatabase<$driver>>) -> Self {
                    Self { database }
                }
            }

            impl CredentialRepository for SeaOrmCredentialRepository {
                async fn upsert(
                    &self,
                    user_id: UserId,
                    hash: &PasswordHash,
                    must_change: bool,
                    now: DateTime<Utc>,
                ) -> Result<(), DatabaseError> {
                    // DELETE-then-INSERT rather than an engine-specific upsert: C-16 §4 permits a
                    // portable upsert only on a table whose sole unique key is its primary key —
                    // which this is — but the two engines spell it differently (`ON CONFLICT` /
                    // `ON DUPLICATE KEY`). Two statements in one transaction say the same thing in
                    // one dialect. **This is the only operation here that needs a transaction.**
                    use renvor_database::{Database as _, UnitOfWork as _};
                    let unit = self.database.begin().await?;
                    let backend = unit.get_database_backend();

                    let delete = sea_orm::Statement::from_sql_and_values(
                        backend,
                        &format!(
                            "DELETE FROM rv_auth_credential WHERE user_id = {}",
                            KIND.placeholder(1)
                        ),
                        [user_id.as_bytes().to_vec().into()],
                    );
                    unit.execute_raw(delete)
                        .await
                        .map_err(|error| classify_db_error(&error))?;

                    let insert = sea_orm::Statement::from_sql_and_values(
                        backend,
                        &format!(
                            "INSERT INTO rv_auth_credential (user_id, password_hash, must_change, updated_at) VALUES ({})",
                            placeholders(4)
                        ),
                        [
                            user_id.as_bytes().to_vec().into(),
                            hash.as_phc().into(),
                            must_change.into(),
                            now.into(),
                        ],
                    );
                    unit.execute_raw(insert)
                        .await
                        .map_err(|error| classify_db_error(&error))?;

                    unit.commit().await
                }

                async fn find(
                    &self,
                    user_id: UserId,
                ) -> Result<Option<CredentialRecord>, DatabaseError> {
                    let connection = self.database.acquire().await?;
                    let statement = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &format!(
                            "SELECT password_hash, must_change FROM rv_auth_credential WHERE user_id = {}",
                            KIND.placeholder(1)
                        ),
                        [user_id.as_bytes().to_vec().into()],
                    );
                    let Some(row) = connection
                        .query_one_raw(statement)
                        .await
                        .map_err(|error| classify_db_error(&error))?
                    else {
                        return Ok(None);
                    };
                    let phc: String = row
                        .try_get("", "password_hash")
                        .map_err(|error| classify_db_error(&error))?;
                    let must_change: bool = row
                        .try_get("", "must_change")
                        .map_err(|error| classify_db_error(&error))?;
                    Ok(Some(CredentialRecord {
                        user_id,
                        password_hash: PasswordHash::from_phc(phc),
                        must_change,
                    }))
                }
            }

            /// Reads and writes one of the single-use token tables.
            #[derive(Clone, Debug)]
            pub struct SeaOrmSingleUseTokenRepository {
                database: Arc<SeaOrmDatabase<$driver>>,
                table: TokenTable,
            }

            impl SeaOrmSingleUseTokenRepository {
                /// Wraps a database handle, addressing `table`.
                #[must_use]
                pub const fn new(
                    database: Arc<SeaOrmDatabase<$driver>>,
                    table: TokenTable,
                ) -> Self {
                    Self { database, table }
                }
            }

            impl SingleUseTokenRepository for SeaOrmSingleUseTokenRepository {
                async fn issue(
                    &self,
                    user_id: UserId,
                    digest: &SecretDigest,
                    expires_at: DateTime<Utc>,
                ) -> Result<(), DatabaseError> {
                    let id = UserId::from_bytes(rand_bytes()?);
                    let connection = self.database.acquire().await?;
                    let statement = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &format!(
                            "INSERT INTO {} (id, user_id, token_hash, expires_at, consumed_at) VALUES ({}, {}, {}, {}, NULL)",
                            self.table.as_str(),
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                            KIND.placeholder(3),
                            KIND.placeholder(4),
                        ),
                        [
                            id.as_bytes().to_vec().into(),
                            user_id.as_bytes().to_vec().into(),
                            digest.as_bytes().to_vec().into(),
                            expires_at.into(),
                        ],
                    );
                    connection
                        .execute_raw(statement)
                        .await
                        .map(|_| ())
                        .map_err(|error| classify_db_error(&error))
                }

                async fn invalidate_all_for(
                    &self,
                    user_id: UserId,
                    now: DateTime<Utc>,
                ) -> Result<u64, DatabaseError> {
                    // See the direct-SQLx implementation for why this marks consumed rather than
                    // deleting, and why `consumed_at IS NULL` is in the predicate.
                    let connection = self.database.acquire().await?;
                    let statement = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &format!(
                            "UPDATE {} SET consumed_at = {} WHERE user_id = {} AND consumed_at IS NULL",
                            self.table.as_str(),
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                        ),
                        [now.into(), user_id.as_bytes().to_vec().into()],
                    );
                    connection
                        .execute_raw(statement)
                        .await
                        .map(|done| done.rows_affected())
                        .map_err(|error| classify_db_error(&error))
                }

                async fn consume(
                    &self,
                    digest: &SecretDigest,
                    now: DateTime<Utc>,
                ) -> Result<Option<UserId>, DatabaseError> {
                    // ONE STATEMENT. The preconditions live in the WHERE clause, so two concurrent
                    // consumers cannot both satisfy it — whatever the isolation level, which C-16
                    // §3 forbids depending on and which these two engines default differently.
                    let connection = self.database.acquire().await?;
                    let update = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &format!(
                            "UPDATE {} SET consumed_at = {} WHERE token_hash = {} AND consumed_at IS NULL AND expires_at > {}",
                            self.table.as_str(),
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                            KIND.placeholder(3),
                        ),
                        [
                            now.into(),
                            digest.as_bytes().to_vec().into(),
                            now.into(),
                        ],
                    );
                    let affected = connection
                        .execute_raw(update)
                        .await
                        .map_err(|error| classify_db_error(&error))?
                        .rows_affected();

                    if affected == 0 {
                        // Unknown, already consumed, or expired — ONE answer for all three.
                        return Ok(None);
                    }

                    let owner = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &format!(
                            "SELECT user_id FROM {} WHERE token_hash = {}",
                            self.table.as_str(),
                            KIND.placeholder(1)
                        ),
                        [digest.as_bytes().to_vec().into()],
                    );
                    let Some(row) = connection
                        .query_one_raw(owner)
                        .await
                        .map_err(|error| classify_db_error(&error))?
                    else {
                        return Ok(None);
                    };
                    let bytes: Vec<u8> = row
                        .try_get("", "user_id")
                        .map_err(|error| classify_db_error(&error))?;
                    user_id_from(&bytes).map(Some)
                }
            }
        }
    };
}

auth_repositories!(
    postgres,
    "db-postgres",
    sqlx::Postgres,
    DatabaseKind::Postgres,
    "The PostgreSQL repositories. Placeholders are numbered (`$1`); `from_sql_and_values` rewrites nothing, so the rule arrives per-module."
);
auth_repositories!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    DatabaseKind::MySql,
    "The MySQL repositories. Placeholders are positional (`?`); `from_sql_and_values` rewrites nothing, so the rule arrives per-module."
);
