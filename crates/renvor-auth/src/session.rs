//! Session lifecycle: creation, validation, rotation, revocation, and the concurrency bound.
//!
//! # Session fixation is impossible here, rather than prevented here
//!
//! The classic fixation attack hands a victim an identifier and waits for them to authenticate
//! under it. No function in this module accepts a session identifier from a caller and turns it
//! into a live session: [`SessionService::begin`] **generates** one, and the only other input it
//! takes is the `Cookie` header of the request that authenticated — which it uses to *revoke* a
//! pre-login session, never to adopt one. There is no code path to close, because there is no code
//! path.
//!
//! # Every liveness question is one conditional `UPDATE`
//!
//! `SELECT` then `UPDATE` cannot promise that two concurrent revocations produce exactly one
//! winner without depending on the isolation level — which `contracts/database-portability.md` §3
//! forbids and which differs between PostgreSQL (`READ COMMITTED`) and MySQL (`REPEATABLE READ`).
//! So expiry and revocation live in `WHERE` clauses, exactly as batch C's single-use token
//! consumption does.
//!
//! # Revoke before create, never the reverse
//!
//! [`SessionService::rotate`] revokes the old row **first**. If the create then fails, the subject
//! is logged out — annoying and safe. The other order fails *open*: a create that succeeds
//! followed by a revoke that fails leaves two live sessions, one of them the identifier the
//! rotation existed to retire.
//!
//! # The timeouts, and which parts of NIST SP 800-63B-4 are quoted rather than paraphrased
//!
//! §2.2.3 (AAL2): *"A definite reauthentication overall timeout **SHALL** be established, which
//! **SHOULD** be no more than 24 hours at AAL2."* and *"The inactivity timeout **SHOULD** be no
//! more than 1 hour."*
//!
//! Two different obligations, treated differently:
//!
//! - *A timeout **SHALL** be established.* [`SessionPolicy`] has no "unlimited" representation, so
//!   a deployment cannot fail to establish one. That is the SHALL, enforced structurally.
//! - *It **SHOULD** be no more than 24 hours / 1 hour.* [`SessionPolicy::new`] **refuses** longer
//!   values. **This implements a SHOULD as a refusal, which is stricter than the document
//!   requires. It is a decision, not a citation** — the reasoning is that a framework which
//!   permits a 30-day session by configuration will be deployed with one, and AAL1's 30-day
//!   allowance describes a lower assurance level than a password-plus-session design targets.
//!
//! The AAL3 numbers (§2.3.3, *"the overall timeout for reauthentication **SHALL** be no more than
//! 12 hours"*, inactivity *"**SHOULD** be no more than 15 minutes"*) are stricter still and are
//! not imposed, because this crate does not implement AAL3 authenticators.

use core::future::Future;
use core::num::NonZeroU32;

use chrono::{DateTime, Utc};
use renvor_core::observe::entropy::EntropySource;
use renvor_database::DatabaseError;

use crate::cookie::{CookiePolicy, CookieRejection, SetCookie};
use crate::error::AuthError;
use crate::opaque::{Opaque, OpaqueKind, SecretDigest};
use crate::service::ServiceError;
use crate::subject::{AuthenticatedSubject, UserId};

/// A live session row, as the domain sees it.
///
/// **Carries no identifier.** The digest is what the repository is keyed by and the raw secret
/// exists only in the client's cookie, so there is nothing here that could be logged back into a
/// working session.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SessionRecord {
    /// Whose session this is.
    pub user_id: UserId,
    /// When it was established. The **absolute** timeout is measured from here.
    pub created_at: DateTime<Utc>,
    /// When it was last used. The **inactivity** timeout is measured from here.
    pub last_seen_at: DateTime<Utc>,
}

/// A live session, addressable for eviction.
#[derive(Clone, Copy, Debug)]
pub struct SessionHandle {
    /// The stored digest — the row key. Not the secret.
    pub digest: SecretDigest,
    /// When it was last used, so a caller can see the eviction order it was given.
    pub last_seen_at: DateTime<Utc>,
}

/// How long sessions live and how many a subject may hold.
///
/// See the module header for which NIST obligations this enforces and which it exceeds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SessionPolicy {
    idle: chrono::Duration,
    absolute: chrono::Duration,
    concurrent: NonZeroU32,
}

impl SessionPolicy {
    /// NIST SP 800-63B-4 §2.2.3's AAL2 inactivity ceiling: **1 hour**.
    pub const AAL2_INACTIVITY_CEILING: chrono::Duration = chrono::Duration::hours(1);
    /// NIST SP 800-63B-4 §2.2.3's AAL2 overall ceiling: **24 hours**.
    pub const AAL2_OVERALL_CEILING: chrono::Duration = chrono::Duration::hours(24);

    /// Builds a policy.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when a duration is not positive, when either exceeds its
    /// AAL2 ceiling, or when `idle` exceeds `absolute`. The last is not pedantry: an inactivity
    /// window longer than the overall window can never be reached, so it would read as a
    /// configured control that does nothing.
    ///
    /// **Refused rather than clamped.** Clamping lets an operator believe they configured
    /// something they did not.
    pub fn new(
        idle: chrono::Duration,
        absolute: chrono::Duration,
        concurrent: NonZeroU32,
    ) -> Result<Self, AuthError> {
        let positive = idle > chrono::Duration::zero() && absolute > chrono::Duration::zero();
        let within =
            idle <= Self::AAL2_INACTIVITY_CEILING && absolute <= Self::AAL2_OVERALL_CEILING;
        if !positive || !within || idle > absolute {
            return Err(AuthError::PolicyMisconfigured);
        }
        Ok(Self {
            idle,
            absolute,
            concurrent,
        })
    }

