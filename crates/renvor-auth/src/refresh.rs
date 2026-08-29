//! Refresh tokens: opaque, rotated on every use, hashed at rest, and revoked as a family on replay.
//!
//! Behind the `tokens` feature, with [`crate::token`].
//!
//! # Why this half is not a JWT
//!
//! An access token is a JWT because resource servers, gateways, and SDKs downstream have to
//! validate it and JWT is what they speak. **Nothing outside Renvor ever parses a refresh token**:
//! it is presented to the issuer and to no one else. A self-contained format would buy
//! interoperability nobody needs and cost the one property that matters here — the issuer must be
//! able to decide, at presentation time, that this credential is dead. That decision needs a row,
//! and once there is a row the token can be an opaque 256-bit value with **nothing** in it.
//!
//! So this half adds no dependency at all. [`crate::opaque::Opaque`] generates it from the entropy
//! port and [`crate::opaque::SecretDigest`] is what the store holds (FR-041).
//!
//! # Rotation, and what replay means
//!
//! Every successful refresh **consumes** the presented token and issues a new one in the same
//! *family* — one family per login. A token is therefore valid exactly once.
//!
//! Presenting a token that has already been consumed means one of two things, and the issuer cannot
//! tell which: either the legitimate client is retrying, or an attacker is spending a stolen copy.
//! ASVS **V10.4.5 (L1)** resolves it by refusing to guess — *"revoke all refresh tokens for that
//! authorization if an already used and invalidated refresh token is provided"*. The whole family
//! dies, both parties are logged out, and the theft is converted from a silent persistent foothold
//! into a visible re-authentication.
//!
//! **The citation is ASVS V10.4.5, not RFC 9700.** RFC 9700 §4.14.2 requires only that the
//! authorization server *"will revoke the active refresh token"*; family-wide revocation appears
//! there in non-normative prose. Citing the RFC for this would be citing it for something it does
//! not require.
//!
//! # The defect this module was rebuilt around
//!
//! The first version drove three independent store calls — consume, then mint, then insert, and a
//! separate revocation on the replay branch. Every one of them was individually correct and the
//! composition was not:
//!
//! ```text
//! A: consume(old)     -> Consumed          A wins the race
//! B: consume(old)     -> Replayed          B loses, and correctly detects the replay
//! B: revoke_family(f) -> revokes every row THAT EXISTS AT THIS INSTANT
//! A: issue(new)       -> INSERT ... revoked_at = NULL
//!                        the family B just revoked has a live token again
//! ```
//!
//! It passed its unit suite because the fake store's `async fn`s contain no `.await`, so
//! `tokio::join!` ran each rotation to completion before starting the next and the interleaving
//! above never occurred. **A fake with no suspension point cannot fail a concurrency test**, which
//! is why the four-row suite is the evidence for this property and this module's tests are not.
//!
//! The fix is not an ordering. It is that the transition — revalidate, consume, insert — has to be
//! **one transaction over a durable family row**, so a concurrent revocation cannot land in the
//! middle of it. See [`crate::repository::RefreshTokenRepository::advance`].
//!
//! # The order this module works in, and why
//!
//! ```text
//! grant_for(presented)   read the immutable grant. Decides NOTHING.
//!        |
//! generate + sign        both secrets, in memory. Nothing durable yet.
//!        |
//! advance(...)           THE decision, under the family lock, in one transaction
//!        |
//!  committed?  yes -> hand over the prepared secrets
//!              no  -> drop them
//! ```
//!
//! Signing before the write is deliberate. A successor that is durable while its access token
//! failed to sign is a live refresh token whose holder was never given anything — an inaccessible
//! credential nothing will clean up. Doing the fallible work first means a failure costs a refused
//! request, not a stranded row.

use std::fmt;

use chrono::{DateTime, Duration, Utc};
use renvor_core::observe::entropy::EntropySource;
use renvor_database::DatabaseError;

use crate::clock::Clock;
use crate::error::AuthError;
use crate::opaque::{Opaque, OpaqueKind, SecretDigest};
use crate::repository::RefreshTokenRepository;
use crate::subject::{AuthenticatedSubject, UserId};
use crate::token::{AccessToken, AccessTokenIssuer, ScopeSet};

/// The placeholder every redacted rendering in this module uses.
const REDACTED: &str = "[redacted]";

/// The longest a refresh token — or a whole refresh family — may be configured to live:
/// **thirty days**.
///
/// Stated as Renvor's ceiling rather than a citation. NIST SP 800-63B-4 §4.1.3 sets
/// reauthentication periods for *sessions* — 30 days at AAL1, 12 hours at AAL2 — and those are
/// requirements about a different thing; borrowing one of those numbers and presenting it as a
/// refresh-token requirement would be dressing a choice as a standard. Thirty days is the outer
/// bound at which a refresh chain stops being a login and starts being a permanent credential.
pub const MAX_REFRESH_LIFETIME: Duration = Duration::days(30);

/// How long an issued refresh token stays valid.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RefreshLifetime(Duration);

impl RefreshLifetime {
    /// Builds a lifetime.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when not positive or above [`MAX_REFRESH_LIFETIME`].
    /// Refused, never clamped: an operator who asked for sixty days should learn that they cannot
    /// have it, not silently receive thirty.
    pub fn new(lifetime: Duration) -> Result<Self, AuthError> {
        if lifetime <= Duration::zero() || lifetime > MAX_REFRESH_LIFETIME {
            return Err(AuthError::PolicyMisconfigured);
        }
        Ok(Self(lifetime))
    }

    /// The configured duration.
    #[must_use]
    pub const fn get(self) -> Duration {
        self.0
    }
}

impl Default for RefreshLifetime {
    /// Fourteen days.
    fn default() -> Self {
        Self(Duration::days(14))
    }
}

/// How long a whole refresh *chain* may live, counted from the login that began it.
///
/// # Why a chain needs its own bound
///
/// [`RefreshLifetime`] bounds one token. Rotation replaces that token, so a chain that is used
/// steadily never expires: fourteen days after the login, and after every rotation, there is
/// always another fourteen days. The credential stops being a session and becomes permanent
/// without anyone deciding that it should.
///
/// This is the decision. A family carries an absolute end written **once**, at
/// [`RefreshRotation::begin`], and no rotation moves it — so a chain outlives its login by at most
/// this long whatever happens to it in between. A successor's expiry is the earlier of the token
/// lifetime and what remains of the family; that is arithmetic rather than a silent clamp of a
/// configured value, and the configuration itself is refused rather than clamped below.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FamilyLifetime(Duration);

