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
use renvor_auth::session::{SessionHandle, SessionRecord, SessionRepository};
use renvor_auth::subject::UserId;
use renvor_database::{DatabaseError, DatabaseErrorKind, DatabaseKind};

#[cfg(feature = "tokens")]
use renvor_auth::refresh::{
    AdvanceRequest, FamilyId, NewRefreshFamily, RefreshGrant, RefreshTransition,
};
#[cfg(feature = "tokens")]
use renvor_auth::repository::RefreshTokenRepository;
#[cfg(feature = "tokens")]
use renvor_auth::token::ScopeSet;

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

/// Rebuilds a `SecretDigest` from the stored `token_hash`.
pub(crate) fn digest_from(bytes: &[u8]) -> Result<SecretDigest, DatabaseError> {
    let sized: [u8; 32] = bytes
        .try_into()
        .map_err(|_| crate::error::record(DatabaseErrorKind::StatementRejected))?;
    Ok(SecretDigest::from_bytes(sized))
}

/// Rebuilds a `UserId` from stored bytes.
pub(crate) fn user_id_from(bytes: &[u8]) -> Result<UserId, DatabaseError> {
    let sized: [u8; 16] = bytes
        .try_into()
        .map_err(|_| crate::error::record(DatabaseErrorKind::StatementRejected))?;
    Ok(UserId::from_bytes(sized))
}

/// Rebuilds a `FamilyId` from stored bytes.
#[cfg(feature = "tokens")]
pub(crate) fn family_id_from(bytes: &[u8]) -> Result<FamilyId, DatabaseError> {
    let sized: [u8; FamilyId::BYTES] = bytes
        .try_into()
        .map_err(|_| crate::error::record(DatabaseErrorKind::StatementRejected))?;
    Ok(FamilyId::from_bytes(sized))
}

/// Rebuilds a `ScopeSet` from the stored claim.
///
/// **Fails closed, and the bound is not this function's.** `ScopeSet::parse_claim` refuses a claim
/// with an over-long scope or more than `ScopeSet::MAX_SCOPES` of them — the same bound the value
/// had when it was written. A stored claim that no longer parses is a corrupted or hand-edited
/// row, and the answer is a refused read rather than a grant assembled from whatever parsed: there
/// is no default scope here, and inventing one would be inventing an authorisation.
#[cfg(feature = "tokens")]
pub(crate) fn scopes_from(claim: &str) -> Result<ScopeSet, DatabaseError> {
    ScopeSet::parse_claim(claim)
        .map_err(|_| crate::error::record(DatabaseErrorKind::StatementRejected))
}