    /// The instant before which a session counts as idle.
    #[must_use]
    pub fn idle_cutoff(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        now - self.idle
    }

    /// The instant before which a session counts as too old, whatever its activity.
    #[must_use]
    pub fn absolute_cutoff(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        now - self.absolute
    }

    /// How many live sessions a subject may hold.
    #[must_use]
    pub const fn concurrent(&self) -> u32 {
        self.concurrent.get()
    }

    /// How long the cookie should be allowed to persist: the **absolute** window.
    ///
    /// The cookie must not outlive the server row it points at. Using the idle window instead
    /// would expire the cookie on a session that is still live server-side.
    #[must_use]
    pub const fn cookie_max_age(&self) -> chrono::Duration {
        self.absolute
    }
}

impl Default for SessionPolicy {
    /// 30 minutes idle, 12 hours overall, 5 concurrent — inside every AAL2 ceiling above.
    fn default() -> Self {
        Self {
            idle: chrono::Duration::minutes(30),
            absolute: chrono::Duration::hours(12),
            concurrent: NonZeroU32::new(5).expect("5 is not zero"),
        }
    }
}

/// The persistence port for sessions.
///
/// Nothing here names a driver. See [`crate::repository`] for why the ports live in this crate and
/// their implementations do not.
pub trait SessionRepository: Send + Sync {
    /// Stores a new session keyed by `digest`.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn create(
        &self,
        user_id: UserId,
        digest: &SecretDigest,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), DatabaseError>> + Send;

    /// Confirms the session is live at `now` **and** refreshes its activity, in one statement.
    ///
    /// Live means: present, not revoked, `last_seen_at` after `idle_cutoff`, and `created_at`
    /// after `absolute_cutoff`. Returns `None` for every other case — unknown, revoked, idle-timed
    /// out, or past its overall timeout — because which one it was is information the presenter of
    /// a stale cookie does not need.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn touch(
        &self,
        digest: &SecretDigest,
        now: DateTime<Utc>,
        idle_cutoff: DateTime<Utc>,
        absolute_cutoff: DateTime<Utc>,
    ) -> impl Future<Output = Result<Option<SessionRecord>, DatabaseError>> + Send;

    /// Revokes one session.
    ///
    /// Returns `true` **only when this call revoked a row that was live**, so two concurrent
    /// logouts produce exactly one `true`. An already-revoked or unknown session is `false`, not an
    /// error: it is not usable either way, which is what logout is for.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn revoke(
        &self,
        digest: &SecretDigest,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<bool, DatabaseError>> + Send;

    /// Revokes every live session for a subject, returning how many.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn revoke_all_for(
        &self,
        user_id: UserId,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<u64, DatabaseError>> + Send;

    /// The subject's live sessions, **least recently seen first**.
    ///
    /// The order is part of the contract, not an incidental property of a query: it is the
    /// eviction order [`SessionService::begin`] applies, and a repository that returned them
    /// newest-first would silently evict the session the subject is using right now.
    ///
    /// # Errors
    ///
    /// Any [`DatabaseError`].
    fn live_for(
        &self,
        user_id: UserId,
        idle_cutoff: DateTime<Utc>,
        absolute_cutoff: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<SessionHandle>, DatabaseError>> + Send;
}

/// Whether a request carried a live session.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SessionOutcome {
    /// It did.
    Live(AuthenticatedSubject),
    /// It did not. The reason is **operator-facing**; the requester is told one thing.
    Rejected(SessionRejection),
}

/// Why a request did not carry a live session.
///
/// Never rendered to a requester. Every variant maps to the same
/// [`AuthError::CredentialNoLongerValid`], the two-value split
/// [`crate::service::DispatchOutcome`] introduced: loud to the caller, uniform to the requester.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum SessionRejection {
    /// The `Cookie` header did not yield a well-formed identifier.
    Cookie(CookieRejection),
    /// The identifier was well-formed and no live session matched it — **unknown, expired by
    /// inactivity, expired overall, revoked, replaced, or replayed after logout**. One variant for
    /// all six, because distinguishing them tells the holder of a dead cookie which kind of dead
    /// it is.
    NotLive,
}

/// What starting a session did besides start one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Established {
    /// Whether a session presented on the authenticating request was revoked.
    pub replaced_presented: bool,
    /// How many of the subject's stalest sessions the concurrency bound evicted.
    pub evicted: u64,
}

/// What ending a session did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LogoutOutcome {
    /// This call revoked a live server-side session.
    Ended,
    /// There was no live server-side session to revoke.
    ///
    /// Still a successful logout: the goal is that no usable session remains, and none does.
    AlreadyEnded,
}

/// Starts, validates, rotates, and ends sessions.
///
/// # `now` is a parameter, not a field
///
/// Matches [`crate::AuthenticationService`], whose operations already take the instant. Expiry is
/// therefore evaluated against a value a test supplies, and there is no wall-clock read anywhere
/// in this module to move time past.
pub struct SessionService<S> {
    sessions: S,
    policy: SessionPolicy,
    cookies: CookiePolicy,
    entropy: std::sync::Arc<dyn EntropySource + Send + Sync>,
}