impl FamilyLifetime {
    /// Builds a chain lifetime.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when not positive or above [`MAX_REFRESH_LIFETIME`].
    pub fn new(lifetime: Duration) -> Result<Self, AuthError> {
        if lifetime <= Duration::zero() || lifetime > MAX_REFRESH_LIFETIME {
            return Err(AuthError::PolicyMisconfigured);
        }
        Ok(Self(lifetime))
    }

    /// The configured duration.
    #[must_use]
    pub const fn get(self) -> Duration {
        self.0
    }
}

impl Default for FamilyLifetime {
    /// Thirty days — the ceiling. A chain reaching the outer bound is the *default* because
    /// shortening it is a decision an operator makes about their own risk, and a default below the
    /// ceiling would silently cap sessions in a way nobody asked for.
    fn default() -> Self {
        Self(MAX_REFRESH_LIFETIME)
    }
}

/// Identifies one chain of rotated refresh tokens — one login.
///
/// Not a secret: it names a family so that a family can be revoked, and it never travels to a
/// client. It is generated from the entropy port anyway, because a predictable family identifier
/// would let anyone who could guess one revoke somebody else's session.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct FamilyId([u8; Self::BYTES]);

impl FamilyId {
    /// The number of bytes behind a family identifier.
    pub const BYTES: usize = 16;

    /// Generates an identifier.
    ///
    /// # Errors
    ///
    /// [`AuthError::EntropyUnavailable`] when the platform CSPRNG fails. No fallback.
    pub fn generate(source: &dyn EntropySource) -> Result<Self, AuthError> {
        let mut bytes = [0_u8; Self::BYTES];
        source
            .fill(&mut bytes)
            .map_err(|_| AuthError::EntropyUnavailable)?;
        Ok(Self(bytes))
    }

    /// Rebuilds an identifier from persistence.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; Self::BYTES]) -> Self {
        Self(bytes)
    }

    /// The raw bytes, for persistence.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; Self::BYTES] {
        &self.0
    }
}

impl fmt::Display for FamilyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Identifies one row in a refresh chain.
///
/// Distinct from [`FamilyId`] as a type rather than as a convention: `replaced_by` points at a
/// token and the family lock is taken on a family, and a single sixteen-byte newtype for both
/// would let one be passed where the other belongs and still compile.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct RefreshTokenId([u8; Self::BYTES]);

impl RefreshTokenId {
    /// The number of bytes behind a token row identifier.
    pub const BYTES: usize = 16;

    /// Generates an identifier.
    ///
    /// # Errors
    ///
    /// [`AuthError::EntropyUnavailable`] when the platform CSPRNG fails.
    pub fn generate(source: &dyn EntropySource) -> Result<Self, AuthError> {
        let mut bytes = [0_u8; Self::BYTES];
        source
            .fill(&mut bytes)
            .map_err(|_| AuthError::EntropyUnavailable)?;
        Ok(Self(bytes))
    }

    /// Rebuilds an identifier from persistence.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; Self::BYTES]) -> Self {
        Self(bytes)
    }

    /// The raw bytes, for persistence.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; Self::BYTES] {
        &self.0
    }
}

impl fmt::Display for RefreshTokenId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// An opaque refresh credential.
///
/// A thin wrapper over [`Opaque`] rather than a second implementation of one: the generation site,
/// the wire form, the digest, and the redaction are all already correct there, and a parallel
/// implementation is how two things that must agree stop agreeing.
#[derive(Clone)]
pub struct RefreshToken(Opaque);

impl RefreshToken {
    /// Generates a refresh token.
    ///
    /// # Errors
    ///
    /// [`AuthError::EntropyUnavailable`] when the platform CSPRNG fails.
    pub fn generate(source: &dyn EntropySource) -> Result<Self, AuthError> {
        Opaque::generate(OpaqueKind::Refresh, source).map(Self)
    }

    /// Rebuilds a token from what a client presented. `None` when it is not the right shape.
    #[must_use]
    pub fn from_wire(presented: &str) -> Option<Self> {
        Opaque::from_wire(OpaqueKind::Refresh, presented).map(Self)
    }

    /// The token, for the one place that must transmit it.
    #[must_use]
    pub fn expose(&self) -> String {
        self.0.expose()
    }

    /// The abuse-control **client key** for a presented refresh token.
    ///
    /// # Why this is here and not computed by the transport
    ///
    /// `evidence/abuse-control-matrix.md` §2.1 records why the client axis is keyed on the token
    /// rather than on its family: the family is not known until the store has been read, and a
    /// rate limit that needs a database lookup to decide whether to rate-limit is not much of a
    /// rate limit.
    ///
    /// The key is the first sixteen bytes of the digest, not the digest itself, because
    /// [`crate::abuse::AttemptKey::Client`] takes sixteen — and truncation costs nothing here: the
    /// value is immediately HMAC'd under the server key and masked into a bucket, so it is a
    /// *lookup key*, never a credential comparison.
    ///
    /// **A malformed presentation is keyed on a fixed constant.** Every unparseable token shares
    /// one bucket, which is correct rather than a compromise — they are all the same event, and
    /// giving each its own bucket would let garbage spread across the whole space for free.
    ///
    /// It lives on this type so there is **one** place that decides what a presented token's key
    /// is. A transport computing its own would be a second answer to the same question.
    #[must_use]
    pub fn client_key(presented: &str) -> [u8; 16] {
        let Some(token) = Self::from_wire(presented) else {
            return [0; 16];
        };
        let digest = token.digest();
        let mut key = [0_u8; 16];
        key.copy_from_slice(&digest.as_bytes()[..16]);
        key
    }

    /// What the store holds in its place (FR-041).
    #[must_use]
    pub fn digest(&self) -> SecretDigest {
        SecretDigest::of(&self.0)
    }
}

impl fmt::Debug for RefreshToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RefreshToken({REDACTED})")
    }
}

impl fmt::Display for RefreshToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

