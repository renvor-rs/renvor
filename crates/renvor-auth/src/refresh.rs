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
//! # The race is decided by the store, not by this module
//!
//! Two concurrent presentations of one *valid* token must produce exactly one rotation. That is not
//! arranged here with a lock; it is a precondition on
//! [`crate::repository::RefreshTokenRepository::consume`], which must be a single conditional
//! statement. The loser of that race observes [`RefreshConsumption`](crate::refresh::RefreshConsumption)`::Replayed`
//! and takes the family
//! down — which is the correct outcome, because from the store's position a second presentation of
//! a consumed token is indistinguishable from theft.

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

/// The longest a refresh token may be configured to live: **thirty days**.
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

/// A row to be written, carrying the digest and never the token.
///
/// # There is nowhere in this type to put the secret
///
/// The field is a [`SecretDigest`]. A caller that wanted to store the raw token would have to
/// change this struct to do it, which is a reviewable act — the same argument
/// [`crate::token::TokenRejection`] makes about rejection reasons, applied to a write.
#[derive(Clone, Debug)]
pub struct NewRefreshToken {
    /// What the store holds in the token's place.
    pub digest: SecretDigest,
    /// The chain this token belongs to.
    pub family: FamilyId,
    /// Whose token it is.
    pub user: UserId,
    /// The privileges a rotation from this token may carry forward.
    pub scopes: ScopeSet,
    /// When it was issued.
    pub issued_at: DateTime<Utc>,
    /// When it stops being valid.
    pub expires_at: DateTime<Utc>,
}

/// What a consumed refresh token grants.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RefreshGrant {
    /// The chain to continue.
    pub family: FamilyId,
    /// Whose token it was.
    pub user: UserId,
    /// The privileges to carry forward. **Never widened** by a rotation.
    pub scopes: ScopeSet,
}

/// What the store's single conditional statement observed.
///
/// The variants are ordered by what they mean to the caller, not by likelihood: the second one is
/// the security event.
#[derive(Clone, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum RefreshConsumption {
    /// The statement consumed the row. This caller won, and no other can.
    Consumed(RefreshGrant),
    /// The row exists and was **already consumed**. ASVS V10.4.5: revoke the family.
    Replayed(FamilyId),
    /// The row exists and its family has already been revoked.
    FamilyRevoked,
    /// No usable row: unknown, or expired. **One answer for both** — an expired token is not a
    /// replay, and telling a presenter which of the two it holds tells them whether the value was
    /// ever real.
    Unusable,
}

/// Why a presented refresh token was refused.
///
/// # This is narrower than what the store saw, deliberately
///
/// [`RefreshConsumption`] distinguishes four states because the *issuer* needs the distinction to
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
    /// The presented token was refused. When the reason is [`RefreshRejection::Replayed`], the
    /// family **has already been revoked** by the time this value exists, and `revoked` says how
    /// many rows that affected — so a caller can assert the response happened rather than trust it.
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
}

impl<R: RefreshTokenRepository> RefreshRotation<R> {
    /// Builds a rotation service.
    pub const fn new(repository: R, lifetime: RefreshLifetime) -> Self {
        Self {
            repository,
            lifetime,
        }
    }