impl<S> core::fmt::Debug for SessionService<S> {
    /// Names the type and its policy. The entropy source and the repository are not rendered.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SessionService")
            .field("policy", &self.policy)
            .field("cookies", &self.cookies)
            .finish_non_exhaustive()
    }
}

impl<S: SessionRepository> SessionService<S> {
    /// Builds the service.
    #[must_use]
    pub fn new(
        sessions: S,
        policy: SessionPolicy,
        cookies: CookiePolicy,
        entropy: std::sync::Arc<dyn EntropySource + Send + Sync>,
    ) -> Self {
        Self {
            sessions,
            policy,
            cookies,
            entropy,
        }
    }

    /// The configured policy.
    #[must_use]
    pub const fn policy(&self) -> SessionPolicy {
        self.policy
    }

    /// Starts a session for a subject that has just authenticated.
    ///
    /// # This is the fixation defence, and it is structural
    ///
    /// `presented` is the authenticating request's `Cookie` header. It is used to **revoke** a
    /// session the subject arrived holding — never to adopt one. The new identifier comes from
    /// [`Opaque::generate`] and from nowhere else, so there is no argument through which an
    /// attacker-chosen identifier could become a live session.
    ///
    /// # Errors
    ///
    /// [`ServiceError::Storage`], or [`ServiceError::Refused`] with
    /// [`AuthError::EntropyUnavailable`]. **No cookie is returned on either**, so a caller cannot
    /// tell a client it is signed in when no row was written.
    pub async fn begin(
        &self,
        subject: AuthenticatedSubject,
        presented: &str,
        now: DateTime<Utc>,
    ) -> Result<(SetCookie, Established), ServiceError> {
        let user = subject.user_id();

        // Retire whatever the subject arrived holding. A malformed or absent cookie is nothing to
        // retire, which is why a rejection here is not an error.
        let replaced_presented = match crate::cookie::read(presented) {
            Ok(old) => self.sessions.revoke(&SecretDigest::of(&old), now).await?,
            Err(_) => false,
        };

        let evicted = self.enforce_bound(user, now).await?;

        let secret = Opaque::generate(OpaqueKind::Session, self.entropy.as_ref())?;
        // The DIGEST is stored; the secret leaves in the cookie and is never written down.
        self.sessions
            .create(user, &SecretDigest::of(&secret), now)
            .await?;

        Ok((
            crate::cookie::issue(&secret, self.cookies, self.policy.cookie_max_age()),
            Established {
                replaced_presented,
                evicted,
            },
        ))
    }

    /// Revokes the subject's stalest sessions until one more will fit.
    ///
    /// # Eviction rather than refusal
    ///
    /// The alternative — refusing the new session once the bound is reached — locks a subject out
    /// of the device in front of them because of sessions on devices they no longer have. A
    /// control users route around is not a control, so the bound evicts **least recently seen
    /// first** and reports how many, for the audit port batch I adds.
    async fn enforce_bound(&self, user: UserId, now: DateTime<Utc>) -> Result<u64, ServiceError> {
        let live = self
            .sessions
            .live_for(
                user,
                self.policy.idle_cutoff(now),
                self.policy.absolute_cutoff(now),
            )
            .await?;
        let bound = self.policy.concurrent() as usize;
        let Some(excess) = live.len().checked_sub(bound - 1) else {
            return Ok(0);
        };
        let mut evicted = 0_u64;
        for handle in live.iter().take(excess) {
            if self.sessions.revoke(&handle.digest, now).await? {
                evicted += 1;
            }
        }
        Ok(evicted)
    }

    /// Authenticates a request from its `Cookie` header, refreshing activity if it succeeds.
    ///
    /// # A storage failure is an `Err`, not a rejection
    ///
    /// Returning [`SessionOutcome::Rejected`] when the database is unreachable would report an
    /// outage as an ordinary sign-out, and the transport above would answer `401` where `503` is
    /// the truth. The request is unauthenticated either way — the distinction is for the operator.
    ///
    /// # Errors
    ///
    /// [`ServiceError::Storage`] only.
    pub async fn authenticate(
        &self,
        presented: &str,
        now: DateTime<Utc>,
    ) -> Result<SessionOutcome, ServiceError> {
        let secret = match crate::cookie::read(presented) {
            Ok(secret) => secret,
            Err(why) => return Ok(SessionOutcome::Rejected(SessionRejection::Cookie(why))),
        };
        let live = self
            .sessions
            .touch(
                &SecretDigest::of(&secret),
                now,
                self.policy.idle_cutoff(now),
                self.policy.absolute_cutoff(now),
            )
            .await?;
        Ok(live.map_or(
            SessionOutcome::Rejected(SessionRejection::NotLive),
            |record| SessionOutcome::Live(AuthenticatedSubject::new(record.user_id)),
        ))
    }