/// A family and its first token, to be created together.
///
/// # There is nowhere in this type to put the secret
///
/// The token field is a [`SecretDigest`]. A caller that wanted to store the raw token would have
/// to change this struct to do it, which is a reviewable act — the same argument
/// [`crate::token::TokenRejection`] makes about rejection reasons, applied to a write.
#[derive(Clone, Debug)]
pub struct NewRefreshFamily {
    /// The chain being started.
    pub family: FamilyId,
    /// Whose chain it is. **Immutable for the life of the family**, and the only copy of the fact.
    pub user: UserId,
    /// The privileges every rotation in this chain carries forward. Also immutable, and also the
    /// only copy: a per-token copy is a second place the answer can be, and two places disagree.
    pub scopes: ScopeSet,
    /// The identity of the first token row.
    pub first_token_id: RefreshTokenId,
    /// What the store holds in the first token's place.
    pub first_token: SecretDigest,
    /// When the login happened.
    pub created_at: DateTime<Utc>,
    /// The absolute end of the chain. Written once; no rotation moves it.
    pub family_expires_at: DateTime<Utc>,
    /// When the first token stops being valid. Never after `family_expires_at`.
    pub token_expires_at: DateTime<Utc>,
}

/// The immutable facts behind a presented refresh token.
///
/// Read before the decision and used to prepare the successor. It is safe to act on a value that
/// may be a moment old because **every field is immutable once written** — see
/// [`crate::repository::RefreshTokenRepository::grant_for`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RefreshGrant {
    /// The chain to continue.
    pub family: FamilyId,
    /// Whose token it is.
    pub user: UserId,
    /// The privileges to carry forward. **Never widened** by a rotation.
    pub scopes: ScopeSet,
    /// The absolute end of the chain, so a successor cannot be issued past it.
    pub family_expires_at: DateTime<Utc>,
}

/// One atomic rotation, as the store is asked to perform it.
///
/// Carries the successor **already generated**: the store's job is to decide and to write, never
/// to mint. A repository that generated the secret would be signing tokens inside a transaction,
/// which is the shape [`crate::repository::RefreshTokenRepository::advance`] exists to avoid.
#[derive(Clone, Copy, Debug)]
pub struct AdvanceRequest<'a> {
    /// The digest of the token the client presented.
    pub presented: &'a SecretDigest,
    /// The identity of the successor row, if one is written.
    pub successor_id: RefreshTokenId,
    /// The digest of the successor, if one is written.
    pub successor: &'a SecretDigest,
    /// When the successor stops being valid.
    pub successor_expires_at: DateTime<Utc>,
    /// The instant the transition is evaluated at — from the injected [`Clock`], never the
    /// database's own clock, so a test can place it exactly.
    pub now: DateTime<Utc>,
}

/// What the store's single transaction did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum RefreshTransition {
    /// The presented token was consumed and the successor is durable. This caller won, and no
    /// other can: both writes happened under the family lock, in one transaction.
    Advanced,
    /// The presented token had already been spent. ASVS V10.4.5: the family's tombstone is set and
    /// every live token in it revoked, **in the same transaction that observed the replay**.
    /// `revoked` is how many token rows that affected.
    Replayed {
        /// How many token rows the response revoked.
        revoked: u64,
    },
    /// The family's tombstone was already set. Nothing was written.
    FamilyRevoked,
    /// No usable row: the digest is unknown, the token has expired, or the family has. **One
    /// answer for all three** — an expired token is not a replay, and telling a presenter which of
    /// them it holds tells them whether the value was ever real.
    Unusable,
}

/// Why a presented refresh token was refused.
///
/// # This is narrower than what the store saw, deliberately
///
/// [`RefreshTransition`] distinguishes four states because the *issuer* needs the distinction to
/// implement family revocation. The *presenter* gets three, and never learns whether the token it
/// holds was unknown or merely expired. Like [`crate::token::TokenRejection`], every variant is a
/// bare marker, so nothing built from one can carry the credential.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[non_exhaustive]
pub enum RefreshRejection {
    /// Not a refresh token this issuer has outstanding, or it has expired.
    Unusable,
    /// Already spent. The family has been revoked as a result — see [`RefreshOutcome`].
    Replayed,
    /// The family was revoked before this presentation.
    FamilyRevoked,
}

impl fmt::Display for RefreshRejection {
    /// A **static** description per variant.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unusable => "the refresh token is not outstanding",
            Self::Replayed => "the refresh token was already spent",
            Self::FamilyRevoked => "the token family has been revoked",
        })
    }
}

/// A fresh pair, issued together.
#[derive(Debug)]
pub struct RotatedTokens {
    /// The short-lived JWT.
    pub access: AccessToken,
    /// The opaque credential that buys the next pair.
    pub refresh: RefreshToken,
    /// The chain both belong to.
    pub family: FamilyId,
}

/// The result of presenting a refresh token.
#[derive(Debug)]
#[non_exhaustive]
pub enum RefreshOutcome {
    /// The presented token was spent and a new pair issued.
    Rotated(RotatedTokens),
    /// The presented token was refused, and **the prepared secrets were discarded** — they were
    /// never durable, so nothing has to be undone. When the reason is
    /// [`RefreshRejection::Replayed`], the family was revoked in the same transaction that
    /// detected the replay, and `revoked` says how many token rows that affected.
    Rejected {
        /// Why.
        reason: RefreshRejection,
        /// How many tokens the replay response revoked. Zero for every reason but a replay.
        revoked: u64,
    },
}

/// Issues and rotates refresh tokens against a store.
#[derive(Debug)]
pub struct RefreshRotation<R> {
    repository: R,
    lifetime: RefreshLifetime,
    family_lifetime: FamilyLifetime,
}

impl<R: RefreshTokenRepository> RefreshRotation<R> {
    /// Builds a rotation service.
    pub const fn new(
        repository: R,
        lifetime: RefreshLifetime,
        family_lifetime: FamilyLifetime,
    ) -> Self {
        Self {
            repository,
            lifetime,
            family_lifetime,
        }
    }

