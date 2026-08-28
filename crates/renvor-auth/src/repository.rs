//! The persistence **ports** for authentication.
//!
//! # Traits here, implementations in the adapters
//!
//! Nothing in this module names a driver, a pool, or a SQL string. `renvor-sqlx` and
//! `renvor-seaorm` implement these, and `xtask`'s persistence-isolation gate asserts this crate
//! resolves neither — so "the auth domain does not know which database it is on" is a property of
//! the dependency graph rather than a claim in a comment.
//!
//! # Why the return types carry `DatabaseError` and nothing richer
//!
//! A repository reports that a write did not happen and **which class** of failure it was — that is
//! `renvor_database::DatabaseErrorKind`, which Phase 008 measured across all four rows precisely so
//! an application swapping adapters does not rewrite its error handling. Turning a driver's text
//! into an auth-level message is `renvor-auth`'s job at a higher layer, not the port's, and
//! [`crate::AuthError`] is fieldless so none of that text can travel with it.
//!
//! # Native `impl Future` rather than `async_trait`
//!
//! Matches `renvor_database::Database` and `UnitOfWork`, which use the same shape. It costs no
//! allocation per call and keeps the `Send` bound visible in the signature rather than hidden in a
//! macro expansion.

use core::future::Future;

use chrono::{DateTime, Utc};
use renvor_database::DatabaseError;

use crate::opaque::SecretDigest;
use crate::password::PasswordHash;
use crate::subject::UserId;

/// A user row, as the domain sees it.
///
/// Carries **no password material**. The credential is a separate row behind
/// [`CredentialRepository`], so a query that loads a user for display cannot accidentally load a
/// hash along with it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UserRecord {
    /// The stable identity.
    pub id: UserId,
    /// The address, already normalised by the caller.
    pub email: String,
    /// When the address was verified, if it has been.
    pub email_verified_at: Option<DateTime<Utc>>,
}

/// A stored credential.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CredentialRecord {
    /// Whose credential this is.
    pub user_id: UserId,
    /// The PHC string. See [`PasswordHash`] for why this is not treated as a secret.
    pub password_hash: PasswordHash,
    /// Set when the password is known compromised (NIST §3.1.1.2's *"SHALL force a change"*).
    pub must_change: bool,
}

/// Whether a registration created the account or found it already present.
///
/// **Not a `bool`.** A duplicate registration is the case FR-080 is about, and a boolean at this
/// boundary reads as `true`/`false` at the call site with nothing to say which is which.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Registration {
    /// This call created the account.
    Created(UserId),
    /// The address was already registered. **Carries no identity**: telling a caller *which*
    /// account exists would hand an enumeration oracle to whatever is above it.
    AlreadyRegistered,
}

/// Reads and writes user rows.
pub trait UserRepository: Send + Sync {
    /// Registers `email`, or reports that it was already taken.
    ///
    /// # The uniqueness must come from the database
    ///
    /// `contracts/database-portability.md` §3 forbids depending on the default isolation level, and
    /// the two engines differ: PostgreSQL defaults to `READ COMMITTED` and MySQL to
    /// `REPEATABLE READ`. A check-then-insert therefore has **different outcomes on different
    /// rows**, which is exactly the class of bug the four-row matrix exists to catch. The unique
    /// constraint on `email` is what makes the answer the same everywhere.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`]. A unique violation is **not** an error here — it is
    /// [`Registration::AlreadyRegistered`], because it is an expected outcome rather than a fault.
    fn register(
        &self,
        email: &str,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Registration, DatabaseError>> + Send;

    /// Finds a user by address.
    ///
    /// Returns `Ok(None)` for an unknown address. **The caller must not branch differently on it in
    /// a way an attacker can observe** — see
    /// [`crate::password::PasswordService::verify_against_stored_or_dummy`].
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn find_by_email(
        &self,
        email: &str,
    ) -> impl Future<Output = Result<Option<UserRecord>, DatabaseError>> + Send;

    /// Finds a user by identity.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn find_by_id(
        &self,
        id: UserId,
    ) -> impl Future<Output = Result<Option<UserRecord>, DatabaseError>> + Send;
}

/// Reads and writes credential rows.
pub trait CredentialRepository: Send + Sync {
    /// Stores or replaces the credential for `user_id`.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn upsert(
        &self,
        user_id: UserId,
        hash: &PasswordHash,
        must_change: bool,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), DatabaseError>> + Send;

    /// Loads the credential for `user_id`, if there is one.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn find(
        &self,
        user_id: UserId,
    ) -> impl Future<Output = Result<Option<CredentialRecord>, DatabaseError>> + Send;
}