macro_rules! auth_repositories {
    ($module:ident, $feature:literal, $driver:ty, $kind:expr, $engine_doc:literal) => {
        #[cfg(feature = $feature)]
        #[doc = $engine_doc]
        pub mod $module {
            use super::{
                Arc, CredentialRecord, CredentialRepository, DatabaseError, DatabaseErrorKind,
                DatabaseKind, DateTime, PasswordHash, Registration, SeaOrmDatabase, SecretDigest,
                SessionHandle, SessionRecord, SessionRepository, SingleUseTokenRepository,
                TokenTable, UserId, UserRecord, UserRepository, Utc, classify_db_error, digest_from,
                rand_bytes, user_id_from,
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

            /// Reads and writes `rv_auth_session`.
            #[derive(Clone, Debug)]
            pub struct SeaOrmSessionRepository {
                database: Arc<SeaOrmDatabase<$driver>>,
            }

            impl SeaOrmSessionRepository {
                /// Wraps a database handle.
                #[must_use]
                pub const fn new(database: Arc<SeaOrmDatabase<$driver>>) -> Self {
                    Self { database }
                }
            }

            impl SessionRepository for SeaOrmSessionRepository {
                async fn create(
                    &self,
                    user_id: UserId,
                    digest: &SecretDigest,
                    now: DateTime<Utc>,
                ) -> Result<(), DatabaseError> {
                    let connection = self.database.acquire().await?;
                    let id = UserId::from_bytes(rand_bytes()?);
                    let statement = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &format!(
                            "INSERT INTO rv_auth_session (id, user_id, token_hash, created_at, last_seen_at, revoked_at) VALUES ({}, {}, {}, {}, {}, NULL)",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                            KIND.placeholder(3),
                            KIND.placeholder(4),
                            KIND.placeholder(5),
                        ),
                        [
                            id.as_bytes().to_vec().into(),
                            user_id.as_bytes().to_vec().into(),
                            digest.as_bytes().to_vec().into(),
                            now.into(),
                            now.into(),
                        ],
                    );
                    connection
                        .execute_raw(statement)
                        .await
                        .map(|_| ())
                        .map_err(|error| classify_db_error(&error))
                }

                async fn touch(
                    &self,
                    digest: &SecretDigest,
                    now: DateTime<Utc>,
                    idle_cutoff: DateTime<Utc>,
                    absolute_cutoff: DateTime<Utc>,
                ) -> Result<Option<SessionRecord>, DatabaseError> {
                    // ONE STATEMENT carries the liveness predicate; see the SQLx twin for why
                    // `rows_affected` is trustworthy on MySQL here.
                    let connection = self.database.acquire().await?;
                    let update = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &format!(
                            "UPDATE rv_auth_session SET last_seen_at = {} WHERE token_hash = {} AND revoked_at IS NULL AND last_seen_at > {} AND created_at > {}",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                            KIND.placeholder(3),
                            KIND.placeholder(4),
                        ),
                        [
                            now.into(),
                            digest.as_bytes().to_vec().into(),
                            idle_cutoff.into(),
                            absolute_cutoff.into(),
                        ],
                    );
                    let affected = connection
                        .execute_raw(update)
                        .await
                        .map_err(|error| classify_db_error(&error))?
                        .rows_affected();
                    if affected == 0 {
                        // Unknown, revoked, idle, or too old — ONE answer for all four.
                        return Ok(None);
                    }

                    let select = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &format!(
                            "SELECT user_id, created_at, last_seen_at FROM rv_auth_session WHERE token_hash = {}",
                            KIND.placeholder(1)
                        ),
                        [digest.as_bytes().to_vec().into()],
                    );
                    let Some(row) = connection
                        .query_one_raw(select)
                        .await
                        .map_err(|error| classify_db_error(&error))?
                    else {
                        return Ok(None);
                    };
                    let id: Vec<u8> = row
                        .try_get("", "user_id")
                        .map_err(|error| classify_db_error(&error))?;
                    let created_at: DateTime<Utc> = row
                        .try_get("", "created_at")
                        .map_err(|error| classify_db_error(&error))?;
                    let last_seen_at: DateTime<Utc> = row
                        .try_get("", "last_seen_at")
                        .map_err(|error| classify_db_error(&error))?;
                    Ok(Some(SessionRecord {
                        user_id: user_id_from(&id)?,
                        created_at,
                        last_seen_at,
                    }))
                }

                async fn revoke(
                    &self,
                    digest: &SecretDigest,
                    now: DateTime<Utc>,
                ) -> Result<bool, DatabaseError> {
                    // `revoked_at IS NULL` makes this genuinely conditional, so exactly one of two
                    // concurrent logouts sees `true`.
                    let connection = self.database.acquire().await?;
                    let statement = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &format!(
                            "UPDATE rv_auth_session SET revoked_at = {} WHERE token_hash = {} AND revoked_at IS NULL",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                        ),
                        [now.into(), digest.as_bytes().to_vec().into()],
                    );
                    connection
                        .execute_raw(statement)
                        .await
                        .map(|done| done.rows_affected() == 1)
                        .map_err(|error| classify_db_error(&error))
                }

                async fn revoke_all_for(
                    &self,
                    user_id: UserId,
                    now: DateTime<Utc>,
                ) -> Result<u64, DatabaseError> {
                    let connection = self.database.acquire().await?;
                    let statement = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &format!(
                            "UPDATE rv_auth_session SET revoked_at = {} WHERE user_id = {} AND revoked_at IS NULL",
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

                async fn live_for(
                    &self,
                    user_id: UserId,
                    idle_cutoff: DateTime<Utc>,
                    absolute_cutoff: DateTime<Utc>,
                ) -> Result<Vec<SessionHandle>, DatabaseError> {
                    // The ORDER BY is the eviction order, and it is part of the port's contract.
                    let connection = self.database.acquire().await?;
                    let select = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &format!(
                            "SELECT token_hash, last_seen_at FROM rv_auth_session WHERE user_id = {} AND revoked_at IS NULL AND last_seen_at > {} AND created_at > {} ORDER BY last_seen_at ASC",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                            KIND.placeholder(3),
                        ),
                        [
                            user_id.as_bytes().to_vec().into(),
                            idle_cutoff.into(),
                            absolute_cutoff.into(),
                        ],
                    );
                    let rows = connection
                        .query_all_raw(select)
                        .await
                        .map_err(|error| classify_db_error(&error))?;
                    rows.into_iter()
                        .map(|row| {
                            let hash: Vec<u8> = row
                                .try_get("", "token_hash")
                                .map_err(|error| classify_db_error(&error))?;
                            let last_seen_at: DateTime<Utc> = row
                                .try_get("", "last_seen_at")
                                .map_err(|error| classify_db_error(&error))?;
                            Ok(SessionHandle {
                                digest: digest_from(&hash)?,
                                last_seen_at,
                            })
                        })
                        .collect()
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

            #[cfg(feature = "tokens")]
            use super::{
                AdvanceRequest, FamilyId, NewRefreshFamily, RefreshGrant, RefreshTokenRepository,
                RefreshTransition, family_id_from, scopes_from,
            };

            /// Reads and writes `rv_auth_refresh_family` and `rv_auth_refresh`.
            ///
            /// # The second operation here that needs a transaction, and the reason
            ///
            /// `upsert` needs one because it writes twice. `advance` needs one because a
            /// revalidation, a consume and an insert must have **nothing able to land between
            /// them** — a weaker guarantee than "these two writes are one write", and the whole of
            /// this port's contract. It takes `Database::begin`, which is `SeaOrmUnitOfWork` and
            /// therefore Renvor's cancellation discipline rather than SeaORM's.
            #[cfg(feature = "tokens")]
            #[derive(Clone, Debug)]
            pub struct SeaOrmRefreshTokenRepository {
                database: Arc<SeaOrmDatabase<$driver>>,
            }

            #[cfg(feature = "tokens")]
            impl SeaOrmRefreshTokenRepository {
                /// Wraps a database handle.
                #[must_use]
                pub const fn new(database: Arc<SeaOrmDatabase<$driver>>) -> Self {
                    Self { database }
                }

                /// The transition itself. Called inside the transaction; the caller commits.
                ///
                /// # The lock order is family, then token, and never the other way
                ///
                /// Two transitions on one family take the same two locks in the same sequence, so
                /// the second queues behind the first rather than deadlocking against it. The
                /// family lock is also what makes the tombstone check meaningful: a concurrent
                /// replay response cannot set it between the check below and the insert at the
                /// end, because it cannot acquire the row until this transaction commits.
                async fn decide(
                    &self,
                    unit: &crate::SeaOrmUnitOfWork<'_, $driver>,
                    request: &AdvanceRequest<'_>,
                ) -> Result<RefreshTransition, DatabaseError> {
                    let backend = unit.get_database_backend();

                    // 1. Which family. `family_id` is written once and never updated, so reading it
                    //    without a lock cannot go stale in a direction that matters — and every
                    //    decision below is taken again with the row locked.
                    let locate = sea_orm::Statement::from_sql_and_values(
                        backend,
                        &format!(
                            "SELECT family_id FROM rv_auth_refresh WHERE token_hash = {}",
                            KIND.placeholder(1)
                        ),
                        [request.presented.as_bytes().to_vec().into()],
                    );
                    let Some(located) = unit
                        .query_one_raw(locate)
                        .await
                        .map_err(|error| classify_db_error(&error))?
                    else {
                        // An unknown digest. Nothing is written, and no other family is touched.
                        return Ok(RefreshTransition::Unusable);
                    };
                    let family_bytes: Vec<u8> = located
                        .try_get("", "family_id")
                        .map_err(|error| classify_db_error(&error))?;
                    let family = family_id_from(&family_bytes)?;

                    // 2. THE FAMILY LOCK.
                    let lock_family = sea_orm::Statement::from_sql_and_values(
                        backend,
                        &format!(
                            "SELECT expires_at, revoked_at FROM rv_auth_refresh_family WHERE id = {} FOR UPDATE",
                            KIND.placeholder(1)
                        ),
                        [family.as_bytes().to_vec().into()],
                    );
                    let Some(family_row) = unit
                        .query_one_raw(lock_family)
                        .await
                        .map_err(|error| classify_db_error(&error))?
                    else {
                        return Ok(RefreshTransition::Unusable);
                    };
                    let family_expires_at: DateTime<Utc> = family_row
                        .try_get("", "expires_at")
                        .map_err(|error| classify_db_error(&error))?;
                    let family_revoked_at: Option<DateTime<Utc>> = family_row
                        .try_get("", "revoked_at")
                        .map_err(|error| classify_db_error(&error))?;
                    if family_revoked_at.is_some() {
                        // A TOMBSTONE IS FINAL. Nothing may be inserted into this family again.
                        return Ok(RefreshTransition::FamilyRevoked);
                    }
                    if family_expires_at <= request.now {
                        return Ok(RefreshTransition::Unusable);
                    }

                    // 3. THE TOKEN LOCK, taken second and always second.
                    let lock_token = sea_orm::Statement::from_sql_and_values(
                        backend,
                        &format!(
                            "SELECT id, expires_at, consumed_at, revoked_at FROM rv_auth_refresh WHERE token_hash = {} FOR UPDATE",
                            KIND.placeholder(1)
                        ),
                        [request.presented.as_bytes().to_vec().into()],
                    );
                    let Some(token_row) = unit
                        .query_one_raw(lock_token)
                        .await
                        .map_err(|error| classify_db_error(&error))?
                    else {
                        return Ok(RefreshTransition::Unusable);
                    };
                    let token_id: Vec<u8> = token_row
                        .try_get("", "id")
                        .map_err(|error| classify_db_error(&error))?;
                    let token_expires_at: DateTime<Utc> = token_row
                        .try_get("", "expires_at")
                        .map_err(|error| classify_db_error(&error))?;
                    let consumed_at: Option<DateTime<Utc>> = token_row
                        .try_get("", "consumed_at")
                        .map_err(|error| classify_db_error(&error))?;
                    let revoked_at: Option<DateTime<Utc>> = token_row
                        .try_get("", "revoked_at")
                        .map_err(|error| classify_db_error(&error))?;

                    if consumed_at.is_some() || revoked_at.is_some() {
                        // THE REPLAY RESPONSE — ASVS V10.4.5, inside the transaction that detected
                        // it. Both writes are conditional on `revoked_at IS NULL`, which makes the
                        // tombstone monotonic AND makes `rows_affected` mean the same thing on both
                        // engines: MySQL reports rows *changed*, so a write that set a column to
                        // the value it already held would report zero and the count would be a lie.
                        let tombstone = sea_orm::Statement::from_sql_and_values(
                            backend,
                            &format!(
                                "UPDATE rv_auth_refresh_family SET revoked_at = {} WHERE id = {} AND revoked_at IS NULL",
                                KIND.placeholder(1),
                                KIND.placeholder(2),
                            ),
                            [request.now.into(), family.as_bytes().to_vec().into()],
                        );
                        unit.execute_raw(tombstone)
                            .await
                            .map_err(|error| classify_db_error(&error))?;

                        let sweep = sea_orm::Statement::from_sql_and_values(
                            backend,
                            &format!(
                                "UPDATE rv_auth_refresh SET revoked_at = {} WHERE family_id = {} AND revoked_at IS NULL",
                                KIND.placeholder(1),
                                KIND.placeholder(2),
                            ),
                            [request.now.into(), family.as_bytes().to_vec().into()],
                        );
                        let revoked = unit
                            .execute_raw(sweep)
                            .await
                            .map_err(|error| classify_db_error(&error))?
                            .rows_affected();
                        return Ok(RefreshTransition::Replayed { revoked });
                    }
                    if token_expires_at <= request.now {
                        // NOT a replay. An expired token revokes nothing.
                        return Ok(RefreshTransition::Unusable);
                    }

                    // 4. Consume, conditionally. C-16 §3 requires a read-modify-write to lock the
                    //    row it read *or* state a condition that fails if the row changed. This
                    //    does both.
                    let consume = sea_orm::Statement::from_sql_and_values(
                        backend,
                        &format!(
                            "UPDATE rv_auth_refresh SET consumed_at = {}, replaced_by = {} WHERE id = {} AND consumed_at IS NULL",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                            KIND.placeholder(3),
                        ),
                        [
                            request.now.into(),
                            request.successor_id.as_bytes().to_vec().into(),
                            token_id.into(),
                        ],
                    );
                    let affected = unit
                        .execute_raw(consume)
                        .await
                        .map_err(|error| classify_db_error(&error))?
                        .rows_affected();
                    if affected != 1 {
                        // Unreachable while the lock above is held, which is exactly why it is
                        // refused rather than assumed: if it ever happens the lock is not doing
                        // what this function claims, and a silent success would hide that.
                        return Err(crate::error::record(DatabaseErrorKind::StatementRejected));
                    }

                    // 5. The successor, in the SAME transaction. This is the statement the old
                    //    three-call design ran after the transaction that consumed its predecessor
                    //    had already committed, which is how a revoked family got a live token.
                    let insert = sea_orm::Statement::from_sql_and_values(
                        backend,
                        &format!(
                            "INSERT INTO rv_auth_refresh (id, family_id, token_hash, issued_at, expires_at, consumed_at, replaced_by, revoked_at) VALUES ({}, NULL, NULL, NULL)",
                            placeholders(5)
                        ),
                        [
                            request.successor_id.as_bytes().to_vec().into(),
                            family.as_bytes().to_vec().into(),
                            request.successor.as_bytes().to_vec().into(),
                            request.now.into(),
                            request.successor_expires_at.into(),
                        ],
                    );
                    unit.execute_raw(insert)
                        .await
                        .map_err(|error| classify_db_error(&error))?;
                    Ok(RefreshTransition::Advanced)
                }

                /// The tombstone and the sweep, in one transaction.
                ///
                /// One transaction rather than two statements so that no reader can observe a
                /// family whose tombstone is set while its tokens are still live — the state the
                /// old design left behind permanently.
                async fn revoke_within(
                    unit: &crate::SeaOrmUnitOfWork<'_, $driver>,
                    family: FamilyId,
                    now: DateTime<Utc>,
                ) -> Result<u64, DatabaseError> {
                    let backend = unit.get_database_backend();
                    let tombstone = sea_orm::Statement::from_sql_and_values(
                        backend,
                        &format!(
                            "UPDATE rv_auth_refresh_family SET revoked_at = {} WHERE id = {} AND revoked_at IS NULL",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                        ),
                        [now.into(), family.as_bytes().to_vec().into()],
                    );
                    unit.execute_raw(tombstone)
                        .await
                        .map_err(|error| classify_db_error(&error))?;

                    let sweep = sea_orm::Statement::from_sql_and_values(
                        backend,
                        &format!(
                            "UPDATE rv_auth_refresh SET revoked_at = {} WHERE family_id = {} AND revoked_at IS NULL",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                        ),
                        [now.into(), family.as_bytes().to_vec().into()],
                    );
                    unit.execute_raw(sweep)
                        .await
                        .map(|done| done.rows_affected())
                        .map_err(|error| classify_db_error(&error))
                }
            }

            #[cfg(feature = "tokens")]
            impl RefreshTokenRepository for SeaOrmRefreshTokenRepository {
                async fn begin_family(
                    &self,
                    family: NewRefreshFamily,
                ) -> Result<(), DatabaseError> {
                    use renvor_database::{Database as _, UnitOfWork as _};
                    // BOTH OR NEITHER. A family with no token is an authorisation nobody can
                    // exercise; a token with no family has no grant and no tombstone.
                    let unit = self.database.begin().await?;
                    let backend = unit.get_database_backend();

                    let insert_family = sea_orm::Statement::from_sql_and_values(
                        backend,
                        &format!(
                            "INSERT INTO rv_auth_refresh_family (id, user_id, scopes, created_at, expires_at, revoked_at) VALUES ({}, NULL)",
                            placeholders(5)
                        ),
                        [
                            family.family.as_bytes().to_vec().into(),
                            family.user.as_bytes().to_vec().into(),
                            // THE CANONICAL CLAIM, and the only copy of the granted scopes. Sorted
                            // and space-delimited by `ScopeSet::to_claim`, which is also what the
                            // access token's `scope` claim carries — one spelling, two destinations.
                            family.scopes.to_claim().into(),
                            family.created_at.into(),
                            family.family_expires_at.into(),
                        ],
                    );
                    unit.execute_raw(insert_family)
                        .await
                        .map_err(|error| classify_db_error(&error))?;

                    let insert_token = sea_orm::Statement::from_sql_and_values(
                        backend,
                        &format!(
                            "INSERT INTO rv_auth_refresh (id, family_id, token_hash, issued_at, expires_at, consumed_at, replaced_by, revoked_at) VALUES ({}, NULL, NULL, NULL)",
                            placeholders(5)
                        ),
                        [
                            family.first_token_id.as_bytes().to_vec().into(),
                            family.family.as_bytes().to_vec().into(),
                            family.first_token.as_bytes().to_vec().into(),
                            family.created_at.into(),
                            family.token_expires_at.into(),
                        ],
                    );
                    unit.execute_raw(insert_token)
                        .await
                        .map_err(|error| classify_db_error(&error))?;

                    unit.commit().await
                }

                async fn grant_for(
                    &self,
                    digest: &SecretDigest,
                ) -> Result<Option<RefreshGrant>, DatabaseError> {
                    // NO LOCK AND NO TRANSACTION. This decides nothing — see the port. Every
                    // column it reads is written once and never updated, so the answer cannot go
                    // stale, and `advance` re-decides all of it under the family lock anyway.
                    let connection = self.database.acquire().await?;
                    let statement = sea_orm::Statement::from_sql_and_values(
                        connection.get_database_backend(),
                        &format!(
                            "SELECT f.id, f.user_id, f.scopes, f.expires_at FROM rv_auth_refresh t JOIN rv_auth_refresh_family f ON f.id = t.family_id WHERE t.token_hash = {}",
                            KIND.placeholder(1)
                        ),
                        [digest.as_bytes().to_vec().into()],
                    );
                    let Some(row) = connection
                        .query_one_raw(statement)
                        .await
                        .map_err(|error| classify_db_error(&error))?
                    else {
                        return Ok(None);
                    };
                    let family: Vec<u8> = row
                        .try_get("", "id")
                        .map_err(|error| classify_db_error(&error))?;
                    let user: Vec<u8> = row
                        .try_get("", "user_id")
                        .map_err(|error| classify_db_error(&error))?;
                    let scopes: String = row
                        .try_get("", "scopes")
                        .map_err(|error| classify_db_error(&error))?;
                    let family_expires_at: DateTime<Utc> = row
                        .try_get("", "expires_at")
                        .map_err(|error| classify_db_error(&error))?;
                    Ok(Some(RefreshGrant {
                        family: family_id_from(&family)?,
                        user: user_id_from(&user)?,
                        scopes: scopes_from(&scopes)?,
                        family_expires_at,
                    }))
                }

                async fn advance(
                    &self,
                    request: AdvanceRequest<'_>,
                ) -> Result<RefreshTransition, DatabaseError> {
                    use renvor_database::{Database as _, UnitOfWork as _};
                    let unit = self.database.begin().await?;
                    match self.decide(&unit, &request).await {
                        Ok(transition) => {
                            // COMMITTED even for a refusal: a refusal wrote nothing, so this is a
                            // no-op, and one exit path keeps the replay branch — which DID write —
                            // from ever reaching a rollback by accident.
                            unit.commit().await?;
                            Ok(transition)
                        }
                        Err(error) => {
                            // The whole transition is undone. A consumed predecessor with no
                            // successor is a silently ended session; a successor with a live
                            // predecessor is two valid tokens where the design promises one.
                            let _ = unit.rollback().await;
                            Err(error)
                        }
                    }
                }

                async fn revoke_family(
                    &self,
                    family: FamilyId,
                    now: DateTime<Utc>,
                ) -> Result<u64, DatabaseError> {
                    use renvor_database::{Database as _, UnitOfWork as _};
                    let unit = self.database.begin().await?;
                    match Self::revoke_within(&unit, family, now).await {
                        Ok(revoked) => {
                            unit.commit().await?;
                            Ok(revoked)
                        }
                        Err(error) => {
                            let _ = unit.rollback().await;
                            Err(error)
                        }
                    }
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