    /// Starts a new family for a subject that has just authenticated, and issues the first pair.
    ///
    /// The family row and its first token are written by one call, so there is never a family with
    /// no token or a token with no grant.
    ///
    /// # Errors
    ///
    /// [`AuthError::EntropyUnavailable`] when the CSPRNG fails, [`AuthError::PolicyMisconfigured`]
    /// when the access token cannot be signed, and [`AuthError::NotPermitted`] when the store
    /// refuses the write — the raw [`DatabaseError`] is deliberately **not** carried out, because
    /// [`AuthError`] is fieldless and a driver's text has no route through it.
    pub async fn begin(
        &self,
        subject: AuthenticatedSubject,
        scopes: &ScopeSet,
        issuer: &AccessTokenIssuer,
        clock: &dyn Clock,
        entropy: &dyn EntropySource,
    ) -> Result<RotatedTokens, AuthError> {
        let now = clock.now();
        let family = FamilyId::generate(entropy)?;
        let first_token_id = RefreshTokenId::generate(entropy)?;
        let refresh = RefreshToken::generate(entropy)?;
        // SIGNED BEFORE THE WRITE. A signing failure after the row is durable would leave a live
        // refresh token whose holder received nothing.
        let access = issuer.issue(subject, scopes, clock, entropy)?;

        let family_expires_at = now + self.family_lifetime.get();
        self.repository
            .begin_family(NewRefreshFamily {
                family,
                user: subject.user_id(),
                scopes: scopes.clone(),
                first_token_id,
                first_token: refresh.digest(),
                created_at: now,
                family_expires_at,
                token_expires_at: token_expiry(now, self.lifetime, family_expires_at),
            })
            .await
            .map_err(store_failure)?;

        Ok(RotatedTokens {
            access,
            refresh,
            family,
        })
    }

    /// Spends `presented` and issues the next pair, or refuses it.
    ///
    /// # The order is the security argument
    ///
    /// The grant is read first and decides nothing. Both secrets are then prepared **in memory**.
    /// The store's single transaction is the decision, and the prepared secrets are released only
    /// if it committed — so a losing racer never receives a token, a replay is answered by
    /// revocation inside the same transaction that detected it, and a signing failure costs a
    /// refused request rather than a stranded row.
    ///
    /// # Errors
    ///
    /// As [`Self::begin`]. A refusal is **not** an error: it is [`RefreshOutcome::Rejected`],
    /// because being handed a dead credential is an ordinary event and not a fault.
    pub async fn rotate(
        &self,
        admitted: crate::abuse::Admitted,
        presented: &str,
        issuer: &AccessTokenIssuer,
        clock: &dyn Clock,
        entropy: &dyn EntropySource,
    ) -> Result<RefreshOutcome, AuthError> {
        // The sixth of FR-063's six flows. `Admitted` has no public constructor, so a rotation
        // that never passed an abuse control cannot be written — see `crate::abuse::Admitted`.
        admitted.expect(crate::abuse::AttemptFlow::TokenRefresh)?;
        let now = clock.now();

        // A value that is not even the right shape is refused without touching the store. This is
        // not an optimisation: it keeps a malformed presentation from becoming a database query.
        let Some(token) = RefreshToken::from_wire(presented) else {
            return Ok(rejected(RefreshRejection::Unusable));
        };
        let presented_digest = token.digest();

        // THE PRELIMINARY READ. It grants no authority; every field it returns is immutable, and
        // `advance` re-decides everything under the family lock.
        let Some(grant) = self
            .repository
            .grant_for(&presented_digest)
            .await
            .map_err(store_failure)?
        else {
            return Ok(rejected(RefreshRejection::Unusable));
        };

        // Prepared, not committed. Nothing below this line is durable until `advance` returns.
        let successor_id = RefreshTokenId::generate(entropy)?;
        let successor = RefreshToken::generate(entropy)?;
        let subject = AuthenticatedSubject::new(grant.user);
        // The scopes come from the GRANT, not from the caller. A rotation carries privileges
        // forward; it is not an opportunity to acquire new ones.
        let access = issuer.issue(subject, &grant.scopes, clock, entropy)?;

        let transition = self
            .repository
            .advance(AdvanceRequest {
                presented: &presented_digest,
                successor_id,
                successor: &successor.digest(),
                successor_expires_at: token_expiry(now, self.lifetime, grant.family_expires_at),
                now,
            })
            .await
            .map_err(store_failure)?;

        match transition {
            RefreshTransition::Advanced => Ok(RefreshOutcome::Rotated(RotatedTokens {
                access,
                refresh: successor,
                family: grant.family,
            })),
            // `access` and `successor` are dropped here, unreturned and never written.
            RefreshTransition::Replayed { revoked } => Ok(RefreshOutcome::Rejected {
                reason: RefreshRejection::Replayed,
                revoked,
            }),
            RefreshTransition::FamilyRevoked => Ok(rejected(RefreshRejection::FamilyRevoked)),
            RefreshTransition::Unusable => Ok(rejected(RefreshRejection::Unusable)),
        }
    }

    /// Ends a chain deliberately — a sign-out, or an administrator closing a session.
    ///
    /// Returns how many token rows were revoked. Distinct from the replay response, which is
    /// inside the store's transition because it must be atomic with the check that detected it.
    ///
    /// # Errors
    ///
    /// [`AuthError::NotPermitted`] when the store refuses the write.
    pub async fn revoke(&self, family: FamilyId, clock: &dyn Clock) -> Result<u64, AuthError> {
        self.repository
            .revoke_family(family, clock.now())
            .await
            .map_err(store_failure)
    }

    /// The store, for tests that must assert what was **written** rather than what was returned.
    #[cfg(test)]
    const fn store_for_test(&self) -> &R {
        &self.repository
    }
}

/// A refusal, with the count that only a replay ever carries.
const fn rejected(reason: RefreshRejection) -> RefreshOutcome {
    RefreshOutcome::Rejected { reason, revoked: 0 }
}

/// When a token issued at `now` stops being valid: the token lifetime, or the end of the family,
/// whichever comes first.
///
/// **Not a clamp of a configured value.** [`RefreshLifetime::new`] refuses a lifetime it will not
/// honour; this is the arithmetic that keeps a token from outliving the chain it belongs to, which
/// is a different statement and one no configuration can express.
fn token_expiry(
    now: DateTime<Utc>,
    lifetime: RefreshLifetime,
    family_expires_at: DateTime<Utc>,
) -> DateTime<Utc> {
    (now + lifetime.get()).min(family_expires_at)
}

/// Narrows a store failure to this crate's fieldless error.
///
/// The [`DatabaseError`] is **dropped on purpose**. `AuthError` has no field a driver's message
/// could occupy, which is the property FR-076 asks for and the reason a raw database error can
/// never reach a log line through this path.
fn store_failure(_: DatabaseError) -> AuthError {
    AuthError::NotPermitted
}