/// Consumes a single-use token, atomically.
///
/// # One statement, not a read then a write
///
/// FR-050 requires two concurrent consumers of one token to produce **exactly one** success, on
/// every one of the four rows. A `SELECT` followed by an `UPDATE` cannot promise that without
/// depending on the isolation level, which §3 forbids and which differs between the engines
/// anyway.
///
/// The implementation is therefore one conditional `UPDATE` whose `WHERE` clause carries the
/// preconditions — unconsumed, unexpired — and which succeeds only if it affected exactly one row.
pub trait SingleUseTokenRepository: Send + Sync {
    /// Issues a token for `user_id`, storing only its digest.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn issue(
        &self,
        user_id: UserId,
        digest: &SecretDigest,
        expires_at: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), DatabaseError>> + Send;

    /// Invalidates **every** outstanding token this repository holds for `user_id`.
    ///
    /// # Why resend must invalidate rather than accumulate
    ///
    /// A resend that leaves the previous token live multiplies the number of valid secrets for one
    /// account, and every one of them is a link sitting in an inbox or a proxy log. Invalidating
    /// first means **at most one** token per purpose per account is ever live, so the blast radius
    /// of a leaked mailbox is one link rather than every link ever sent.
    ///
    /// Marks them consumed rather than deleting them: a consumed row is evidence that a token
    /// existed, which an abuse control can count, and a deleted row is not.
    ///
    /// Returns how many were invalidated, so a caller can assert the effect rather than assume it.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn invalidate_all_for(
        &self,
        user_id: UserId,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<u64, DatabaseError>> + Send;

    /// Consumes the token matching `digest`, if it is unconsumed and unexpired at `now`.
    ///
    /// Returns the owner on success and `None` when the token is unknown, already consumed, or
    /// expired — **one answer for all three**, because distinguishing them tells a holder of a
    /// stale link something about the account.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn consume(
        &self,
        digest: &SecretDigest,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Option<UserId>, DatabaseError>> + Send;
}

/// The refresh-token store (FR-040 … FR-042). Part of API token mode, so it is behind `tokens`.
///
/// # Why this port cannot answer `Option`, where [`SingleUseTokenRepository`] can
///
/// [`SingleUseTokenRepository::consume`] deliberately returns one answer for "unknown", "already
/// consumed", and "expired": telling the holder of a stale password-reset link which of the three
/// it is tells them something about the account.
///
/// A refresh token is the opposite case. **"Already consumed" is the security signal**, not a
/// detail to be hidden — ASVS **V10.4.5 (L1)** requires that presenting an already-invalidated
/// refresh token *"revoke all refresh tokens for that authorization"*, and a port that cannot
/// distinguish a replay from an unknown token cannot implement that sentence. The distinction is
/// still never given to the *presenter*: [`crate::refresh::RefreshRejection`] narrows it back down
/// before anything leaves the crate.
///
/// # One statement decides the winner, and a later read is safe
///
/// [`Self::consume`] must be a **single conditional statement** whose `WHERE` clause carries the
/// preconditions — unconsumed, unexpired, unrevoked family — exactly as
/// [`SingleUseTokenRepository`] requires, so two concurrent presentations of one valid token
/// produce **exactly one** rotation on all four rows.
///
/// When that statement affects no row, the implementation may then read the row to say *why*. That
/// second read does not need to be in the same transaction, because every state it distinguishes is
/// **terminal**: a consumed token is never unconsumed again, and a revoked family is never
/// un-revoked. The diagnosis cannot go stale in the direction that matters.
#[cfg(feature = "tokens")]
pub trait RefreshTokenRepository: Send + Sync {
    /// Stores a newly issued refresh token. **Only its digest** — FR-041.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn issue(
        &self,
        record: crate::refresh::NewRefreshToken,
    ) -> impl Future<Output = Result<(), DatabaseError>> + Send;

    /// Atomically consumes the token matching `digest`, and reports what was observed.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn consume(
        &self,
        digest: &SecretDigest,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<crate::refresh::RefreshConsumption, DatabaseError>> + Send;

    /// Revokes **every** token in `family`, consumed or not, and returns how many were affected.
    ///
    /// This is the ASVS V10.4.5 response to a detected replay. It returns a count so a caller can
    /// assert the effect rather than assume it — a revocation that silently affected nothing is the
    /// failure mode this signature exists to make visible.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn revoke_family(
        &self,
        family: crate::refresh::FamilyId,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<u64, DatabaseError>> + Send;
}