    /// Replaces the session identifier, keeping the subject signed in.
    ///
    /// For a privilege-boundary change — a role grant, a password change, a step-up. The old
    /// identifier stops working before the new one starts; see the module header for why that
    /// order is the only safe one.
    ///
    /// Returns `None` when there was no live session to rotate, together with an **expiry** cookie:
    /// the client's copy is cleared rather than left pointing at nothing.
    ///
    /// # Errors
    ///
    /// [`ServiceError::Storage`], or [`ServiceError::Refused`] with
    /// [`AuthError::EntropyUnavailable`].
    pub async fn rotate(
        &self,
        presented: &str,
        now: DateTime<Utc>,
    ) -> Result<(SetCookie, Option<AuthenticatedSubject>), ServiceError> {
        let Ok(secret) = crate::cookie::read(presented) else {
            return Ok((crate::cookie::expire(self.cookies), None));
        };
        let digest = SecretDigest::of(&secret);

        // Read the owner BEFORE revoking, because after revocation the row is no longer live and
        // `touch` would decline to name it.
        let Some(record) = self
            .sessions
            .touch(
                &digest,
                now,
                self.policy.idle_cutoff(now),
                self.policy.absolute_cutoff(now),
            )
            .await?
        else {
            return Ok((crate::cookie::expire(self.cookies), None));
        };

        // Exactly one concurrent rotation wins this. The loser revoked nothing, so it must not
        // mint a replacement — otherwise two rotations of one session produce two live sessions.
        if !self.sessions.revoke(&digest, now).await? {
            return Ok((crate::cookie::expire(self.cookies), None));
        }

        let fresh = Opaque::generate(OpaqueKind::Session, self.entropy.as_ref())?;
        self.sessions
            .create(record.user_id, &SecretDigest::of(&fresh), now)
            .await?;
        Ok((
            crate::cookie::issue(&fresh, self.cookies, self.policy.cookie_max_age()),
            Some(AuthenticatedSubject::new(record.user_id)),
        ))
    }

    /// Ends the session a request presents.
    ///
    /// # A cleared cookie is not a logout
    ///
    /// The expiry cookie is only reachable through the `Ok` arm, and the `Ok` arm is only reached
    /// once the repository has **returned successfully** from revoking. If the revoke fails this
    /// returns [`ServiceError::Storage`] and there is **no `SetCookie` to send**, so a caller
    /// cannot tell a browser it is signed out while a usable row remains on the server. That is a
    /// property of the return type, not of remembering to check.
    ///
    /// Repeated logout is [`LogoutOutcome::AlreadyEnded`] and still succeeds: nothing usable
    /// remains, which is what was asked for.
    ///
    /// # Errors
    ///
    /// [`ServiceError::Storage`].
    pub async fn log_out(
        &self,
        presented: &str,
        now: DateTime<Utc>,
    ) -> Result<(SetCookie, LogoutOutcome), ServiceError> {
        let outcome = match crate::cookie::read(presented) {
            // A cookie that never parsed cannot name a row; there is nothing to revoke and the
            // client's copy is cleared anyway.
            Err(_) => LogoutOutcome::AlreadyEnded,
            Ok(secret) => {
                if self
                    .sessions
                    .revoke(&SecretDigest::of(&secret), now)
                    .await?
                {
                    LogoutOutcome::Ended
                } else {
                    LogoutOutcome::AlreadyEnded
                }
            }
        };
        Ok((crate::cookie::expire(self.cookies), outcome))
    }