    /// Starts a new family for a subject that has just authenticated, and issues the first pair.
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
        let family = FamilyId::generate(entropy)?;
        self.mint(family, subject, scopes, issuer, clock, entropy)
            .await
    }

    /// Spends `presented` and issues the next pair, or refuses it.
    ///
    /// # The order is the security argument
    ///
    /// The store decides first, in one statement. Only then is anything issued — so a losing racer
    /// never receives a token, and a replay is answered by revocation **before** the caller is told
    /// what happened.
    ///
    /// # Errors
    ///
    /// As [`Self::begin`]. A refusal is **not** an error: it is [`RefreshOutcome::Rejected`],
    /// because being handed a dead credential is an ordinary event and not a fault.
    pub async fn rotate(
        &self,
        presented: &str,
        issuer: &AccessTokenIssuer,
        clock: &dyn Clock,
        entropy: &dyn EntropySource,
    ) -> Result<RefreshOutcome, AuthError> {
        let now = clock.now();

        // A value that is not even the right shape is refused without touching the store. This is
        // not an optimisation: it keeps a malformed presentation from becoming a database query.
        let Some(token) = RefreshToken::from_wire(presented) else {
            return Ok(RefreshOutcome::Rejected {
                reason: RefreshRejection::Unusable,
                revoked: 0,
            });
        };

        let observed = self
            .repository
            .consume(&token.digest(), now)
            .await
            .map_err(store_failure)?;

        match observed {
            RefreshConsumption::Consumed(grant) => {
                let subject = AuthenticatedSubject::new(grant.user);
                // The scopes come from the GRANT, not from the caller. A rotation carries
                // privileges forward; it is not an opportunity to acquire new ones.
                self.mint(grant.family, subject, &grant.scopes, issuer, clock, entropy)
                    .await
                    .map(RefreshOutcome::Rotated)
            }
            RefreshConsumption::Replayed(family) => {
                // ASVS V10.4.5. The revocation happens HERE, before the caller is answered, so
                // there is no window in which a replay has been detected and not yet responded to.
                let revoked = self
                    .repository
                    .revoke_family(family, now)
                    .await
                    .map_err(store_failure)?;
                Ok(RefreshOutcome::Rejected {
                    reason: RefreshRejection::Replayed,
                    revoked,
                })
            }
            RefreshConsumption::FamilyRevoked => Ok(RefreshOutcome::Rejected {
                reason: RefreshRejection::FamilyRevoked,
                revoked: 0,
            }),
            RefreshConsumption::Unusable => Ok(RefreshOutcome::Rejected {
                reason: RefreshRejection::Unusable,
                revoked: 0,
            }),
        }
    }

    /// The store, for tests that must assert what was **written** rather than what was returned.
    #[cfg(test)]
    const fn store_for_test(&self) -> &R {
        &self.repository
    }

    /// Generates a pair, stores the digest, and returns the secrets.
    async fn mint(
        &self,
        family: FamilyId,
        subject: AuthenticatedSubject,
        scopes: &ScopeSet,
        issuer: &AccessTokenIssuer,
        clock: &dyn Clock,
        entropy: &dyn EntropySource,
    ) -> Result<RotatedTokens, AuthError> {
        let now = clock.now();
        let refresh = RefreshToken::generate(entropy)?;
        let access = issuer.issue(subject, scopes, clock, entropy)?;

        self.repository
            .issue(NewRefreshToken {
                digest: refresh.digest(),
                family,
                user: subject.user_id(),
                scopes: scopes.clone(),
                issued_at: now,
                expires_at: now + self.lifetime.get(),
            })
            .await
            .map_err(store_failure)?;

        Ok(RotatedTokens {
            access,
            refresh,
            family,
        })
    }
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
    use super::{
        FamilyId, MAX_REFRESH_LIFETIME, NewRefreshToken, REDACTED, RefreshConsumption,
        RefreshGrant, RefreshLifetime, RefreshOutcome, RefreshRejection, RefreshRotation,
        RefreshToken,
    };
    use crate::clock::FixedClock;
    use crate::error::AuthError;
    use crate::opaque::SecretDigest;
    use crate::repository::RefreshTokenRepository;
    use crate::subject::{AuthenticatedSubject, UserId};
    use crate::token::{
        AccessLifetime, AccessTokenIssuer, AccessTokenVerifier, Audience, Issuer, KeyId, KeyRing,
        Scope, ScopeSet, Skew, VerifyingKey, generate_ed25519,
    };
    use chrono::{DateTime, Duration, Utc};
    use renvor_core::observe::entropy::{EntropySource, EntropyUnavailable};
    use renvor_database::DatabaseError;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

    // NO DIAGNOSTIC IN THIS MODULE INTERPOLATES A TOKEN OR A DIGEST. Same rule as `token`'s tests
    // and for the same reason: a refresh token in a failure message is a live credential in a log.

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
    struct Row {
        digest: SecretDigest,
        family: FamilyId,
        user: UserId,
        scopes: ScopeSet,
        expires_at: DateTime<Utc>,
        consumed_at: Option<DateTime<Utc>>,
        revoked_at: Option<DateTime<Utc>>,
    }

    /// An in-memory store with the **same decision order** the port's documentation requires.
    ///
    /// The `Mutex` is what makes `consume` a single atomic decision here, standing in for the one
    /// conditional `UPDATE` an adapter must use. What this proves is the **contract** — that a
    /// second consumption reports a replay. That two real connections race correctly on all four
    /// rows is a property of the adapters' SQL and belongs to the four-row suites; it is **not**
    /// claimed by anything in this module.
    #[derive(Debug, Default)]
    struct Store {
        rows: Mutex<Vec<Row>>,
        queries: AtomicUsize,
    }

    impl Store {
        fn raw_values_held(&self) -> Vec<SecretDigest> {
            self.rows
                .lock()
                .expect("not poisoned")
                .iter()
                .map(|row| row.digest)
                .collect()
        }

        fn queries(&self) -> usize {
            self.queries.load(Ordering::SeqCst)
        }

        fn live_count(&self) -> usize {
            self.rows
                .lock()
                .expect("not poisoned")
                .iter()
                .filter(|row| row.consumed_at.is_none() && row.revoked_at.is_none())
                .count()
        }
    }

    impl RefreshTokenRepository for Store {
        async fn issue(&self, record: NewRefreshToken) -> Result<(), DatabaseError> {
            self.rows.lock().expect("not poisoned").push(Row {
                digest: record.digest,
                family: record.family,
                user: record.user,
                scopes: record.scopes,
                expires_at: record.expires_at,
                consumed_at: None,
                revoked_at: None,
            });
            Ok(())
        }

        async fn consume(
            &self,
            digest: &SecretDigest,
            now: DateTime<Utc>,
        ) -> Result<RefreshConsumption, DatabaseError> {
            self.queries.fetch_add(1, Ordering::SeqCst);
            let mut rows = self.rows.lock().expect("not poisoned");
            let Some(row) = rows.iter_mut().find(|row| row.digest.matches(digest)) else {
                return Ok(RefreshConsumption::Unusable);
            };
            if row.revoked_at.is_some() {
                return Ok(RefreshConsumption::FamilyRevoked);
            }
            if row.consumed_at.is_some() {
                return Ok(RefreshConsumption::Replayed(row.family));
            }
            if row.expires_at <= now {
                return Ok(RefreshConsumption::Unusable);
            }
            row.consumed_at = Some(now);
            Ok(RefreshConsumption::Consumed(RefreshGrant {
                family: row.family,
                user: row.user,
                scopes: row.scopes.clone(),
            }))
        }

        async fn revoke_family(
            &self,
            family: FamilyId,
            now: DateTime<Utc>,
        ) -> Result<u64, DatabaseError> {
            let mut rows = self.rows.lock().expect("not poisoned");
            let mut revoked = 0_u64;
            for row in rows.iter_mut().filter(|row| row.family == family) {
                if row.revoked_at.is_none() {
                    row.revoked_at = Some(now);
                    revoked += 1;
                }
            }
            Ok(revoked)
        }
    }

    struct Fixture {
        rotation: RefreshRotation<Store>,
        issuer: AccessTokenIssuer,
        verifying: VerifyingKey,
        entropy: Varying,
    }

    fn fixture() -> Fixture {
        let entropy = Varying::default();
        let pair = generate_ed25519(KeyId::new("k1").expect("a well-formed key id"), &entropy)
            .expect("a generated key pair");
        Fixture {
            rotation: RefreshRotation::new(Store::default(), RefreshLifetime::default()),
            issuer: AccessTokenIssuer::new(
                Issuer::new("https://renvor.test/issuer").expect("a well-formed issuer"),
                Audience::new("https://renvor.test/api").expect("a well-formed audience"),
                pair.signing,
                AccessLifetime::default(),
            ),
            verifying: pair.verifying,
            entropy,
        }
    }

    impl Fixture {
        fn store(&self) -> &Store {
            // The service owns the store; reaching it is how a test asserts what was WRITTEN
            // rather than only what was returned.
            self.rotation.store_for_test()
        }
    }

    // -- issuing --------------------------------------------------------------------------------

    #[tokio::test]
    async fn a_new_family_issues_a_pair_and_stores_only_the_digest() {
        let f = fixture();
        let clock = FixedClock::at(at(0));
        let issued = f
            .rotation
            .begin(subject(), &scopes(&["read"]), &f.issuer, &clock, &f.entropy)
            .await
            .expect("a first pair");

        assert!(
            f.store().live_count() == 1,
            "the store does not hold exactly one live token"
        );

        // FR-041. The store holds the DIGEST; the raw token is not recoverable from it.
        let held = f.store().raw_values_held();
        assert!(held.len() == 1, "the store holds the wrong number of rows");
        assert!(
            held[0].matches(&issued.refresh.digest()),
            "the store does not hold the digest of the issued token"
        );
        assert!(
            !format!("{:?}", held[0]).contains(&issued.refresh.expose()),
            "the raw refresh token is recoverable from what the store holds"
        );
    }

    // -- rotation -------------------------------------------------------------------------------

    #[tokio::test]
    async fn a_rotation_spends_the_old_token_and_issues_a_different_one_in_the_same_family() {
        let f = fixture();
        let clock = FixedClock::at(at(0));
        let first = f
            .rotation
            .begin(subject(), &scopes(&["read"]), &f.issuer, &clock, &f.entropy)
            .await
            .expect("a first pair");

        let outcome = f
            .rotation
            .rotate(&first.refresh.expose(), &f.issuer, &clock, &f.entropy)
            .await
            .expect("a store that answered");

        let RefreshOutcome::Rotated(second) = outcome else {
            panic!("a valid refresh token was refused")
        };
        assert!(
            second.refresh.expose() != first.refresh.expose(),
            "the rotation returned the same refresh token it was given"
        );
        assert!(
            second.family == first.family,
            "the rotation started a new family"
        );
        assert!(
            f.store().live_count() == 1,
            "the rotation did not leave exactly one live token"
        );
    }

    #[tokio::test]
    async fn a_rotation_carries_the_scopes_forward_and_the_presenter_cannot_widen_them() {
        let f = fixture();
        let clock = FixedClock::at(at(0));
        let granted = scopes(&["orders.read"]);
        let first = f
            .rotation
            .begin(subject(), &granted, &f.issuer, &clock, &f.entropy)
            .await
            .expect("a first pair");

        let RefreshOutcome::Rotated(second) = f
            .rotation
            .rotate(&first.refresh.expose(), &f.issuer, &clock, &f.entropy)
            .await
            .expect("a store that answered")
        else {
            panic!("a valid refresh token was refused")
        };

        // `rotate` takes no scope parameter at all — the privileges come from the stored grant.
        // This asserts the consequence: the new access token carries exactly what was granted.
        let verifier = verifier_for(f.verifying.clone());
        let verified = verifier
            .verify(second.access.expose(), &clock)
            .expect("the access token this rotation just issued");
        assert!(
            verified.scopes() == &granted,
            "a rotation changed the granted scopes"
        );
    }

    // -- replay, ASVS V10.4.5 ---------------------------------------------------------------------

    #[tokio::test]
    async fn replaying_a_spent_token_revokes_the_entire_family() {
        let f = fixture();
        let clock = FixedClock::at(at(0));
        let first = f
            .rotation
            .begin(subject(), &scopes(&["read"]), &f.issuer, &clock, &f.entropy)
            .await
            .expect("a first pair");
        let RefreshOutcome::Rotated(second) = f
            .rotation
            .rotate(&first.refresh.expose(), &f.issuer, &clock, &f.entropy)
            .await
            .expect("a store that answered")
        else {
            panic!("a valid refresh token was refused")
        };

        // The stolen copy of the FIRST token is presented after the legitimate rotation.
        let replay = f
            .rotation
            .rotate(&first.refresh.expose(), &f.issuer, &clock, &f.entropy)
            .await
            .expect("a store that answered");

        let RefreshOutcome::Rejected { reason, revoked } = replay else {
            panic!("a replayed refresh token was accepted")
        };
        assert!(
            reason == RefreshRejection::Replayed,
            "a replay was not reported as one"
        );
        assert!(revoked >= 1, "a replay revoked nothing at all");
        assert!(
            f.store().live_count() == 0,
            "a live token survived the family revocation"
        );

        // And the consequence that matters: the token the LEGITIMATE client is holding is now dead.
        let after = f
            .rotation
            .rotate(&second.refresh.expose(), &f.issuer, &clock, &f.entropy)
            .await
            .expect("a store that answered");
        assert!(
            matches!(
                after,
                RefreshOutcome::Rejected {
                    reason: RefreshRejection::FamilyRevoked,
                    ..
                }
            ),
            "a sibling of a revoked family was still usable"
        );
    }

    #[tokio::test]
    async fn two_presentations_of_one_valid_token_produce_exactly_one_rotation() {
        // WHAT THIS PROVES: the port's contract. The store decides in one guarded step, so the
        // second presentation observes a replay rather than minting a second token.
        //
        // WHAT IT DOES NOT PROVE: that two real database connections race correctly. That is a
        // property of each adapter's SQL and belongs to the four-row suites. Nothing here is
        // evidence about PostgreSQL or MySQL, and this comment exists so no reader mistakes it for
        // some.
        let f = fixture();
        let clock = FixedClock::at(at(0));
        let first = f
            .rotation
            .begin(subject(), &scopes(&["read"]), &f.issuer, &clock, &f.entropy)
            .await
            .expect("a first pair");
        let presented = first.refresh.expose();

        let (a, b) = tokio::join!(
            f.rotation.rotate(&presented, &f.issuer, &clock, &f.entropy),
            f.rotation.rotate(&presented, &f.issuer, &clock, &f.entropy),
        );
        let outcomes = [
            a.expect("a store that answered"),
            b.expect("a store that answered"),
        ];
        let rotated = outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RefreshOutcome::Rotated(_)))
            .count();
        assert!(
            rotated == 1,
            "one token was spent more than once, or not at all"
        );
        assert!(
            outcomes.iter().any(|outcome| matches!(
                outcome,
                RefreshOutcome::Rejected {
                    reason: RefreshRejection::Replayed,
                    ..
                }
            )),
            "the losing presentation was not reported as a replay"
        );
        assert!(
            f.store().live_count() == 0,
            "the replay response did not take the family down"
        );
    }

    // -- refusals that are not replays ------------------------------------------------------------

    #[tokio::test]
    async fn an_unknown_token_is_refused_and_revokes_nothing() {
        let f = fixture();
        let clock = FixedClock::at(at(0));
        f.rotation
            .begin(subject(), &scopes(&["read"]), &f.issuer, &clock, &f.entropy)
            .await
            .expect("a first pair");

        let stranger = RefreshToken::generate(&f.entropy).expect("a generated token");
        let outcome = f
            .rotation
            .rotate(&stranger.expose(), &f.issuer, &clock, &f.entropy)
            .await
            .expect("a store that answered");

        assert!(
            matches!(
                outcome,
                RefreshOutcome::Rejected {
                    reason: RefreshRejection::Unusable,
                    revoked: 0
                }
            ),
            "an unknown token was not refused as unusable, or it revoked something"
        );
        assert!(
            f.store().live_count() == 1,
            "an unknown token disturbed a live family"
        );
    }

    #[tokio::test]
    async fn an_expired_token_is_refused_and_is_not_treated_as_a_replay() {
        let f = fixture();
        let first = f
            .rotation
            .begin(
                subject(),
                &scopes(&["read"]),
                &f.issuer,
                &FixedClock::at(at(0)),
                &f.entropy,
            )
            .await
            .expect("a first pair");

        let past_expiry = at(RefreshLifetime::default().get().num_seconds() + 1);
        let outcome = f
            .rotation
            .rotate(
                &first.refresh.expose(),
                &f.issuer,
                &FixedClock::at(past_expiry),
                &f.entropy,
            )
            .await
            .expect("a store that answered");

        // An expired token is a dead credential, NOT evidence of theft. Revoking the family here
        // would log a user out for the ordinary act of coming back after a fortnight.
        assert!(
            matches!(
                outcome,
                RefreshOutcome::Rejected {
                    reason: RefreshRejection::Unusable,
                    revoked: 0
                }
            ),
            "an expired token was treated as a replay"
        );
    }

    #[tokio::test]
    async fn a_malformed_presentation_never_reaches_the_store() {
        let f = fixture();
        let clock = FixedClock::at(at(0));
        for malformed in ["", "not-hex", &"z".repeat(64), &"a".repeat(63)] {
            let outcome = f
                .rotation
                .rotate(malformed, &f.issuer, &clock, &f.entropy)
                .await
                .expect("a store that answered");
            assert!(
                matches!(
                    outcome,
                    RefreshOutcome::Rejected {
                        reason: RefreshRejection::Unusable,
                        revoked: 0
                    }
                ),
                "a malformed presentation was not refused"
            );
        }
        assert!(
            f.store().queries() == 0,
            "a malformed presentation was turned into a database query"
        );
    }

    // -- redaction and configuration ----------------------------------------------------------------

    #[tokio::test]
    async fn a_refresh_token_never_renders_itself() {
        let f = fixture();
        let issued = f
            .rotation
            .begin(
                subject(),
                &scopes(&["read"]),
                &f.issuer,
                &FixedClock::at(at(0)),
                &f.entropy,
            )
            .await
            .expect("a first pair");

        let debug = format!("{:?}", issued.refresh);
        let display = format!("{}", issued.refresh);
        assert!(
            debug.contains(REDACTED),
            "Debug omitted the redaction placeholder"
        );
        assert!(
            !debug.contains(&issued.refresh.expose()),
            "Debug rendered the refresh token"
        );
        assert!(
            display == REDACTED,
            "Display was not exactly the redaction placeholder"
        );

        // And the enclosing struct, which is what a caller is most likely to log.
        let whole = format!("{issued:?}");
        assert!(
            !whole.contains(&issued.refresh.expose()),
            "the rotated pair rendered its refresh token"
        );
        assert!(
            !whole.contains(issued.access.expose()),
            "the rotated pair rendered its access token"
        );
    }

    #[test]
    fn a_rejection_has_nowhere_to_put_a_credential() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<RefreshRejection>();
        let rendered = format!(
            "{:?} {}",
            RefreshRejection::Replayed,
            RefreshRejection::Unusable
        );
        assert!(
            rendered.is_ascii(),
            "a rejection rendered something unexpected"
        );
    }

    #[test]
    fn a_lifetime_out_of_range_is_refused_rather_than_clamped() {
        assert!(
            RefreshLifetime::new(MAX_REFRESH_LIFETIME + Duration::seconds(1)).is_err(),
            "an over-long refresh lifetime was accepted"
        );
        assert!(
            RefreshLifetime::new(Duration::zero()).is_err(),
            "a zero lifetime was accepted"
        );
        assert!(
            RefreshLifetime::new(MAX_REFRESH_LIFETIME).is_ok(),
            "the ceiling was refused"
        );
        assert!(
            RefreshLifetime::new(-Duration::days(1)).unwrap_err() == AuthError::PolicyMisconfigured,
            "a lifetime refusal used the wrong error variant"
        );
    }

    #[test]
    fn family_identifiers_come_from_the_entropy_port_and_differ() {
        let entropy = Varying::default();
        let first = FamilyId::generate(&entropy).expect("a generated family");
        let second = FamilyId::generate(&entropy).expect("a generated family");
        assert!(
            first != second,
            "two generated family identifiers were equal"
        );
        assert!(
            FamilyId::from_bytes(*first.as_bytes()) == first,
            "a family identifier did not round trip through its bytes"
        );
        assert!(
            first.to_string().len() == FamilyId::BYTES * 2,
            "a family identifier does not render as fixed-width hex"
        );
    }

    #[test]
    fn a_refresh_token_is_the_refresh_kind_and_nothing_else() {
        let entropy = Varying::default();
        let token = RefreshToken::generate(&entropy).expect("a generated token");
        assert!(
            RefreshToken::from_wire(&token.expose()).is_some(),
            "a generated refresh token did not survive its own wire form"
        );
        assert!(
            RefreshToken::from_wire("").is_none(),
            "an empty presentation parsed"
        );
        assert!(
            RefreshToken::from_wire(&token.expose().to_uppercase()).is_none(),
            "an upper-case presentation parsed, so two renderings would name one token"
        );
    }

    /// Builds a verifier for `key`, for the one test that must read a claim back out.
    fn verifier_for(key: VerifyingKey) -> AccessTokenVerifier {
        AccessTokenVerifier::new(
            Issuer::new("https://renvor.test/issuer").expect("a well-formed issuer"),
            [Audience::new("https://renvor.test/api").expect("a well-formed audience")],
            KeyRing::new(vec![key]).expect("a bounded ring"),
            Skew::default(),
        )
        .expect("a configured verifier")
    }
}