#[cfg(test)]
mod tests {
    /// An admission for the refresh flow, without counting one.
    ///
    /// These tests are about the rotation transition. The abuse control that guards it is measured
    /// in `crate::abuse` and in the four-row suite. `Admitted::for_test` is `#[cfg(test)]`, so it
    /// exists in no build a consumer compiles.
    fn admitted() -> crate::abuse::Admitted {
        crate::abuse::Admitted::for_test(crate::abuse::AttemptFlow::TokenRefresh)
    }

    use super::{
        AdvanceRequest, FamilyId, FamilyLifetime, MAX_REFRESH_LIFETIME, NewRefreshFamily, REDACTED,
        RefreshGrant, RefreshLifetime, RefreshOutcome, RefreshRejection, RefreshRotation,
        RefreshToken, RefreshTokenId, RefreshTransition,
    };
    use crate::clock::FixedClock;
    use crate::error::AuthError;
    use crate::opaque::SecretDigest;
    use crate::repository::RefreshTokenRepository;
    use crate::subject::{AuthenticatedSubject, UserId};
    use crate::token::{
        AccessLifetime, AccessTokenIssuer, AccessTokenVerifier, Audience, Issuer, KeyId, KeyRing,
        Scope, ScopeSet, Skew, generate_ed25519,
    };
    use chrono::{DateTime, Duration, Utc};
    use renvor_core::observe::entropy::{EntropySource, EntropyUnavailable};
    use renvor_database::{DatabaseError, DatabaseErrorKind};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

    // NO DIAGNOSTIC IN THIS MODULE INTERPOLATES A TOKEN OR A DIGEST. Same rule as `token`'s tests
    // and for the same reason: a refresh token in a failure message is a live credential in a log.

