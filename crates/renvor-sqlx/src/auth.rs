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
use renvor_auth::abuse::{
    AttemptDimension, AttemptObservation, AttemptOutcome, AttemptRepository, AttemptState,
};
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
    ($module:ident, $feature:literal, $driver:ty, $kind:expr, $upsert_nothing:literal, $engine_doc:literal) => {
        #[cfg(feature = $feature)]
        #[doc = $engine_doc]
        pub mod $module {
            use super::{
                AttemptDimension, AttemptObservation, AttemptOutcome, AttemptRepository,
                AttemptState,
                CredentialRecord, CredentialRepository, DatabaseError, DatabaseErrorKind,
                DatabaseKind, DateTime, PasswordHash, Registration, SecretDigest,
                SessionHandle, SessionRecord, SessionRepository, SingleUseTokenRepository,
                TokenTable, UserId, UserRecord, UserRepository, Utc, classify_error, digest_from,
                into_user, rand_bytes, user_id_from,
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

            /// Reads and writes `rv_auth_session`.
            #[derive(Clone, Debug)]
            pub struct SqlxSessionRepository {
                pool: sqlx::Pool<$driver>,
            }

            impl SqlxSessionRepository {
                /// Wraps a pool.
                #[must_use]
                pub const fn new(pool: sqlx::Pool<$driver>) -> Self {
                    Self { pool }
                }
            }

            impl SessionRepository for SqlxSessionRepository {
                async fn create(
                    &self,
                    user_id: UserId,
                    digest: &SecretDigest,
                    now: DateTime<Utc>,
                ) -> Result<(), DatabaseError> {
                    let id = UserId::from_bytes(rand_bytes()?);
                    let statement = format!(
                        "INSERT INTO rv_auth_session (id, user_id, token_hash, created_at, last_seen_at, revoked_at) VALUES ({}, {}, {}, {}, {}, NULL)",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                        KIND.placeholder(4),
                        KIND.placeholder(5),
                    );
                    sqlx::query(sqlx::AssertSqlSafe(statement))
                        .bind(id.as_bytes().as_slice())
                        .bind(user_id.as_bytes().as_slice())
                        .bind(digest.as_bytes().as_slice())
                        .bind(now)
                        .bind(now)
                        .execute(&self.pool)
                        .await
                        .map(|_| ())
                        .map_err(|error| classify_error(&error))
                }

                async fn touch(
                    &self,
                    digest: &SecretDigest,
                    now: DateTime<Utc>,
                    idle_cutoff: DateTime<Utc>,
                    absolute_cutoff: DateTime<Utc>,
                ) -> Result<Option<SessionRecord>, DatabaseError> {
                    // ONE STATEMENT carries the liveness predicate, so a concurrent revoke cannot
                    // slip between a check and a refresh.
                    //
                    // `rows_affected` is load-bearing here, and on MySQL that is only sound
                    // because `sqlx-mysql` negotiates `CLIENT_FOUND_ROWS` — without it MySQL
                    // reports rows *changed*, and a second request in the same microsecond would
                    // set `last_seen_at` to the value it already held, report zero, and appear to
                    // be a signed-out session. The four-row suite pins that with a test that
                    // touches twice at ONE instant.
                    let update = format!(
                        "UPDATE rv_auth_session SET last_seen_at = {} WHERE token_hash = {} AND revoked_at IS NULL AND last_seen_at > {} AND created_at > {}",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                        KIND.placeholder(4),
                    );
                    let affected = sqlx::query(sqlx::AssertSqlSafe(update))
                        .bind(now)
                        .bind(digest.as_bytes().as_slice())
                        .bind(idle_cutoff)
                        .bind(absolute_cutoff)
                        .execute(&self.pool)
                        .await
                        .map_err(|error| classify_error(&error))?
                        .rows_affected();
                    if affected == 0 {
                        // Unknown, revoked, idle, or too old — ONE answer for all four.
                        return Ok(None);
                    }

                    let select = format!(
                        "SELECT user_id, created_at, last_seen_at FROM rv_auth_session WHERE token_hash = {}",
                        KIND.placeholder(1)
                    );
                    let row: Option<(Vec<u8>, DateTime<Utc>, DateTime<Utc>)> =
                        sqlx::query_as(sqlx::AssertSqlSafe(select))
                            .bind(digest.as_bytes().as_slice())
                            .fetch_optional(&self.pool)
                            .await
                            .map_err(|error| classify_error(&error))?;
                    row.map(|(id, created_at, last_seen_at)| {
                        Ok(SessionRecord {
                            user_id: user_id_from(&id)?,
                            created_at,
                            last_seen_at,
                        })
                    })
                    .transpose()
                }

                async fn revoke(
                    &self,
                    digest: &SecretDigest,
                    now: DateTime<Utc>,
                ) -> Result<bool, DatabaseError> {
                    // `revoked_at IS NULL` makes this genuinely conditional: exactly one of two
                    // concurrent logouts changes the row, so exactly one sees `true`. The SET also
                    // always alters a value, so `rows_affected` is unambiguous on either engine.
                    let statement = format!(
                        "UPDATE rv_auth_session SET revoked_at = {} WHERE token_hash = {} AND revoked_at IS NULL",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                    );
                    sqlx::query(sqlx::AssertSqlSafe(statement))
                        .bind(now)
                        .bind(digest.as_bytes().as_slice())
                        .execute(&self.pool)
                        .await
                        .map(|done| done.rows_affected() == 1)
                        .map_err(|error| classify_error(&error))
                }

                async fn revoke_all_for(
                    &self,
                    user_id: UserId,
                    now: DateTime<Utc>,
                ) -> Result<u64, DatabaseError> {
                    let statement = format!(
                        "UPDATE rv_auth_session SET revoked_at = {} WHERE user_id = {} AND revoked_at IS NULL",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                    );
                    sqlx::query(sqlx::AssertSqlSafe(statement))
                        .bind(now)
                        .bind(user_id.as_bytes().as_slice())
                        .execute(&self.pool)
                        .await
                        .map(|done| done.rows_affected())
                        .map_err(|error| classify_error(&error))
                }

                async fn live_for(
                    &self,
                    user_id: UserId,
                    idle_cutoff: DateTime<Utc>,
                    absolute_cutoff: DateTime<Utc>,
                ) -> Result<Vec<SessionHandle>, DatabaseError> {
                    // The ORDER BY is part of the port's contract, not a convenience: it IS the
                    // eviction order, and a repository returning newest-first would evict the
                    // session the subject is using.
                    let select = format!(
                        "SELECT token_hash, last_seen_at FROM rv_auth_session WHERE user_id = {} AND revoked_at IS NULL AND last_seen_at > {} AND created_at > {} ORDER BY last_seen_at ASC",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                    );
                    let rows: Vec<(Vec<u8>, DateTime<Utc>)> =
                        sqlx::query_as(sqlx::AssertSqlSafe(select))
                            .bind(user_id.as_bytes().as_slice())
                            .bind(idle_cutoff)
                            .bind(absolute_cutoff)
                            .fetch_all(&self.pool)
                            .await
                            .map_err(|error| classify_error(&error))?;
                    rows.into_iter()
                        .map(|(hash, last_seen_at)| {
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

                async fn invalidate_all_for(
                    &self,
                    user_id: UserId,
                    now: DateTime<Utc>,
                ) -> Result<u64, DatabaseError> {
                    // Marks consumed rather than deleting: a consumed row is evidence a token
                    // existed, which an abuse control can count. `consumed_at IS NULL` keeps an
                    // already-consumed row's timestamp — the moment it was USED is the fact worth
                    // keeping, not the moment a later resend swept it.
                    let statement = format!(
                        "UPDATE {} SET consumed_at = {} WHERE user_id = {} AND consumed_at IS NULL",
                        self.table.as_str(),
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                    );
                    sqlx::query(sqlx::AssertSqlSafe(statement))
                        .bind(now)
                        .bind(user_id.as_bytes().as_slice())
                        .execute(&self.pool)
                        .await
                        .map(|done| done.rows_affected())
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

            /// Reads and writes `rv_auth_attempt`: the bounded abuse counters.
            ///
            /// # Why this one holds a pool and opens its own transaction
            ///
            /// The same reason `SqlxRefreshTokenRepository` does. `observe` must increment **and**
            /// report the resulting count with nothing able to land between them, or two concurrent
            /// attempts both read `limit - 1` and both proceed. That is a transaction, not a
            /// statement.
            ///
            /// # The row is created first, then locked
            ///
            /// Three statements, in this order and no other:
            ///
            /// 1. an **upsert that changes nothing** — it exists only to guarantee the row is
            ///    there. `SELECT ... FOR UPDATE` on an absent row locks nothing, so two
            ///    transactions would both find nothing and both insert; one would then fail on the
            ///    primary key. This removes that case instead of retrying it.
            /// 2. `SELECT ... FOR UPDATE`, which now has a row to lock.
            /// 3. `UPDATE`, with the new counts computed **in Rust**.
            ///
            /// `contracts/database-portability.md` §3 forbids depending on the default isolation
            /// level and requires a read-modify-write either to lock the row it read or to state a
            /// condition that fails if the row changed. Step 2 locks it.
            ///
            /// # The arithmetic is deliberately not in SQL
            ///
            /// `SET current_count = current_count + 1` would ask the engine to add, and an engine
            /// that overflows raises — PostgreSQL `22003`, MySQL `1264`. Turning a rate-limit check
            /// on an unauthenticated endpoint into an error is worse than the overflow. Computing
            /// in Rust gives `saturating_add` and a ceiling that fits `BIGINT`.
            #[derive(Clone, Debug)]
            pub struct SqlxAttemptRepository {
                pool: sqlx::Pool<$driver>,
            }

            impl SqlxAttemptRepository {
                /// Wraps a pool.
                #[must_use]
                pub const fn new(pool: sqlx::Pool<$driver>) -> Self {
                    Self { pool }
                }

                /// The whole transition, inside the caller's transaction.
                async fn count_within(
                    uow: &mut crate::SqlxUnitOfWork<'_, $driver>,
                    observation: &AttemptObservation,
                ) -> Result<AttemptOutcome, DatabaseError> {
                    let dimension = observation.dimension.code();
                    // `bucket` is masked into `[0, buckets)` and `buckets <= 2^20`, so this
                    // conversion cannot fail. It is written as a checked one anyway: an
                    // unrepresentable bucket must not become a negative one the CHECK would refuse
                    // with a different error.
                    let bucket = i32::try_from(observation.bucket.get()).map_err(|_| {
                        crate::error::record(DatabaseErrorKind::StatementRejected)
                    })?;

                    // 1. ENSURE THE ROW. Changes nothing when it is already there.
                    let ensure = format!(
                        "INSERT INTO rv_auth_attempt (dimension, bucket, window_start, current_count, previous_count, expires_at) VALUES ({}, {}, {}, 0, 0, {}){}",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                        KIND.placeholder(4),
                        $upsert_nothing,
                    );
                    sqlx::query(sqlx::AssertSqlSafe(ensure))
                        .bind(dimension)
                        .bind(bucket)
                        .bind(observation.window_start)
                        .bind(observation.expires_at)
                        .execute(&mut **uow.inner())
                        .await
                        .map_err(|error| classify_error(&error))?;

                    // 2. THE LOCK.
                    let lock = format!(
                        "SELECT window_start, current_count, previous_count FROM rv_auth_attempt WHERE dimension = {} AND bucket = {} FOR UPDATE",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                    );
                    let row: Option<(DateTime<Utc>, i64, i64)> =
                        sqlx::query_as(sqlx::AssertSqlSafe(lock))
                            .bind(dimension)
                            .bind(bucket)
                            .fetch_optional(&mut **uow.inner())
                            .await
                            .map_err(|error| classify_error(&error))?;
                    let Some((stored_start, stored_current, stored_previous)) = row else {
                        // Unreachable: step 1 guaranteed the row. Refused rather than assumed —
                        // if it ever happens, step 1 is not doing what this function claims, and a
                        // silent success would hide that.
                        return Err(crate::error::record(DatabaseErrorKind::StatementRejected));
                    };

                    // 3. THE DECISION, in Rust.
                    if stored_start > observation.window_start {
                        // A stored instant in the future. Writing would erase evidence, so nothing
                        // is written and the caller refuses.
                        return Ok(AttemptOutcome::ClockRegressed);
                    }
                    let current = u64::try_from(stored_current).unwrap_or(0);
                    let previous = u64::try_from(stored_previous).unwrap_or(0);
                    let (next_current, next_previous) = if stored_start == observation.window_start {
                        (
                            current.saturating_add(1).min(AttemptState::CEILING),
                            previous,
                        )
                    } else if stored_start == observation.previous_window_start {
                        // The window rolled by exactly one: the old count becomes the tail the
                        // weighted estimate charges.
                        (1, current)
                    } else {
                        // A gap of more than one window. There is no tail to charge.
                        (1, 0)
                    };

                    let write = format!(
                        "UPDATE rv_auth_attempt SET window_start = {}, current_count = {}, previous_count = {}, expires_at = {} WHERE dimension = {} AND bucket = {}",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                        KIND.placeholder(4),
                        KIND.placeholder(5),
                        KIND.placeholder(6),
                    );
                    let affected = sqlx::query(sqlx::AssertSqlSafe(write))
                        .bind(observation.window_start)
                        // The ceiling is `i64::MAX`, so this conversion is total.
                        .bind(i64::try_from(next_current).unwrap_or(i64::MAX))
                        .bind(i64::try_from(next_previous).unwrap_or(i64::MAX))
                        .bind(observation.expires_at)
                        .bind(dimension)
                        .bind(bucket)
                        .execute(&mut **uow.inner())
                        .await
                        .map_err(|error| classify_error(&error))?
                        .rows_affected();
                    if affected != 1 {
                        // Unreachable while the lock is held. Same argument as above.
                        return Err(crate::error::record(DatabaseErrorKind::StatementRejected));
                    }

                    Ok(AttemptOutcome::Counted(AttemptState {
                        current: next_current,
                        previous: next_previous,
                        window_start: observation.window_start,
                    }))
                }
            }

            impl AttemptRepository for SqlxAttemptRepository {
                async fn observe(
                    &self,
                    observation: AttemptObservation,
                ) -> Result<AttemptOutcome, DatabaseError> {
                    let mut uow = crate::SqlxUnitOfWork::begin_on(&self.pool, KIND).await?;
                    match Self::count_within(&mut uow, &observation).await {
                        Ok(outcome) => {
                            renvor_database::UnitOfWork::commit(uow).await?;
                            Ok(outcome)
                        }
                        Err(error) => {
                            let _ = renvor_database::UnitOfWork::rollback(uow).await;
                            Err(error)
                        }
                    }
                }

                async fn prune(
                    &self,
                    dimension: AttemptDimension,
                    from: u32,
                    count: u32,
                    now: DateTime<Utc>,
                ) -> Result<u64, DatabaseError> {
                    // A HALF-OPEN BUCKET RANGE, not a `LIMIT`. PostgreSQL has no `LIMIT` on
                    // `DELETE` and MySQL refuses one inside an `IN` subquery, so the two obvious
                    // portable-looking forms are not portable. A range is bounded by construction:
                    // `(dimension, bucket)` is unique, so at most `count` rows match.
                    let upper = u64::from(from).saturating_add(u64::from(count));
                    let statement = format!(
                        "DELETE FROM rv_auth_attempt WHERE dimension = {} AND bucket >= {} AND bucket < {} AND expires_at <= {}",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                        KIND.placeholder(4),
                    );
                    Ok(sqlx::query(sqlx::AssertSqlSafe(statement))
                        .bind(dimension.code())
                        .bind(i64::from(from))
                        .bind(i64::try_from(upper).unwrap_or(i64::MAX))
                        .bind(now)
                        .execute(&self.pool)
                        .await
                        .map_err(|error| classify_error(&error))?
                        .rows_affected())
                }
            }

            #[cfg(feature = "tokens")]
            use super::{
                AdvanceRequest, FamilyId, NewRefreshFamily, RefreshGrant,
                RefreshTokenRepository, RefreshTransition, family_id_from, scopes_from,
            };

            /// Reads and writes `rv_auth_refresh_family` and `rv_auth_refresh`.
            ///
            /// # Why this one holds a pool and opens its own transaction
            ///
            /// Every other repository here runs single statements, so each composes onto whatever
            /// connection it is given. `advance` cannot: its contract is that a revalidation, a
            /// consume and an insert happen with **nothing able to land between them**, and that
            /// is a transaction rather than a statement. It opens one through
            /// `SqlxUnitOfWork::begin_on`, which is the same body `Database::begin` runs — so the
            /// cancellation discipline is inherited rather than reimplemented.
            #[cfg(feature = "tokens")]
            #[derive(Clone, Debug)]
            pub struct SqlxRefreshTokenRepository {
                pool: sqlx::Pool<$driver>,
            }

            #[cfg(feature = "tokens")]
            impl SqlxRefreshTokenRepository {
                /// Wraps a pool.
                #[must_use]
                pub const fn new(pool: sqlx::Pool<$driver>) -> Self {
                    Self { pool }
                }

                /// Writes the family and its first token. Called inside the transaction.
                async fn write_family(
                    uow: &mut crate::SqlxUnitOfWork<'_, $driver>,
                    family: &NewRefreshFamily,
                ) -> Result<(), DatabaseError> {
                    let insert_family = format!(
                        "INSERT INTO rv_auth_refresh_family (id, user_id, scopes, created_at, expires_at, revoked_at) VALUES ({}, {}, {}, {}, {}, NULL)",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                        KIND.placeholder(4),
                        KIND.placeholder(5),
                    );
                    sqlx::query(sqlx::AssertSqlSafe(insert_family))
                        .bind(family.family.as_bytes().as_slice())
                        .bind(family.user.as_bytes().as_slice())
                        // THE CANONICAL CLAIM, and the only copy of the granted scopes. Sorted and
                        // space-delimited by `ScopeSet::to_claim`, which is also what the access
                        // token's `scope` claim carries — one spelling, two destinations.
                        .bind(family.scopes.to_claim())
                        .bind(family.created_at)
                        .bind(family.family_expires_at)
                        .execute(&mut **uow.inner())
                        .await
                        .map_err(|error| classify_error(&error))?;

                    let insert_token = format!(
                        "INSERT INTO rv_auth_refresh (id, family_id, token_hash, issued_at, expires_at, consumed_at, replaced_by, revoked_at) VALUES ({}, {}, {}, {}, {}, NULL, NULL, NULL)",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                        KIND.placeholder(4),
                        KIND.placeholder(5),
                    );
                    sqlx::query(sqlx::AssertSqlSafe(insert_token))
                        .bind(family.first_token_id.as_bytes().as_slice())
                        .bind(family.family.as_bytes().as_slice())
                        .bind(family.first_token.as_bytes().as_slice())
                        .bind(family.created_at)
                        .bind(family.token_expires_at)
                        .execute(&mut **uow.inner())
                        .await
                        .map_err(|error| classify_error(&error))?;
                    Ok(())
                }

                /// The transition itself. Called inside the transaction; the caller commits.
                ///
                /// # The lock order is family, then token, and never the other way
                ///
                /// Two transitions on one family take the same two locks in the same sequence, so
                /// the second queues behind the first rather than deadlocking against it. The
                /// family lock is also what makes the tombstone check meaningful: a concurrent
                /// replay response cannot set it between the check below and the insert at the end,
                /// because it cannot acquire the row until this transaction commits.
                async fn decide(
                    &self,
                    uow: &mut crate::SqlxUnitOfWork<'_, $driver>,
                    request: &AdvanceRequest<'_>,
                ) -> Result<RefreshTransition, DatabaseError> {
                    // 1. Which family. `family_id` is written once and never updated, so reading it
                    //    without a lock cannot go stale in a direction that matters — and every
                    //    decision below is taken again with the row locked.
                    let locate = format!(
                        "SELECT family_id FROM rv_auth_refresh WHERE token_hash = {}",
                        KIND.placeholder(1)
                    );
                    let located: Option<(Vec<u8>,)> = sqlx::query_as(sqlx::AssertSqlSafe(locate))
                        .bind(request.presented.as_bytes().as_slice())
                        .fetch_optional(&mut **uow.inner())
                        .await
                        .map_err(|error| classify_error(&error))?;
                    let Some((family_bytes,)) = located else {
                        // An unknown digest. Nothing is written, and no other family is touched.
                        return Ok(RefreshTransition::Unusable);
                    };
                    let family = family_id_from(&family_bytes)?;

                    // 2. THE FAMILY LOCK.
                    let lock_family = format!(
                        "SELECT expires_at, revoked_at FROM rv_auth_refresh_family WHERE id = {} FOR UPDATE",
                        KIND.placeholder(1)
                    );
                    let family_row: Option<(DateTime<Utc>, Option<DateTime<Utc>>)> =
                        sqlx::query_as(sqlx::AssertSqlSafe(lock_family))
                            .bind(family.as_bytes().as_slice())
                            .fetch_optional(&mut **uow.inner())
                            .await
                            .map_err(|error| classify_error(&error))?;
                    let Some((family_expires_at, family_revoked_at)) = family_row else {
                        return Ok(RefreshTransition::Unusable);
                    };
                    if family_revoked_at.is_some() {
                        // A TOMBSTONE IS FINAL. Nothing may be inserted into this family again.
                        return Ok(RefreshTransition::FamilyRevoked);
                    }
                    if family_expires_at <= request.now {
                        return Ok(RefreshTransition::Unusable);
                    }

                    // 3. THE TOKEN LOCK, taken second and always second.
                    let lock_token = format!(
                        "SELECT id, expires_at, consumed_at, revoked_at FROM rv_auth_refresh WHERE token_hash = {} FOR UPDATE",
                        KIND.placeholder(1)
                    );
                    let token_row: Option<(
                        Vec<u8>,
                        DateTime<Utc>,
                        Option<DateTime<Utc>>,
                        Option<DateTime<Utc>>,
                    )> = sqlx::query_as(sqlx::AssertSqlSafe(lock_token))
                        .bind(request.presented.as_bytes().as_slice())
                        .fetch_optional(&mut **uow.inner())
                        .await
                        .map_err(|error| classify_error(&error))?;
                    let Some((token_id, token_expires_at, consumed_at, revoked_at)) = token_row
                    else {
                        return Ok(RefreshTransition::Unusable);
                    };

                    if consumed_at.is_some() || revoked_at.is_some() {
                        // THE REPLAY RESPONSE — ASVS V10.4.5, inside the transaction that detected
                        // it. Both writes are conditional on `revoked_at IS NULL`, which makes the
                        // tombstone monotonic AND makes `rows_affected` mean the same thing on both
                        // engines: MySQL reports rows *changed*, so a write that set a column to the
                        // value it already held would report zero and the count would be a lie.
                        let tombstone = format!(
                            "UPDATE rv_auth_refresh_family SET revoked_at = {} WHERE id = {} AND revoked_at IS NULL",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                        );
                        sqlx::query(sqlx::AssertSqlSafe(tombstone))
                            .bind(request.now)
                            .bind(family.as_bytes().as_slice())
                            .execute(&mut **uow.inner())
                            .await
                            .map_err(|error| classify_error(&error))?;

                        let sweep = format!(
                            "UPDATE rv_auth_refresh SET revoked_at = {} WHERE family_id = {} AND revoked_at IS NULL",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                        );
                        let revoked = sqlx::query(sqlx::AssertSqlSafe(sweep))
                            .bind(request.now)
                            .bind(family.as_bytes().as_slice())
                            .execute(&mut **uow.inner())
                            .await
                            .map_err(|error| classify_error(&error))?
                            .rows_affected();
                        return Ok(RefreshTransition::Replayed { revoked });
                    }
                    if token_expires_at <= request.now {
                        // NOT a replay. An expired token revokes nothing.
                        return Ok(RefreshTransition::Unusable);
                    }

                    // 4. Consume, conditionally. `contracts/database-portability.md` §3 requires a
                    //    read-modify-write to lock the row it read *or* state a condition that
                    //    fails if the row changed. This does both.
                    let consume = format!(
                        "UPDATE rv_auth_refresh SET consumed_at = {}, replaced_by = {} WHERE id = {} AND consumed_at IS NULL",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                    );
                    let affected = sqlx::query(sqlx::AssertSqlSafe(consume))
                        .bind(request.now)
                        .bind(request.successor_id.as_bytes().as_slice())
                        .bind(token_id.as_slice())
                        .execute(&mut **uow.inner())
                        .await
                        .map_err(|error| classify_error(&error))?
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
                    let insert = format!(
                        "INSERT INTO rv_auth_refresh (id, family_id, token_hash, issued_at, expires_at, consumed_at, replaced_by, revoked_at) VALUES ({}, {}, {}, {}, {}, NULL, NULL, NULL)",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                        KIND.placeholder(4),
                        KIND.placeholder(5),
                    );
                    sqlx::query(sqlx::AssertSqlSafe(insert))
                        .bind(request.successor_id.as_bytes().as_slice())
                        .bind(family.as_bytes().as_slice())
                        .bind(request.successor.as_bytes().as_slice())
                        .bind(request.now)
                        .bind(request.successor_expires_at)
                        .execute(&mut **uow.inner())
                        .await
                        .map_err(|error| classify_error(&error))?;
                    Ok(RefreshTransition::Advanced)
                }

                /// Ends the transaction the way its outcome requires.
                ///
                /// Committed on success **including a refusal**: a refusal wrote nothing, so the
                /// commit is a no-op, and one exit path keeps the replay branch — which did write —
                /// from ever reaching a rollback by accident. An error rolls the whole thing back.
                async fn finish(
                    uow: crate::SqlxUnitOfWork<'_, $driver>,
                    outcome: Result<RefreshTransition, DatabaseError>,
                ) -> Result<RefreshTransition, DatabaseError> {
                    match outcome {
                        Ok(transition) => {
                            renvor_database::UnitOfWork::commit(uow).await?;
                            Ok(transition)
                        }
                        Err(error) => {
                            let _ = renvor_database::UnitOfWork::rollback(uow).await;
                            Err(error)
                        }
                    }
                }
            }

            #[cfg(feature = "tokens")]
            impl RefreshTokenRepository for SqlxRefreshTokenRepository {
                async fn begin_family(
                    &self,
                    family: NewRefreshFamily,
                ) -> Result<(), DatabaseError> {
                    let mut uow = crate::SqlxUnitOfWork::begin_on(&self.pool, KIND).await?;
                    // BOTH OR NEITHER. A family with no token is an authorisation nobody can
                    // exercise; a token with no family has no grant and no tombstone.
                    let written = Self::write_family(&mut uow, &family).await;
                    match written {
                        Ok(()) => renvor_database::UnitOfWork::commit(uow).await,
                        Err(error) => {
                            let _ = renvor_database::UnitOfWork::rollback(uow).await;
                            Err(error)
                        }
                    }
                }

                async fn grant_for(
                    &self,
                    digest: &SecretDigest,
                ) -> Result<Option<RefreshGrant>, DatabaseError> {
                    // NO LOCK AND NO TRANSACTION. This decides nothing — see the port. Every
                    // column it reads is written once and never updated, so the answer cannot go
                    // stale, and `advance` re-decides all of it under the family lock anyway.
                    let select = format!(
                        "SELECT f.id, f.user_id, f.scopes, f.expires_at FROM rv_auth_refresh t JOIN rv_auth_refresh_family f ON f.id = t.family_id WHERE t.token_hash = {}",
                        KIND.placeholder(1)
                    );
                    let row: Option<(Vec<u8>, Vec<u8>, String, DateTime<Utc>)> =
                        sqlx::query_as(sqlx::AssertSqlSafe(select))
                            .bind(digest.as_bytes().as_slice())
                            .fetch_optional(&self.pool)
                            .await
                            .map_err(|error| classify_error(&error))?;
                    row.map(|(family, user, scopes, family_expires_at)| {
                        Ok(RefreshGrant {
                            family: family_id_from(&family)?,
                            user: user_id_from(&user)?,
                            scopes: scopes_from(&scopes)?,
                            family_expires_at,
                        })
                    })
                    .transpose()
                }

                async fn advance(
                    &self,
                    request: AdvanceRequest<'_>,
                ) -> Result<RefreshTransition, DatabaseError> {
                    let mut uow = crate::SqlxUnitOfWork::begin_on(&self.pool, KIND).await?;
                    let outcome = self.decide(&mut uow, &request).await;
                    Self::finish(uow, outcome).await
                }

                async fn revoke_family(
                    &self,
                    family: FamilyId,
                    now: DateTime<Utc>,
                ) -> Result<u64, DatabaseError> {
                    let mut uow = crate::SqlxUnitOfWork::begin_on(&self.pool, KIND).await?;
                    let outcome = Self::revoke_within(&mut uow, family, now).await;
                    match outcome {
                        Ok(revoked) => {
                            renvor_database::UnitOfWork::commit(uow).await?;
                            Ok(revoked)
                        }
                        Err(error) => {
                            let _ = renvor_database::UnitOfWork::rollback(uow).await;
                            Err(error)
                        }
                    }
                }
            }

            #[cfg(feature = "tokens")]
            impl SqlxRefreshTokenRepository {
                /// The tombstone and the sweep, in one transaction.
                ///
                /// One transaction rather than two statements so that no reader can observe a
                /// family whose tombstone is set while its tokens are still live — the state the
                /// old design left behind permanently.
                async fn revoke_within(
                    uow: &mut crate::SqlxUnitOfWork<'_, $driver>,
                    family: FamilyId,
                    now: DateTime<Utc>,
                ) -> Result<u64, DatabaseError> {
                    let tombstone = format!(
                        "UPDATE rv_auth_refresh_family SET revoked_at = {} WHERE id = {} AND revoked_at IS NULL",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                    );
                    sqlx::query(sqlx::AssertSqlSafe(tombstone))
                        .bind(now)
                        .bind(family.as_bytes().as_slice())
                        .execute(&mut **uow.inner())
                        .await
                        .map_err(|error| classify_error(&error))?;

                    let sweep = format!(
                        "UPDATE rv_auth_refresh SET revoked_at = {} WHERE family_id = {} AND revoked_at IS NULL",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                    );
                    sqlx::query(sqlx::AssertSqlSafe(sweep))
                        .bind(now)
                        .bind(family.as_bytes().as_slice())
                        .execute(&mut **uow.inner())
                        .await
                        .map(|done| done.rows_affected())
                        .map_err(|error| classify_error(&error))
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

/// Rebuilds a `SecretDigest` from the stored `token_hash`.
pub(crate) fn digest_from(bytes: &[u8]) -> Result<SecretDigest, DatabaseError> {
    let sized: [u8; 32] = bytes
        .try_into()
        .map_err(|_| crate::error::record(DatabaseErrorKind::StatementRejected))?;
    Ok(SecretDigest::from_bytes(sized))
}

/// Rebuilds a `UserId` from the stored bytes.
pub(crate) fn user_id_from(bytes: &[u8]) -> Result<UserId, DatabaseError> {
    let sized: [u8; 16] = bytes
        .try_into()
        .map_err(|_| crate::error::record(DatabaseErrorKind::StatementRejected))?;
    Ok(UserId::from_bytes(sized))
}

/// Rebuilds a `FamilyId` from the stored bytes.
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
/// with an over-long scope or more than `ScopeSet::MAX_SCOPES` of them, which is the same bound the
/// value had when it was written. A stored claim that no longer parses is therefore a corrupted or
/// hand-edited row, and the answer is a refused read rather than a grant assembled from whatever
/// parsed — there is no default scope here and inventing one would be inventing an authorisation.
#[cfg(feature = "tokens")]
pub(crate) fn scopes_from(claim: &str) -> Result<ScopeSet, DatabaseError> {
    ScopeSet::parse_claim(claim)
        .map_err(|_| crate::error::record(DatabaseErrorKind::StatementRejected))
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
    // The no-op upsert that guarantees an abuse row exists. PostgreSQL names the conflict target.
    " ON CONFLICT (dimension, bucket) DO NOTHING",
    "The PostgreSQL repositories. Placeholders are numbered (`$1`), which is why the statements are composed rather than written once."
);
auth_repositories!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    DatabaseKind::MySql,
    // MySQL has no `ON CONFLICT`. `expires_at = expires_at` is chosen over `dimension = dimension`
    // deliberately: assigning to a primary-key column, even its own value, is not something to ask
    // an engine to reason about. `INSERT IGNORE` was rejected outright — it downgrades EVERY error
    // to a warning, so it would swallow a CHECK violation as readily as a duplicate key.
    " ON DUPLICATE KEY UPDATE expires_at = expires_at",
    "The MySQL repositories. Placeholders are positional (`?`), which is why the statements are composed rather than written once."
);
