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
/// # This port was redesigned, and the reason is a defect that was in it
///
/// The first shape had three independent methods — `consume`, `issue`, `revoke_family` — and the
/// rotation service called them in that order. Against a *fake* repository whose async methods
/// contain no `.await`, `tokio::join!` runs each rotation effectively to completion before the
/// next starts, so a two-caller race never interleaved and the suite was green. Against a real
/// database it interleaves like this:
///
/// ```text
/// A: consume(old)      -> Consumed          A won the race
/// B: consume(old)      -> Replayed          B lost, and correctly detects the replay
/// B: revoke_family(f)  -> revokes every row THAT EXISTS RIGHT NOW
/// A: issue(successor)  -> INSERT ... revoked_at = NULL
///                         the revoked family is live again
/// ```
///
/// No ordering of three separate statements fixes that, because the successor does not exist when
/// the revocation runs. **The transition has to be one transaction**, and the family has to have a
/// durable row of its own so that there is something to hold a lock on and something to carry a
/// tombstone.
///
/// # The shape that replaces it
///
/// ```text
/// rv_auth_refresh_family   id, user_id, scopes, created_at, expires_at, revoked_at
///        ^ one row per login: the IMMUTABLE grant, plus the monotonic tombstone
///        |
/// rv_auth_refresh          id, family_id, token_hash, issued_at, expires_at,
///                          consumed_at, replaced_by, revoked_at
///          ^ the chain. No user and no scopes: they are facts about the FAMILY, and a
///            copy on every token row is a copy that can disagree with the original.
/// ```
///
/// [`Self::advance`] locks the family, then the token, revalidates both, and either consumes and
/// inserts **in one transaction** or refuses. A revocation and an insertion into the same family
/// can no longer interleave, because the second cannot begin until the first commits.
///
/// # Why the caller reads before it decides, and why that is safe
///
/// Minting an access token needs the subject and the scopes, and signing after the successor is
/// durable would risk a live refresh token whose access token was never handed out. So the service
/// reads the grant with [`Self::grant_for`], prepares both secrets **in memory**, and only then
/// calls [`Self::advance`].
///
/// That preliminary read grants no authority. Every field it returns is immutable for the life of
/// the row — a token's `family_id` never changes, and a family's `user_id`, `scopes` and
/// `expires_at` are written once by [`Self::begin_family`] and never updated — so the value cannot
/// go stale in a direction that matters. Whether the rotation *happens* is decided entirely by
/// `advance`, under the lock, and a caller that ignored a refusal would still have written nothing.
///
/// # There is deliberately no generic "insert a token into a family"
///
/// The old `issue` was exactly that, and it is the method the interleaving above needed. A
/// successor can now be created **only** by `advance`, in the same transaction that consumed its
/// predecessor and revalidated the tombstone, and a first token only by `begin_family`, which
/// creates the family alongside it.
///
/// # Why this port answers `Replayed` where [`SingleUseTokenRepository`] answers `None`
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
#[cfg(feature = "tokens")]
pub trait RefreshTokenRepository: Send + Sync {
    /// Creates a family and its first token **in one transaction**.
    ///
    /// Both or neither. A family with no token is an authorisation nobody can exercise and nothing
    /// will ever clean up; a token with no family has no grant to carry and no tombstone to check.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn begin_family(
        &self,
        family: crate::refresh::NewRefreshFamily,
    ) -> impl Future<Output = Result<(), DatabaseError>> + Send;

    /// Reads the immutable grant behind a presented digest, if the digest names a token at all.
    ///
    /// **This grants nothing.** It reports what a rotation *would* carry forward so the caller can
    /// prepare the secrets before the decision; [`Self::advance`] is the decision. It answers for
    /// consumed, expired and revoked tokens too — the grant is a fact about the family, and
    /// withholding it here would only move the same read inside the transaction.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn grant_for(
        &self,
        digest: &SecretDigest,
    ) -> impl Future<Output = Result<Option<crate::refresh::RefreshGrant>, DatabaseError>> + Send;

    /// The one atomic transition: consume the presented token and insert its successor, **or**
    /// refuse and — for a replay — tombstone the family, all in a single transaction.
    ///
    /// # What the implementation must do, in this order
    ///
    /// 1. Find the presented token's family. Unknown digest → [`crate::refresh::RefreshTransition::Unusable`],
    ///    with nothing written.
    /// 2. Lock the **family** row, then the **token** row. Always that order: two transitions on
    ///    one family take the same locks in the same sequence, so they queue rather than deadlock.
    /// 3. Revalidate under the lock. A revoked or expired family →
    ///    [`crate::refresh::RefreshTransition::FamilyRevoked`] or [`crate::refresh::RefreshTransition::Unusable`], and **no other
    ///    family's state may change**.
    /// 4. A token already consumed or revoked → the replay response: write the family's tombstone
    ///    if it is not already set, revoke every unrevoked token in the family, and return
    ///    [`crate::refresh::RefreshTransition::Replayed`] with the count. In the same transaction.
    /// 5. A live, unexpired token → consume it, record `replaced_by`, and insert the successor.
    ///    In the same transaction. Return [`crate::refresh::RefreshTransition::Advanced`].
    ///
    /// A successor may **never** be inserted into a family whose tombstone is set. That is what
    /// step 2's lock buys: a concurrent replay response cannot land between step 3 and step 5.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`]. A failure must roll back the whole transition — a consumed
    /// predecessor with no successor is a silently ended session, and a successor with a live
    /// predecessor is two valid tokens where the design promises one.
    fn advance(
        &self,
        request: crate::refresh::AdvanceRequest<'_>,
    ) -> impl Future<Output = Result<crate::refresh::RefreshTransition, DatabaseError>> + Send;

    /// Revokes `family` outright: the tombstone, and every token still live in it.
    ///
    /// **Not part of rotation** — the replay response lives inside [`Self::advance`], where it can
    /// be atomic with the check that detected it. This is the deliberate operation: a sign-out, or
    /// an administrator ending a session.
    ///
    /// The tombstone is **monotonic**. A family revoked at `t1` and revoked again at `t2` keeps
    /// `t1`: the moment authorisation ended is the fact worth keeping, and a later write that moved
    /// it forward would make the audit trail describe the second call rather than the event.
    ///
    /// Returns how many token rows were affected, so a caller can assert the effect rather than
    /// assume it — a revocation that silently touched nothing is the failure mode this signature
    /// exists to make visible.
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