    // WHAT THIS MODULE PROVES, AND WHAT IT DOES NOT
    //
    // It proves the SERVICE's contract: the order it works in, that a refusal returns nothing and
    // writes nothing, that scopes come from the grant, that a raw token never reaches the store.
    //
    // It does NOT prove atomicity, and it cannot. `Store` below is a `Mutex` over a `Vec` whose
    // `async fn`s contain no `.await`, so two rotations joined with `tokio::join!` run one after
    // the other and no interleaving is ever attempted. The PREVIOUS version of this module had a
    // concurrency test built exactly that way, it passed, and the defect it was supposed to catch
    // shipped anyway. The evidence for the race lives in the four-row suite against real servers
    // on separate connections — `renvor_testkit::refresh` — and nowhere else.

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + seconds, 0).expect("a representable instant")
    }

    fn user() -> UserId {
        UserId::from_bytes([9_u8; 16])
    }

    fn subject() -> AuthenticatedSubject {
        AuthenticatedSubject::new(user())
    }

    fn scopes(names: &[&str]) -> ScopeSet {
        ScopeSet::new(
            names
                .iter()
                .map(|n| Scope::new(n).expect("a well-formed scope")),
        )
        .expect("a bounded scope set")
    }

    /// Deterministic entropy that differs on every call, so two issued tokens differ.
    ///
    /// `FixedEntropy` cannot be used here: it is fixed by design, and a rotation whose "new" token
    /// equalled the old one would pass every assertion below while being completely broken.
    #[derive(Debug, Default)]
    struct Varying {
        calls: AtomicU8,
    }

    impl EntropySource for Varying {
        fn fill(&self, destination: &mut [u8]) -> Result<(), EntropyUnavailable> {
            let nth = self.calls.fetch_add(1, Ordering::SeqCst);
            for (index, slot) in destination.iter_mut().enumerate() {
                *slot = nth
                    .wrapping_mul(37)
                    .wrapping_add(u8::try_from(index & 0xff).unwrap_or(0));
            }
            Ok(())
        }
    }

    #[derive(Debug)]
    struct Family {
        id: FamilyId,
        user: UserId,
        scopes: ScopeSet,
        expires_at: DateTime<Utc>,
        revoked_at: Option<DateTime<Utc>>,
    }

    #[derive(Debug)]
    struct Row {
        family: FamilyId,
        digest: SecretDigest,
        expires_at: DateTime<Utc>,
        consumed_at: Option<DateTime<Utc>>,
        revoked_at: Option<DateTime<Utc>>,
    }

    #[derive(Debug, Default)]
    struct State {
        families: Vec<Family>,
        rows: Vec<Row>,
    }

    /// An in-memory store with the **same decision order** the port's documentation requires.
    ///
    /// One `Mutex` covers both tables, which is what makes `advance` a single transition here —
    /// standing in for the transaction and the two row locks an adapter must take. It stands in
    /// for them; it does not test them.
    #[derive(Debug, Default)]
    struct Store {
        state: Mutex<State>,
        /// Every port method called, in order, so a test can assert what ran before what.
        calls: Mutex<Vec<&'static str>>,
        /// Set to fail the next `advance`, to prove a store failure is an error and not a refusal.
        fail_advance: AtomicUsize,
    }

    impl Store {
        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().expect("not poisoned").clone()
        }

        fn record(&self, name: &'static str) {
            self.calls.lock().expect("not poisoned").push(name);
        }

        fn digests_held(&self) -> Vec<SecretDigest> {
            self.state
                .lock()
                .expect("not poisoned")
                .rows
                .iter()
                .map(|row| row.digest)
                .collect()
        }

        fn live_count(&self) -> usize {
            self.state
                .lock()
                .expect("not poisoned")
                .rows
                .iter()
                .filter(|row| row.consumed_at.is_none() && row.revoked_at.is_none())
                .count()
        }

        fn row_count(&self) -> usize {
            self.state.lock().expect("not poisoned").rows.len()
        }

        fn tombstone(&self, family: FamilyId) -> Option<DateTime<Utc>> {
            self.state
                .lock()
                .expect("not poisoned")
                .families
                .iter()
                .find(|entry| entry.id == family)
                .and_then(|entry| entry.revoked_at)
        }

        fn expiry_of(&self, digest: &SecretDigest) -> Option<DateTime<Utc>> {
            self.state
                .lock()
                .expect("not poisoned")
                .rows
                .iter()
                .find(|row| row.digest.matches(digest))
                .map(|row| row.expires_at)
        }
    }

    impl RefreshTokenRepository for Store {
        async fn begin_family(&self, family: NewRefreshFamily) -> Result<(), DatabaseError> {
            self.record("begin_family");
            let mut state = self.state.lock().expect("not poisoned");
            state.families.push(Family {
                id: family.family,
                user: family.user,
                scopes: family.scopes,
                expires_at: family.family_expires_at,
                revoked_at: None,
            });
            state.rows.push(Row {
                family: family.family,
                digest: family.first_token,
                expires_at: family.token_expires_at,
                consumed_at: None,
                revoked_at: None,
            });
            Ok(())
        }

        async fn grant_for(
            &self,
            digest: &SecretDigest,
        ) -> Result<Option<RefreshGrant>, DatabaseError> {
            self.record("grant_for");
            let state = self.state.lock().expect("not poisoned");
            let Some(row) = state.rows.iter().find(|row| row.digest.matches(digest)) else {
                return Ok(None);
            };
            let family = state
                .families
                .iter()
                .find(|entry| entry.id == row.family)
                .expect("a token row always has its family");
            Ok(Some(RefreshGrant {
                family: family.id,
                user: family.user,
                scopes: family.scopes.clone(),
                family_expires_at: family.expires_at,
            }))
        }

        async fn advance(
            &self,
            request: AdvanceRequest<'_>,
        ) -> Result<RefreshTransition, DatabaseError> {
            self.record("advance");
            if self.fail_advance.load(Ordering::SeqCst) > 0 {
                return Err(DatabaseError::new(DatabaseErrorKind::StatementRejected));
            }
            let mut state = self.state.lock().expect("not poisoned");

            let Some(index) = state
                .rows
                .iter()
                .position(|row| row.digest.matches(request.presented))
            else {
                return Ok(RefreshTransition::Unusable);
            };
            let family_id = state.rows[index].family;
            let family = state
                .families
                .iter()
                .find(|entry| entry.id == family_id)
                .expect("a token row always has its family");

            if family.revoked_at.is_some() {
                return Ok(RefreshTransition::FamilyRevoked);
            }
            if family.expires_at <= request.now {
                return Ok(RefreshTransition::Unusable);
            }

            let row = &state.rows[index];
            if row.consumed_at.is_some() || row.revoked_at.is_some() {
                // THE REPLAY RESPONSE, in the same transition that detected it.
                let mut revoked = 0_u64;
                for entry in state.families.iter_mut() {
                    if entry.id == family_id && entry.revoked_at.is_none() {
                        entry.revoked_at = Some(request.now);
                    }
                }
                for row in state.rows.iter_mut() {
                    if row.family == family_id && row.revoked_at.is_none() {
                        row.revoked_at = Some(request.now);
                        revoked += 1;
                    }
                }
                return Ok(RefreshTransition::Replayed { revoked });
            }
            if row.expires_at <= request.now {
                return Ok(RefreshTransition::Unusable);
            }

            state.rows[index].consumed_at = Some(request.now);
            state.rows.push(Row {
                family: family_id,
                digest: *request.successor,
                expires_at: request.successor_expires_at,
                consumed_at: None,
                revoked_at: None,
            });
            Ok(RefreshTransition::Advanced)
        }

        async fn revoke_family(
            &self,
            family: FamilyId,
            now: DateTime<Utc>,
        ) -> Result<u64, DatabaseError> {
            self.record("revoke_family");
            let mut state = self.state.lock().expect("not poisoned");
            for entry in state.families.iter_mut() {
                // MONOTONIC. An already-set tombstone keeps its instant.
                if entry.id == family && entry.revoked_at.is_none() {
                    entry.revoked_at = Some(now);
                }
            }
            let mut revoked = 0_u64;
            for row in state.rows.iter_mut() {
                if row.family == family && row.revoked_at.is_none() {
                    row.revoked_at = Some(now);
                    revoked += 1;
                }
            }
            Ok(revoked)
        }
    }

    /// The issuer, verifier and clock every test below shares.
    struct Fixture {
        issuer: AccessTokenIssuer,
        verifier: AccessTokenVerifier,
        clock: FixedClock,
        entropy: Varying,
    }

    fn fixture() -> Fixture {
        let entropy = Varying::default();
        let pair = generate_ed25519(KeyId::new("k1").expect("a well-formed key id"), &entropy)
            .expect("a generated key pair");
        let issuer = AccessTokenIssuer::new(
            Issuer::new("https://issuer.test").expect("a well-formed issuer"),
            Audience::new("https://api.test").expect("a well-formed audience"),
            pair.signing,
            AccessLifetime::default(),
        );
        let verifier = AccessTokenVerifier::new(
            Issuer::new("https://issuer.test").expect("a well-formed issuer"),
            [Audience::new("https://api.test").expect("a well-formed audience")],
            KeyRing::new(vec![pair.verifying]).expect("a bounded ring"),
            Skew::default(),
        )
        .expect("a configured verifier");
        Fixture {
            issuer,
            verifier,
            clock: FixedClock::at(at(0)),
            entropy,
        }
    }

    fn rotation(store: Store) -> RefreshRotation<Store> {
        RefreshRotation::new(store, RefreshLifetime::default(), FamilyLifetime::default())
    }

    #[tokio::test]
    async fn a_new_family_holds_exactly_one_usable_token() {
        let f = fixture();
        let service = rotation(Store::default());
        let issued = service
            .begin(
                subject(),
                &scopes(&["read"]),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("a family begins");

        assert_eq!(service.store_for_test().live_count(), 1);
        assert_eq!(service.store_for_test().row_count(), 1);
        // The access token that came with it is genuinely valid.
        let verified = f
            .verifier
            .verify(issued.access.expose(), &f.clock)
            .expect("the access token verifies");
        assert_eq!(verified.subject(), subject());
        assert_eq!(verified.scopes(), &scopes(&["read"]));
    }

    #[tokio::test]
    async fn a_rotation_consumes_the_old_token_and_leaves_one_live_successor() {
        let f = fixture();
        let service = rotation(Store::default());
        let first = service
            .begin(
                subject(),
                &scopes(&["read"]),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("a family begins");

        let outcome = service
            .rotate(
                admitted(),
                &first.refresh.expose(),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("the store answered");
        let RefreshOutcome::Rotated(second) = outcome else {
            panic!("a live token was refused");
        };

        assert_eq!(second.family, first.family, "the chain changed identity");
        assert_ne!(
            second.refresh.digest().as_bytes(),
            first.refresh.digest().as_bytes(),
            "the successor is the same secret as its predecessor"
        );
        assert_eq!(service.store_for_test().row_count(), 2);
        assert_eq!(
            service.store_for_test().live_count(),
            1,
            "exactly one token in a family is usable at a time"
        );
    }

    #[tokio::test]
    async fn a_replay_revokes_the_family_and_reports_the_count() {
        let f = fixture();
        let service = rotation(Store::default());
        let first = service
            .begin(
                subject(),
                &scopes(&["read"]),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("a family begins");
        let wire = first.refresh.expose();

        let RefreshOutcome::Rotated(second) = service
            .rotate(admitted(), &wire, &f.issuer, &f.clock, &f.entropy)
            .await
            .expect("the store answered")
        else {
            panic!("a live token was refused");
        };

        // The SAME token again. ASVS V10.4.5.
        let outcome = service
            .rotate(admitted(), &wire, &f.issuer, &f.clock, &f.entropy)
            .await
            .expect("the store answered");
        let RefreshOutcome::Rejected { reason, revoked } = outcome else {
            panic!("a replayed token was rotated");
        };
        assert_eq!(reason, RefreshRejection::Replayed);
        // TWO, not one. The count is how many token ROWS the response changed, and the family
        // holds two: the predecessor, consumed but not yet revoked, and the live successor.
        // Marking the consumed one is not redundant — `revoked_at` records that the authorisation
        // ended, which `consumed_at` does not say.
        assert_eq!(
            revoked, 2,
            "the replay response left a row in the family unrevoked"
        );
        assert!(
            service.store_for_test().tombstone(first.family).is_some(),
            "the family carries no tombstone after a replay"
        );
        assert_eq!(service.store_for_test().live_count(), 0);

        // And the successor the legitimate client is holding is dead too.
        let after = service
            .rotate(
                admitted(),
                &second.refresh.expose(),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("the store answered");
        let RefreshOutcome::Rejected { reason, .. } = after else {
            panic!("a descendant of a revoked family was rotated");
        };
        assert_eq!(reason, RefreshRejection::FamilyRevoked);
    }

    #[tokio::test]
    async fn a_refusal_returns_no_tokens_and_writes_nothing() {
        let f = fixture();
        let service = rotation(Store::default());
        let first = service
            .begin(
                subject(),
                &scopes(&["read"]),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("a family begins");
        service
            .revoke(first.family, &f.clock)
            .await
            .expect("the family is revoked");

        let before = service.store_for_test().row_count();
        let outcome = service
            .rotate(
                admitted(),
                &first.refresh.expose(),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("the store answered");

        assert!(
            matches!(
                outcome,
                RefreshOutcome::Rejected {
                    reason: RefreshRejection::FamilyRevoked,
                    revoked: 0
                }
            ),
            "a revoked family did not refuse the rotation"
        );
        assert_eq!(
            service.store_for_test().row_count(),
            before,
            "a refused rotation wrote a row"
        );
    }

    #[tokio::test]
    async fn a_malformed_presentation_never_reaches_the_store() {
        let f = fixture();
        let service = rotation(Store::default());
        let outcome = service
            .rotate(
                admitted(),
                "not-a-refresh-token",
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("no store call to fail");
        assert!(matches!(
            outcome,
            RefreshOutcome::Rejected {
                reason: RefreshRejection::Unusable,
                ..
            }
        ));
        assert!(
            service.store_for_test().calls().is_empty(),
            "a malformed value became a query"
        );
    }

    #[tokio::test]
    async fn an_unknown_digest_is_refused_by_the_read_before_anything_is_minted() {
        let f = fixture();
        let service = rotation(Store::default());
        let stranger = RefreshToken::generate(&f.entropy).expect("a generated token");

        let outcome = service
            .rotate(
                admitted(),
                &stranger.expose(),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("the store answered");
        assert!(matches!(
            outcome,
            RefreshOutcome::Rejected {
                reason: RefreshRejection::Unusable,
                ..
            }
        ));
        assert_eq!(
            service.store_for_test().calls(),
            vec!["grant_for"],
            "an unknown digest reached the decision"
        );
    }

    #[tokio::test]
    async fn the_decision_comes_after_the_read_and_nothing_is_written_between_them() {
        let f = fixture();
        let service = rotation(Store::default());
        let first = service
            .begin(
                subject(),
                &scopes(&["read"]),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("a family begins");
        service
            .rotate(
                admitted(),
                &first.refresh.expose(),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("the store answered");

        assert_eq!(
            service.store_for_test().calls(),
            vec!["begin_family", "grant_for", "advance"],
            "the rotation wrote outside its single transition"
        );
    }

    #[tokio::test]
    async fn scopes_come_from_the_grant_and_a_rotation_cannot_widen_them() {
        let f = fixture();
        let service = rotation(Store::default());
        let first = service
            .begin(
                subject(),
                &scopes(&["read"]),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("a family begins");

        let RefreshOutcome::Rotated(second) = service
            .rotate(
                admitted(),
                &first.refresh.expose(),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("the store answered")
        else {
            panic!("a live token was refused");
        };

        let verified = f
            .verifier
            .verify(second.access.expose(), &f.clock)
            .expect("the rotated access token verifies");
        assert_eq!(
            verified.scopes(),
            &scopes(&["read"]),
            "the rotation changed the granted scopes"
        );
        // `rotate` takes no scope parameter at all: there is no argument a caller could widen.
        assert!(
            !verified
                .scopes()
                .grants(&Scope::new("write").expect("a scope"))
        );
    }

    #[tokio::test]
    async fn the_store_never_holds_the_raw_token() {
        let f = fixture();
        let service = rotation(Store::default());
        let first = service
            .begin(
                subject(),
                &scopes(&["read"]),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("a family begins");

        let wire = first.refresh.expose();
        let raw = wire.as_bytes();
        for digest in service.store_for_test().digests_held() {
            assert_ne!(
                digest.as_bytes().as_slice(),
                raw,
                "the store holds the presented value rather than its digest"
            );
        }
        // And the digest it DOES hold is the digest of that token.
        assert!(
            service
                .store_for_test()
                .digests_held()
                .iter()
                .any(|held| held.matches(&first.refresh.digest())),
            "the stored digest is not the digest of the issued token"
        );
    }

    #[test]
    fn debug_and_display_render_no_secret() {
        let entropy = Varying::default();
        let token = RefreshToken::generate(&entropy).expect("a generated token");
        let exposed = token.expose();

        let debug = format!("{token:?}");
        let display = format!("{token}");
        assert!(!debug.contains(&exposed), "Debug rendered the secret");
        assert!(!display.contains(&exposed), "Display rendered the secret");
        assert!(debug.contains(REDACTED));
        assert_eq!(display, REDACTED);
    }

    #[test]
    fn an_identifier_renders_as_hex_and_round_trips() {
        let family = FamilyId::from_bytes([0xab_u8; FamilyId::BYTES]);
        assert_eq!(family.to_string(), "ab".repeat(FamilyId::BYTES));
        assert_eq!(FamilyId::from_bytes(*family.as_bytes()), family);

        let token = RefreshTokenId::from_bytes([0x0f_u8; RefreshTokenId::BYTES]);
        assert_eq!(token.to_string(), "0f".repeat(RefreshTokenId::BYTES));
        assert_eq!(RefreshTokenId::from_bytes(*token.as_bytes()), token);
    }

    #[test]
    fn a_lifetime_outside_the_bounds_is_refused_rather_than_clamped() {
        assert_eq!(
            RefreshLifetime::new(Duration::zero()),
            Err(AuthError::PolicyMisconfigured)
        );
        assert_eq!(
            RefreshLifetime::new(MAX_REFRESH_LIFETIME + Duration::seconds(1)),
            Err(AuthError::PolicyMisconfigured)
        );
        assert_eq!(
            RefreshLifetime::new(MAX_REFRESH_LIFETIME)
                .expect("the ceiling itself is allowed")
                .get(),
            MAX_REFRESH_LIFETIME
        );
        assert_eq!(
            FamilyLifetime::new(Duration::zero()),
            Err(AuthError::PolicyMisconfigured)
        );
        assert_eq!(
            FamilyLifetime::new(MAX_REFRESH_LIFETIME + Duration::seconds(1)),
            Err(AuthError::PolicyMisconfigured)
        );
        assert_eq!(FamilyLifetime::default().get(), MAX_REFRESH_LIFETIME);
        assert_eq!(RefreshLifetime::default().get(), Duration::days(14));
    }

    #[tokio::test]
    async fn a_token_never_outlives_the_family_it_belongs_to() {
        let f = fixture();
        // A chain shorter than one token's lifetime: the token must be cut down to the chain.
        let service = RefreshRotation::new(
            Store::default(),
            RefreshLifetime::new(Duration::days(14)).expect("a bounded lifetime"),
            FamilyLifetime::new(Duration::hours(1)).expect("a bounded chain"),
        );
        let first = service
            .begin(
                subject(),
                &scopes(&["read"]),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("a family begins");

        let expiry = service
            .store_for_test()
            .expiry_of(&first.refresh.digest())
            .expect("the first token was stored");
        assert_eq!(
            expiry,
            at(3600),
            "the first token outlives the chain it belongs to"
        );

        let RefreshOutcome::Rotated(second) = service
            .rotate(
                admitted(),
                &first.refresh.expose(),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("the store answered")
        else {
            panic!("a live token was refused");
        };
        let successor_expiry = service
            .store_for_test()
            .expiry_of(&second.refresh.digest())
            .expect("the successor was stored");
        assert_eq!(
            successor_expiry,
            at(3600),
            "a rotation extended the chain past its absolute end"
        );
    }

    #[tokio::test]
    async fn an_expired_family_refuses_a_rotation() {
        let f = fixture();
        let service = RefreshRotation::new(
            Store::default(),
            RefreshLifetime::new(Duration::hours(2)).expect("a bounded lifetime"),
            FamilyLifetime::new(Duration::hours(1)).expect("a bounded chain"),
        );
        let first = service
            .begin(
                subject(),
                &scopes(&["read"]),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("a family begins");

        let later = FixedClock::at(at(3601));
        let outcome = service
            .rotate(
                admitted(),
                &first.refresh.expose(),
                &f.issuer,
                &later,
                &f.entropy,
            )
            .await
            .expect("the store answered");
        assert!(
            matches!(
                outcome,
                RefreshOutcome::Rejected {
                    reason: RefreshRejection::Unusable,
                    ..
                }
            ),
            "an expired chain was rotated"
        );
    }

    #[tokio::test]
    async fn a_deliberate_revocation_is_monotonic() {
        let f = fixture();
        let service = rotation(Store::default());
        let first = service
            .begin(
                subject(),
                &scopes(&["read"]),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("a family begins");

        let revoked = service
            .revoke(first.family, &f.clock)
            .await
            .expect("the family is revoked");
        assert_eq!(revoked, 1);
        let first_tombstone = service
            .store_for_test()
            .tombstone(first.family)
            .expect("a tombstone");

        let again = service
            .revoke(first.family, &FixedClock::at(at(600)))
            .await
            .expect("a second revocation answers");
        assert_eq!(again, 0, "a second revocation revoked rows a second time");
        assert_eq!(
            service.store_for_test().tombstone(first.family),
            Some(first_tombstone),
            "a second revocation moved the tombstone forward"
        );
    }

    #[tokio::test]
    async fn a_store_failure_is_an_error_and_never_a_refusal() {
        let f = fixture();
        let service = rotation(Store::default());
        let first = service
            .begin(
                subject(),
                &scopes(&["read"]),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect("a family begins");
        service
            .store_for_test()
            .fail_advance
            .store(1, Ordering::SeqCst);

        let error = service
            .rotate(
                admitted(),
                &first.refresh.expose(),
                &f.issuer,
                &f.clock,
                &f.entropy,
            )
            .await
            .expect_err("a failing store must not look like a refusal");
        assert_eq!(error, AuthError::NotPermitted);
        // FIELDLESS. There is nowhere in the error for the driver's text to be.
        assert_eq!(error.to_string(), "the operation is not permitted");
    }

    #[test]
    fn every_rejection_reason_renders_a_static_description() {
        for reason in [
            RefreshRejection::Unusable,
            RefreshRejection::Replayed,
            RefreshRejection::FamilyRevoked,
        ] {
            let rendered = reason.to_string();
            assert!(!rendered.is_empty());
            assert!(
                !rendered.contains("rv-"),
                "a rejection description carries a token prefix"
            );
        }
    }
}