    /// Ends **every** live session for a subject, returning how many were revoked.
    ///
    /// The operation behind "sign out everywhere", and the one a password reset or a compromise
    /// report calls. Returns an expiry cookie for the calling client alongside the count.
    ///
    /// # Errors
    ///
    /// [`ServiceError::Storage`].
    pub async fn log_out_everywhere(
        &self,
        subject: AuthenticatedSubject,
        now: DateTime<Utc>,
    ) -> Result<(SetCookie, u64), ServiceError> {
        let revoked = self.sessions.revoke_all_for(subject.user_id(), now).await?;
        Ok((crate::cookie::expire(self.cookies), revoked))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::{
        LogoutOutcome, SessionHandle, SessionOutcome, SessionPolicy, SessionRecord,
        SessionRejection, SessionRepository, SessionService,
    };
    use crate::cookie::{CookiePolicy, CookieRejection, SESSION_COOKIE_NAME};
    use crate::error::AuthError;
    use crate::opaque::SecretDigest;
    use crate::service::ServiceError;
    use crate::subject::{AuthenticatedSubject, UserId};
    use chrono::{DateTime, Duration, TimeZone as _, Utc};
    use renvor_database::DatabaseError;

    // ---- an in-memory repository, with a failure switch --------------------------------------

    #[derive(Clone, Copy, Debug)]
    struct Row {
        user_id: UserId,
        created_at: DateTime<Utc>,
        last_seen_at: DateTime<Utc>,
        revoked: bool,
    }

    #[derive(Default)]
    struct MemoryStore {
        rows: Mutex<Vec<([u8; 32], Row)>>,
        fail: Mutex<bool>,
    }

    impl MemoryStore {
        fn fail_next(&self) {
            *self.fail.lock().expect("test lock") = true;
        }

        fn check(&self) -> Result<(), DatabaseError> {
            let mut flag = self.fail.lock().expect("test lock");
            if *flag {
                *flag = false;
                return Err(renvor_database::DatabaseError::new(
                    renvor_database::DatabaseErrorKind::ConnectFailed,
                ));
            }
            Ok(())
        }

        fn live_rows(&self) -> usize {
            self.rows
                .lock()
                .expect("test lock")
                .iter()
                .filter(|(_, row)| !row.revoked)
                .count()
        }

        fn holds_digest(&self, digest: &SecretDigest) -> bool {
            self.rows
                .lock()
                .expect("test lock")
                .iter()
                .any(|(key, _)| key == digest.as_bytes())
        }

        fn stored_keys(&self) -> Vec<[u8; 32]> {
            self.rows
                .lock()
                .expect("test lock")
                .iter()
                .map(|(key, _)| *key)
                .collect()
        }
    }

    impl SessionRepository for MemoryStore {
        async fn create(
            &self,
            user_id: UserId,
            digest: &SecretDigest,
            now: DateTime<Utc>,
        ) -> Result<(), DatabaseError> {
            self.check()?;
            self.rows.lock().expect("test lock").push((
                *digest.as_bytes(),
                Row {
                    user_id,
                    created_at: now,
                    last_seen_at: now,
                    revoked: false,
                },
            ));
            Ok(())
        }

        async fn touch(
            &self,
            digest: &SecretDigest,
            now: DateTime<Utc>,
            idle_cutoff: DateTime<Utc>,
            absolute_cutoff: DateTime<Utc>,
        ) -> Result<Option<SessionRecord>, DatabaseError> {
            self.check()?;
            let mut rows = self.rows.lock().expect("test lock");
            for (key, row) in rows.iter_mut() {
                if key != digest.as_bytes() {
                    continue;
                }
                // The same predicate the adapters put in a WHERE clause.
                if row.revoked
                    || row.last_seen_at <= idle_cutoff
                    || row.created_at <= absolute_cutoff
                {
                    return Ok(None);
                }
                row.last_seen_at = now;
                return Ok(Some(SessionRecord {
                    user_id: row.user_id,
                    created_at: row.created_at,
                    last_seen_at: row.last_seen_at,
                }));
            }
            Ok(None)
        }

        async fn revoke(
            &self,
            digest: &SecretDigest,
            _now: DateTime<Utc>,
        ) -> Result<bool, DatabaseError> {
            self.check()?;
            let mut rows = self.rows.lock().expect("test lock");
            for (key, row) in rows.iter_mut() {
                if key == digest.as_bytes() && !row.revoked {
                    row.revoked = true;
                    return Ok(true);
                }
            }
            Ok(false)
        }

        async fn revoke_all_for(
            &self,
            user_id: UserId,
            _now: DateTime<Utc>,
        ) -> Result<u64, DatabaseError> {
            self.check()?;
            let mut rows = self.rows.lock().expect("test lock");
            let mut count = 0;
            for (_, row) in rows.iter_mut() {
                if row.user_id == user_id && !row.revoked {
                    row.revoked = true;
                    count += 1;
                }
            }
            Ok(count)
        }

        async fn live_for(
            &self,
            user_id: UserId,
            idle_cutoff: DateTime<Utc>,
            absolute_cutoff: DateTime<Utc>,
        ) -> Result<Vec<SessionHandle>, DatabaseError> {
            self.check()?;
            let rows = self.rows.lock().expect("test lock");
            let mut live: Vec<SessionHandle> = rows
                .iter()
                .filter(|(_, row)| {
                    row.user_id == user_id
                        && !row.revoked
                        && row.last_seen_at > idle_cutoff
                        && row.created_at > absolute_cutoff
                })
                .map(|(key, row)| SessionHandle {
                    digest: SecretDigest::from_bytes(*key),
                    last_seen_at: row.last_seen_at,
                })
                .collect();
            live.sort_by_key(|handle| handle.last_seen_at);
            Ok(live)
        }
    }

    // ---- fixtures ----------------------------------------------------------------------------

    fn entropy() -> std::sync::Arc<dyn renvor_core::observe::entropy::EntropySource + Send + Sync> {
        std::sync::Arc::new(renvor_core::observe::entropy::OsEntropy::new())
    }

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 1, hour, minute, 0)
            .single()
            .expect("a real instant")
    }

    fn subject() -> AuthenticatedSubject {
        AuthenticatedSubject::new(UserId::from_bytes([7; 16]))
    }

    fn service(store: MemoryStore) -> SessionService<MemoryStore> {
        SessionService::new(
            store,
            SessionPolicy::default(),
            CookiePolicy::default(),
            entropy(),
        )
    }

    /// Turns a `Set-Cookie` value into the `Cookie` header a browser would send back.
    fn as_request_header(set_cookie: crate::cookie::SetCookie) -> String {
        set_cookie
            .expose_header_value()
            .split(';')
            .next()
            .expect("a header has a first pair")
            .to_owned()
    }

    fn five() -> core::num::NonZeroU32 {
        core::num::NonZeroU32::new(5).expect("5 is not zero")
    }

    /// Decodes the 32 raw bytes behind a hex identifier, so a comparison against a stored digest
    /// compares BYTES to BYTES. Batch A shipped a digest test that passed against a digest which
    /// returned its input, because it compared hex text against decimal bytes; this is the shape
    /// that does not do that.
    fn raw_bytes(hex: &str) -> Vec<u8> {
        (0..hex.len() / 2)
            .map(|index| u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).expect("valid hex"))
            .collect()
    }

    #[tokio::test]
    async fn the_store_holds_a_digest_and_never_the_identifier() {
        let service = service(MemoryStore::default());
        let (cookie, _) = service
            .begin(subject(), "", at(9, 0))
            .await
            .expect("a session starts");
        let secret = crate::cookie::read(&as_request_header(cookie)).expect("readable");
        assert_eq!(
            secret.expose().len(),
            64,
            "32 bytes of entropy, hex-encoded"
        );

        let stored = service.sessions.stored_keys();
        assert_eq!(stored.len(), 1);
        // Positive control: it IS the digest.
        assert_eq!(
            stored[0],
            *SecretDigest::of(&secret).as_bytes(),
            "the store does not hold the digest of the identifier"
        );
        // And it is NOT the identifier, compared byte-to-byte rather than byte-to-hex.
        assert_ne!(
            stored[0].as_slice(),
            raw_bytes(&secret.expose()).as_slice(),
            "the raw session identifier reached the store"
        );
        assert!(service.sessions.holds_digest(&SecretDigest::of(&secret)));
    }

    // ---- the policy refuses rather than clamps ----------------------------------------------

    #[test]
    fn a_policy_beyond_the_aal2_inactivity_ceiling_is_refused() {
        let over = SessionPolicy::AAL2_INACTIVITY_CEILING + Duration::seconds(1);
        assert_eq!(
            SessionPolicy::new(over, Duration::hours(12), five()).expect_err("refused"),
            AuthError::PolicyMisconfigured
        );
    }

    #[test]
    fn a_policy_at_the_aal2_ceilings_exactly_is_admitted() {
        // The positive control. A refusal test that also refused the boundary would be indis-
        // tinguishable from one that refuses everything.
        SessionPolicy::new(
            SessionPolicy::AAL2_INACTIVITY_CEILING,
            SessionPolicy::AAL2_OVERALL_CEILING,
            five(),
        )
        .expect("the ceilings themselves are admissible");
    }

    #[test]
    fn a_policy_beyond_the_aal2_overall_ceiling_is_refused() {
        let over = SessionPolicy::AAL2_OVERALL_CEILING + Duration::seconds(1);
        assert_eq!(
            SessionPolicy::new(Duration::minutes(30), over, five()).expect_err("refused"),
            AuthError::PolicyMisconfigured
        );
    }

    #[test]
    fn an_inactivity_window_longer_than_the_overall_one_is_refused() {
        assert_eq!(
            SessionPolicy::new(Duration::minutes(50), Duration::minutes(40), five())
                .expect_err("refused"),
            AuthError::PolicyMisconfigured
        );
    }

    #[test]
    fn a_non_positive_window_is_refused() {
        assert_eq!(
            SessionPolicy::new(Duration::zero(), Duration::hours(1), five()).expect_err("refused"),
            AuthError::PolicyMisconfigured
        );
    }

    #[test]
    fn the_cookie_outlives_no_session_row() {
        let policy = SessionPolicy::default();
        assert_eq!(policy.cookie_max_age(), Duration::hours(12));
    }

    // ---- establishing a session --------------------------------------------------------------

    #[tokio::test]
    async fn a_new_session_authenticates_and_stores_only_a_digest() {
        let service = service(MemoryStore::default());
        let (cookie, _) = service
            .begin(subject(), "", at(9, 0))
            .await
            .expect("a session starts");
        let header = as_request_header(cookie);

        // The identifier in the cookie must NOT be what the store holds.
        let raw = header.split_once('=').expect("name=value").1.to_owned();
        for key in service.sessions.stored_keys() {
            assert_ne!(
                crate::cookie::read(&header)
                    .expect("readable")
                    .expose()
                    .as_bytes(),
                key.as_slice(),
                "the store holds the identifier itself, not a digest"
            );
        }
        assert_eq!(raw.len(), 64, "32 bytes of entropy, hex-encoded");

        match service
            .authenticate(&header, at(9, 1))
            .await
            .expect("no storage failure")
        {
            SessionOutcome::Live(who) => assert_eq!(who, subject()),
            other => panic!("expected a live session, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn two_sessions_never_share_an_identifier() {
        let service = service(MemoryStore::default());
        let (first, _) = service.begin(subject(), "", at(9, 0)).await.expect("first");
        let (second, _) = service
            .begin(subject(), "", at(9, 1))
            .await
            .expect("second");
        assert_ne!(
            as_request_header(first),
            as_request_header(second),
            "two logins produced the same session identifier"
        );
    }

    // ---- fixation ----------------------------------------------------------------------------

    #[tokio::test]
    async fn authenticating_while_holding_a_session_retires_it() {
        let service = service(MemoryStore::default());
        let (old, _) = service.begin(subject(), "", at(9, 0)).await.expect("first");
        let old_header = as_request_header(old);

        let (new, established) = service
            .begin(subject(), &old_header, at(9, 5))
            .await
            .expect("second");
        assert!(
            established.replaced_presented,
            "the session presented at login must be revoked"
        );

        // The old identifier is dead; the new one works.
        assert!(matches!(
            service
                .authenticate(&old_header, at(9, 6))
                .await
                .expect("ok"),
            SessionOutcome::Rejected(SessionRejection::NotLive)
        ));
        assert!(matches!(
            service
                .authenticate(&as_request_header(new), at(9, 6))
                .await
                .expect("ok"),
            SessionOutcome::Live(_)
        ));
    }

    #[tokio::test]
    async fn an_attacker_chosen_identifier_never_becomes_the_session() {
        // The fixation attempt itself: the victim arrives carrying a well-formed identifier the
        // attacker generated, and authenticates.
        let planted = format!("{SESSION_COOKIE_NAME}={}", "ab".repeat(32));
        let service = service(MemoryStore::default());
        let (issued, _) = service
            .begin(subject(), &planted, at(9, 0))
            .await
            .expect("a session starts");
        assert_ne!(
            as_request_header(issued),
            planted,
            "the planted identifier became the session"
        );
        assert!(matches!(
            service.authenticate(&planted, at(9, 1)).await.expect("ok"),
            SessionOutcome::Rejected(SessionRejection::NotLive)
        ));
    }

    // ---- expiry ------------------------------------------------------------------------------

    #[tokio::test]
    async fn a_session_idle_past_the_window_is_refused_at_the_boundary() {
        let service = service(MemoryStore::default());
        let (cookie, _) = service.begin(subject(), "", at(9, 0)).await.expect("begin");
        let header = as_request_header(cookie);

        // One second inside the 30-minute window: still live.
        assert!(matches!(
            service.authenticate(&header, at(9, 29)).await.expect("ok"),
            SessionOutcome::Live(_)
        ));
        // Exactly ON the boundary from the refreshed instant: refused. `>` not `>=`.
        assert!(matches!(
            service.authenticate(&header, at(9, 59)).await.expect("ok"),
            SessionOutcome::Rejected(SessionRejection::NotLive)
        ));
    }

    #[tokio::test]
    async fn activity_extends_the_idle_window_but_never_the_absolute_one() {
        let service = service(MemoryStore::default());
        let (cookie, _) = service.begin(subject(), "", at(0, 0)).await.expect("begin");
        let header = as_request_header(cookie);

        // Stay active every 20 minutes for 12 hours. The idle window never lapses...
        for hour in 0..12 {
            for third in [0, 20, 40] {
                assert!(
                    matches!(
                        service
                            .authenticate(&header, at(hour, third))
                            .await
                            .expect("ok"),
                        SessionOutcome::Live(_)
                    ),
                    "unexpectedly idle at {hour}:{third:02}"
                );
            }
        }
        // ...but the 12-hour absolute window still ends it, which is the whole point of having one.
        assert!(matches!(
            service.authenticate(&header, at(12, 1)).await.expect("ok"),
            SessionOutcome::Rejected(SessionRejection::NotLive)
        ));
    }

    // ---- revocation, logout, and what a cleared cookie does not prove -------------------------

    #[tokio::test]
    async fn logging_out_revokes_the_server_row_and_clears_the_client_copy() {
        let store = MemoryStore::default();
        let service = service(store);
        let (cookie, _) = service.begin(subject(), "", at(9, 0)).await.expect("begin");
        let header = as_request_header(cookie);

        let (expiry, outcome) = service.log_out(&header, at(9, 5)).await.expect("logout");
        assert_eq!(outcome, LogoutOutcome::Ended);
        assert!(expiry.attributes().contains("Max-Age=0"));
        // THE ASSERTION THAT MATTERS: the server row, not the cookie.
        assert_eq!(
            service.sessions.live_rows(),
            0,
            "a cleared cookie is not a logout"
        );
        assert!(matches!(
            service.authenticate(&header, at(9, 6)).await.expect("ok"),
            SessionOutcome::Rejected(SessionRejection::NotLive)
        ));
    }

    #[tokio::test]
    async fn a_repeated_logout_succeeds_and_says_it_had_nothing_to_do() {
        let service = service(MemoryStore::default());
        let (cookie, _) = service.begin(subject(), "", at(9, 0)).await.expect("begin");
        let header = as_request_header(cookie);
        assert_eq!(
            service.log_out(&header, at(9, 1)).await.expect("first").1,
            LogoutOutcome::Ended
        );
        assert_eq!(
            service.log_out(&header, at(9, 2)).await.expect("second").1,
            LogoutOutcome::AlreadyEnded
        );
    }

    #[tokio::test]
    async fn logging_out_an_already_expired_session_still_succeeds() {
        let service = service(MemoryStore::default());
        let (cookie, _) = service.begin(subject(), "", at(9, 0)).await.expect("begin");
        let header = as_request_header(cookie);
        // Well past the 12-hour absolute window.
        let (_, outcome) = service.log_out(&header, at(23, 0)).await.expect("logout");
        // The row is revoked either way; there is nothing usable left, which is what was asked.
        assert_eq!(outcome, LogoutOutcome::Ended);
        assert_eq!(service.sessions.live_rows(), 0);
    }

    #[tokio::test]
    async fn a_logout_with_no_cookie_at_all_succeeds() {
        let service = service(MemoryStore::default());
        let (_, outcome) = service.log_out("", at(9, 0)).await.expect("logout");
        assert_eq!(outcome, LogoutOutcome::AlreadyEnded);
    }

    #[tokio::test]
    async fn a_storage_failure_during_logout_yields_no_expiry_cookie() {
        // THE REQUIREMENT: never report a successful logout while a usable row remains. There is
        // no `SetCookie` on this path at all, so a caller cannot tell the browser it is signed out.
        let service = service(MemoryStore::default());
        let (cookie, _) = service.begin(subject(), "", at(9, 0)).await.expect("begin");
        let header = as_request_header(cookie);

        service.sessions.fail_next();
        let failure = service
            .log_out(&header, at(9, 1))
            .await
            .expect_err("the storage failure must surface");
        assert!(matches!(failure, ServiceError::Storage(_)));
        // And the row really is still live — the test would be vacuous otherwise.
        assert_eq!(service.sessions.live_rows(), 1);
        assert!(matches!(
            service.authenticate(&header, at(9, 2)).await.expect("ok"),
            SessionOutcome::Live(_)
        ));
    }

    #[tokio::test]
    async fn a_storage_failure_during_authentication_is_an_error_not_a_sign_out() {
        let service = service(MemoryStore::default());
        let (cookie, _) = service.begin(subject(), "", at(9, 0)).await.expect("begin");
        service.sessions.fail_next();
        assert!(matches!(
            service
                .authenticate(&as_request_header(cookie), at(9, 1))
                .await,
            Err(ServiceError::Storage(_))
        ));
    }

    #[tokio::test]
    async fn signing_out_everywhere_revokes_every_session_for_the_subject() {
        let service = service(MemoryStore::default());
        let mut headers = Vec::new();
        for minute in 0..3 {
            let (cookie, _) = service
                .begin(subject(), "", at(9, minute))
                .await
                .expect("begin");
            headers.push(as_request_header(cookie));
        }
        // A second subject's session is the negative control: it must survive.
        let other = AuthenticatedSubject::new(UserId::from_bytes([9; 16]));
        let (survivor, _) = service.begin(other, "", at(9, 4)).await.expect("begin");

        let (_, revoked) = service
            .log_out_everywhere(subject(), at(9, 5))
            .await
            .expect("everywhere");
        assert_eq!(revoked, 3);
        for header in &headers {
            assert!(matches!(
                service.authenticate(header, at(9, 6)).await.expect("ok"),
                SessionOutcome::Rejected(SessionRejection::NotLive)
            ));
        }
        assert!(matches!(
            service
                .authenticate(&as_request_header(survivor), at(9, 6))
                .await
                .expect("ok"),
            SessionOutcome::Live(_)
        ));
    }

    // ---- rotation ----------------------------------------------------------------------------

    #[tokio::test]
    async fn rotation_retires_the_old_identifier_and_keeps_the_subject() {
        let service = service(MemoryStore::default());
        let (cookie, _) = service.begin(subject(), "", at(9, 0)).await.expect("begin");
        let old = as_request_header(cookie);

        let (fresh, who) = service.rotate(&old, at(9, 1)).await.expect("rotate");
        assert_eq!(who, Some(subject()));
        let new = as_request_header(fresh);
        assert_ne!(new, old, "rotation reissued the same identifier");

        assert!(matches!(
            service.authenticate(&old, at(9, 2)).await.expect("ok"),
            SessionOutcome::Rejected(SessionRejection::NotLive)
        ));
        assert!(matches!(
            service.authenticate(&new, at(9, 2)).await.expect("ok"),
            SessionOutcome::Live(_)
        ));
    }

    #[tokio::test]
    async fn rotating_a_dead_session_mints_nothing_and_clears_the_cookie() {
        let service = service(MemoryStore::default());
        let (cookie, _) = service.begin(subject(), "", at(9, 0)).await.expect("begin");
        let header = as_request_header(cookie);
        service.log_out(&header, at(9, 1)).await.expect("logout");

        let (expiry, who) = service.rotate(&header, at(9, 2)).await.expect("rotate");
        assert_eq!(who, None);
        assert!(expiry.attributes().contains("Max-Age=0"));
        assert_eq!(
            service.sessions.live_rows(),
            0,
            "rotating a dead session created a live one"
        );
    }

    // ---- the concurrency bound ---------------------------------------------------------------

    #[tokio::test]
    async fn the_bound_evicts_the_least_recently_seen_session() {
        let service = service(MemoryStore::default());
        let mut headers = Vec::new();
        for minute in 0..5 {
            let (cookie, established) = service
                .begin(subject(), "", at(9, minute))
                .await
                .expect("begin");
            assert_eq!(established.evicted, 0, "evicted while under the bound");
            headers.push(as_request_header(cookie));
        }
        assert_eq!(service.sessions.live_rows(), 5);

        // Touch the OLDEST so it is no longer the stalest — the eviction order must follow
        // activity, not creation.
        service
            .authenticate(&headers[0], at(9, 10))
            .await
            .expect("ok");

        let (_, established) = service
            .begin(subject(), "", at(9, 11))
            .await
            .expect("the sixth");
        assert_eq!(established.evicted, 1);
        assert_eq!(service.sessions.live_rows(), 5, "the bound held");

        // headers[1] was the stalest and is gone; headers[0] was refreshed and survives.
        assert!(matches!(
            service
                .authenticate(&headers[1], at(9, 12))
                .await
                .expect("ok"),
            SessionOutcome::Rejected(SessionRejection::NotLive)
        ));
        assert!(matches!(
            service
                .authenticate(&headers[0], at(9, 12))
                .await
                .expect("ok"),
            SessionOutcome::Live(_)
        ));
    }

    #[tokio::test]
    async fn the_bound_counts_only_the_subjects_own_sessions() {
        let service = service(MemoryStore::default());
        let other = AuthenticatedSubject::new(UserId::from_bytes([9; 16]));
        for minute in 0..5 {
            service
                .begin(other, "", at(9, minute))
                .await
                .expect("begin");
        }
        let (_, established) = service.begin(subject(), "", at(9, 6)).await.expect("begin");
        assert_eq!(
            established.evicted, 0,
            "another subject's sessions counted against this one's bound"
        );
    }

    // ---- what a rejection may say ------------------------------------------------------------

    #[tokio::test]
    async fn a_malformed_cookie_is_reported_as_a_cookie_problem_carrying_no_value() {
        let service = service(MemoryStore::default());
        let outcome = service
            .authenticate(
                &format!("{SESSION_COOKIE_NAME}={}", "z".repeat(64)),
                at(9, 0),
            )
            .await
            .expect("no storage failure");
        assert_eq!(
            outcome,
            SessionOutcome::Rejected(SessionRejection::Cookie(CookieRejection::Malformed))
        );
        assert!(!format!("{outcome:?}").contains('z'));
    }

    #[test]
    fn the_service_debug_reveals_no_secret() {
        let rendered = format!("{:?}", service(MemoryStore::default()));
        assert!(rendered.contains("SessionService"), "{rendered}");
        assert!(!rendered.to_lowercase().contains("entropy"), "{rendered}");
    }
}
