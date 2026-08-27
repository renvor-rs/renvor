//! The authentication operations: registration, login, and current-user.
//!
//! # The policy check lives HERE, not in the transport
//!
//! FR-057 requires authorization and credential policy to be enforced inside the application
//! operation, and FR-061 requires a transport adapter to be structurally unable to bypass it. Both
//! hold because this module is where the decision is made and `renvor-auth` names no transport —
//! there is no HTTP handler that could reach a repository without passing through here.

use core::fmt;

use renvor_core::observe::entropy::EntropySource;
use renvor_database::DatabaseError;

use crate::error::AuthError;
use crate::password::{PasswordBlocklist, PasswordHash, PasswordPolicy, PasswordService};
use crate::repository::{CredentialRepository, Registration, UserRepository};
use crate::subject::{AuthenticatedSubject, UserId};

/// Why an operation could not complete.
///
/// Separates a **domain refusal** from an **infrastructure failure**, because they mean different
/// things to a caller: the first is an answer, the second is an outage.
#[derive(Debug)]
#[non_exhaustive]
pub enum ServiceError {
    /// The operation was refused. Fieldless payload — see [`AuthError`].
    Refused(AuthError),
    /// The database could not be reached or the statement failed.
    Storage(DatabaseError),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(error) => write!(f, "{error}"),
            // The driver's text is NOT rendered. `DatabaseError`'s kind is a closed set; its
            // message could carry a value from a failed statement.
            Self::Storage(_) => f.write_str("a storage operation failed"),
        }
    }
}

impl std::error::Error for ServiceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Refused(error) => Some(error),
            // DELIBERATELY NOT the database error. `Error::source` is walked by every error
            // reporter, and FR-013 forbids credential material or unsafe driver text on that path.
            Self::Storage(_) => None,
        }
    }
}

impl From<AuthError> for ServiceError {
    fn from(error: AuthError) -> Self {
        Self::Refused(error)
    }
}

impl From<DatabaseError> for ServiceError {
    fn from(error: DatabaseError) -> Self {
        Self::Storage(error)
    }
}

/// What a successful authentication produced.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Authenticated {
    /// The subject that authenticated.
    pub subject: AuthenticatedSubject,
    /// Whether the stored password must be changed before the account is used further.
    ///
    /// NIST §3.1.1.2: *"verifiers SHALL force a change if there is evidence that the authenticator
    /// has been compromised."* The flag travels with the success so a caller cannot forget to ask.
    pub must_change_password: bool,
}

/// What the requester may be told. **One variant, deliberately.**
///
/// A verification or forgot-password request must answer identically whether or not the address has
/// an account (FR-052). A type with one variant cannot carry the difference, so a handler that
/// returns this is generic **by construction** rather than by the author remembering to be.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Acknowledged;

/// What the **operator** learns. Never the response body.
///
/// # Why this is a separate value rather than an error
///
/// The obvious design returns `Err` when delivery fails. That is an **account-enumeration oracle**:
/// delivery is only attempted for an address that has an account, so if the mail transport is down,
/// every known address errors and every unknown one succeeds. An attacker who can knock over SMTP —
/// or who simply waits for an outage — reads the user table off the difference.
///
/// So the public answer is [`Acknowledged`] on every path, and this rides beside it for the caller
/// to log or audit. **Putting a `DispatchOutcome` in a response body is a visible mistake**, which
/// is the point of it being a distinct type with this documentation on it.
///
/// It is not silent: a failure is returned to the caller, loudly and in a shape that must be
/// handled — it simply is not returned to the *requester*.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum DispatchOutcome {
    /// No account for that address. Nothing was issued and nothing was sent.
    NoAccount,
    /// A token was issued and the mail was handed to the transport.
    Delivered,
    /// A token was issued and the transport **refused it**. The token remains valid until it
    /// expires; it was never delivered, so nobody holds it.
    DeliveryFailed,
}

/// Registration, login, and current-user.
pub struct AuthenticationService<U, C, B> {
    users: U,
    credentials: C,
    blocklist: B,
    passwords: PasswordService,
    policy: PasswordPolicy,
    dummy: PasswordHash,
    entropy: std::sync::Arc<dyn EntropySource + Send + Sync>,
}

/// How long a mailed token stays valid.
///
/// # The ceiling is normative, not a preference
///
/// NIST SP 800-63B-4 §4.2.1.2: *"Issued recovery codes SHALL be valid for at most … 24 hours —
/// email address."* A `SHALL`, so a configuration above it is refused rather than clamped —
/// clamping would let an operator believe they had configured something they had not.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TokenLifetime(chrono::Duration);

impl TokenLifetime {
    /// The normative ceiling for an email-delivered code: **24 hours**.
    pub const CEILING: chrono::Duration = chrono::Duration::hours(24);

    /// Builds a lifetime.
    ///
    /// # Errors
    ///
    /// [`AuthError::PasswordRejected`] when `lifetime` is not positive or exceeds [`Self::CEILING`].
    /// The variant is reused rather than adding one: this is a configuration refusal, it never
    /// reaches a requester, and a new public error code is a compatibility promise.
    pub fn new(lifetime: chrono::Duration) -> Result<Self, AuthError> {
        if lifetime <= chrono::Duration::zero() || lifetime > Self::CEILING {
            return Err(AuthError::PasswordRejected);
        }
        Ok(Self(lifetime))
    }

    /// The configured duration.
    #[must_use]
    pub const fn get(self) -> chrono::Duration {
        self.0
    }
}

impl Default for TokenLifetime {
    /// One hour — well inside the ceiling, and short enough that a link left in an inbox stops
    /// working the same working day.
    fn default() -> Self {
        Self(chrono::Duration::hours(1))
    }
}

impl<U, C, B> fmt::Debug for AuthenticationService<U, C, B> {
    /// Names the type and nothing else. The dummy hash is not a credential, but printing service
    /// internals is how a `{:?}` on an application struct starts including one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthenticationService")
            .finish_non_exhaustive()
    }
}

impl<U, C, B> AuthenticationService<U, C, B>
where
    U: UserRepository,
    C: CredentialRepository,
    B: PasswordBlocklist,
{
    /// Builds the service, computing the dummy hash once.
    ///
    /// # The dummy hash is real work on a password nobody knows
    ///
    /// FR-012 wants an unknown account to cost what a known one costs. The dummy is an Argon2id
    /// hash of **32 bytes of entropy**, computed here at construction with the same parameters
    /// every real verification uses — so the absent-account path performs the same hashing, and no
    /// caller can supply the input that would match it.
    ///
    /// # Errors
    ///
    /// [`AuthError::EntropyUnavailable`] if the dummy's input cannot be generated. There is no
    /// fallback: a service that could not build its dummy would be an enumeration oracle.
    pub fn new(
        users: U,
        credentials: C,
        blocklist: B,
        passwords: PasswordService,
        policy: PasswordPolicy,
        entropy: std::sync::Arc<dyn EntropySource + Send + Sync>,
    ) -> Result<Self, AuthError> {
        let mut filler = [0_u8; 32];
        entropy
            .fill(&mut filler)
            .map_err(|_| AuthError::EntropyUnavailable)?;
        let unguessable: String = filler.iter().map(|byte| format!("{byte:02x}")).collect();
        let dummy = passwords.hash(&unguessable, entropy.as_ref())?;
        Ok(Self {
            users,
            credentials,
            blocklist,
            passwords,
            policy,
            dummy,
            entropy,
        })
    }

    /// Registers an account.
    ///
    /// # Errors
    ///
    /// [`ServiceError::Refused`] with [`AuthError::PasswordRejected`] when the password fails
    /// policy or is blocklisted, or [`ServiceError::Storage`].
    pub async fn register(
        &self,
        email: &str,
        password: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Registration, ServiceError> {
        // Policy and blocklist BEFORE the database. Hashing a password that policy will refuse
        // spends 64 MiB on an answer already known, from an unauthenticated endpoint.
        self.policy.admit(password)?;
        if self.blocklist.contains(password) {
            return Err(AuthError::PasswordRejected.into());
        }

        let outcome = self.users.register(email, now).await?;
        let Registration::Created(user) = outcome else {
            return Ok(Registration::AlreadyRegistered);
        };

        let hash = self.passwords.hash(password, self.entropy.as_ref())?;
        self.credentials.upsert(user, &hash, false, now).await?;
        Ok(Registration::Created(user))
    }

    /// Authenticates a subject.
    ///
    /// # Errors
    ///
    /// [`ServiceError::Refused`] with [`AuthError::InvalidCredentials`] — **the same value for an
    /// unknown account and a wrong password** — or [`ServiceError::Storage`].
    pub async fn log_in(&self, email: &str, password: &str) -> Result<Authenticated, ServiceError> {
        // ONE PATH. Neither a missing account nor a missing credential shortens the work, because
        // the identity to look up and the hash to verify against are **selected** rather than
        // branched on. An early return here is the enumeration oracle FR-012 exists to close, and
        // it is not subtle: the known path costs a 64 MiB Argon2id hash that the short-circuit skips.
        let user = self.users.find_by_email(email).await?;

        // An unknown account is looked up under a **freshly generated** identity, so the credential
        // round trip happens either way. Freshly generated rather than a fixed sentinel: a constant
        // is a value somebody could eventually hold, and it would then be a real account whose
        // credential an attacker had found a way to fetch.
        let mut absent = [0_u8; 16];
        self.entropy
            .fill(&mut absent)
            .map_err(|_| ServiceError::Refused(AuthError::EntropyUnavailable))?;
        let lookup = user
            .as_ref()
            .map_or_else(|| UserId::from_bytes(absent), |record| record.id);
        let credential = self.credentials.find(lookup).await?;

        // ONE verification, against the stored hash if there is one and the dummy if there is not.
        // `verify_against_stored_or_dummy` returns false whenever the account is absent, whatever
        // the dummy holds — see its documentation for why that matters.
        let stored = credential.as_ref().map(|record| &record.password_hash);
        let verified = self
            .passwords
            .verify_against_stored_or_dummy(password, stored, &self.dummy);

        // Both `None` arms below produce the SAME value as a wrong password.
        let (Some(user), Some(credential)) = (user, credential) else {
            return Err(AuthError::InvalidCredentials.into());
        };
        if !verified {
            return Err(AuthError::InvalidCredentials.into());
        }

        Ok(Authenticated {
            subject: AuthenticatedSubject::new(user.id),
            must_change_password: credential.must_change,
        })
    }

    /// The user behind an authenticated subject.
    ///
    /// # Errors
    ///
    /// [`ServiceError::Refused`] with [`AuthError::CredentialNoLongerValid`] when the account has
    /// gone since the subject was minted, or [`ServiceError::Storage`].
    pub async fn current_user(
        &self,
        subject: AuthenticatedSubject,
    ) -> Result<crate::repository::UserRecord, ServiceError> {
        self.users
            .find_by_id(subject.user_id())
            .await?
            .ok_or_else(|| AuthError::CredentialNoLongerValid.into())
    }

    /// The identity a subject asserts, for callers that need only that.
    #[must_use]
    pub const fn subject_id(subject: AuthenticatedSubject) -> UserId {
        subject.user_id()
    }
}

impl<U, C, B> AuthenticationService<U, C, B>
where
    U: UserRepository,
    C: CredentialRepository,
    B: PasswordBlocklist,
{
    /// Issues a mailed token for `email`, if that address has an account.
    ///
    /// Shared by verification, resend, and forgot-password, because the three differ only in which
    /// table they write and which template they name — and a shared implementation is how the
    /// **generic answer** stays identical across all three rather than being re-derived each time.
    ///
    /// # Earlier tokens are invalidated first
    ///
    /// At most one token per purpose per account is ever live. A resend that left the previous one
    /// valid would multiply the number of working links sitting in inboxes and proxy logs.
    ///
    /// # Errors
    ///
    /// [`ServiceError::Storage`], or [`ServiceError::Refused`] with
    /// [`AuthError::EntropyUnavailable`]. **A delivery failure is not an error** — see
    /// [`DispatchOutcome`].
    async fn issue_and_send<T, M>(
        &self,
        tokens: &T,
        mail: &M,
        kind: crate::opaque::OpaqueKind,
        template: crate::mail::MailKind,
        email: &str,
        lifetime: TokenLifetime,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(Acknowledged, DispatchOutcome), ServiceError>
    where
        T: crate::repository::SingleUseTokenRepository,
        M: crate::mail::MailPort,
    {
        let Some(user) = self.users.find_by_email(email).await? else {
            // No account: nothing issued, nothing sent, and the SAME public answer.
            return Ok((Acknowledged, DispatchOutcome::NoAccount));
        };

        tokens.invalidate_all_for(user.id, now).await?;

        let secret = crate::opaque::Opaque::generate(kind, self.entropy.as_ref())?;
        let digest = crate::opaque::SecretDigest::of(&secret);
        tokens.issue(user.id, &digest, now + lifetime.get()).await?;

        // The DIGEST went to the database; the SECRET goes to the transport, once.
        let message = crate::mail::OutgoingMail::new(template, user.email, secret);
        match mail.deliver(message).await {
            Ok(()) => Ok((Acknowledged, DispatchOutcome::Delivered)),
            // Loud to the caller, silent to the requester. The token stays valid until it expires;
            // it was never delivered, so nobody holds it.
            Err(_) => Ok((Acknowledged, DispatchOutcome::DeliveryFailed)),
        }
    }

    /// Sends an email-verification link.
    ///
    /// # Errors
    ///
    /// See [`Self::issue_and_send`].
    pub async fn send_verification<T, M>(
        &self,
        tokens: &T,
        mail: &M,
        email: &str,
        lifetime: TokenLifetime,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(Acknowledged, DispatchOutcome), ServiceError>
    where
        T: crate::repository::SingleUseTokenRepository,
        M: crate::mail::MailPort,
    {
        self.issue_and_send(
            tokens,
            mail,
            crate::opaque::OpaqueKind::Verification,
            crate::mail::MailKind::Verification,
            email,
            lifetime,
            now,
        )
        .await
    }

    /// Begins a password reset.
    ///
    /// # Errors
    ///
    /// See [`Self::issue_and_send`].
    pub async fn forgot_password<T, M>(
        &self,
        tokens: &T,
        mail: &M,
        email: &str,
        lifetime: TokenLifetime,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(Acknowledged, DispatchOutcome), ServiceError>
    where
        T: crate::repository::SingleUseTokenRepository,
        M: crate::mail::MailPort,
    {
        self.issue_and_send(
            tokens,
            mail,
            crate::opaque::OpaqueKind::PasswordReset,
            crate::mail::MailKind::PasswordReset,
            email,
            lifetime,
            now,
        )
        .await
    }

    /// Confirms an email address using a verification token.
    ///
    /// # Errors
    ///
    /// [`ServiceError::Refused`] with [`AuthError::CredentialNoLongerValid`] when the token is
    /// unknown, already consumed, or expired — **one answer for all three**.
    pub async fn confirm_verification<T>(
        &self,
        tokens: &T,
        presented: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<UserId, ServiceError>
    where
        T: crate::repository::SingleUseTokenRepository,
    {
        let secret =
            crate::opaque::Opaque::from_wire(crate::opaque::OpaqueKind::Verification, presented)
                .ok_or(AuthError::CredentialNoLongerValid)?;
        tokens
            .consume(&crate::opaque::SecretDigest::of(&secret), now)
            .await?
            .ok_or_else(|| AuthError::CredentialNoLongerValid.into())
    }

    /// Completes a password reset.
    ///
    /// # The token decides whose password changes
    ///
    /// **No caller-supplied identity is accepted.** The owner comes back from consuming the token,
    /// and that is the account that changes. A signature taking an email beside the token would be
    /// a way to reset somebody else's password with your own link.
    ///
    /// Consumption happens **before** the new password is admitted, so a replayed token is refused
    /// even when the replay carries a valid password.
    ///
    /// # Errors
    ///
    /// [`AuthError::CredentialNoLongerValid`] for an unknown, consumed, or expired token;
    /// [`AuthError::PasswordRejected`] when the new password fails policy or is blocklisted.
    pub async fn reset_password<T>(
        &self,
        tokens: &T,
        presented: &str,
        new_password: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<UserId, ServiceError>
    where
        T: crate::repository::SingleUseTokenRepository,
    {
        let secret =
            crate::opaque::Opaque::from_wire(crate::opaque::OpaqueKind::PasswordReset, presented)
                .ok_or(AuthError::CredentialNoLongerValid)?;

        // CONSUME FIRST. Admitting the password first would let a replayed token be distinguished
        // from a valid one by which error came back.
        let owner = tokens
            .consume(&crate::opaque::SecretDigest::of(&secret), now)
            .await?
            .ok_or(AuthError::CredentialNoLongerValid)?;

        self.policy.admit(new_password)?;
        if self.blocklist.contains(new_password) {
            return Err(AuthError::PasswordRejected.into());
        }

        let hash = self.passwords.hash(new_password, self.entropy.as_ref())?;
        // `must_change` returns to false: the reset IS the change NIST §3.1.1.2 asks for.
        self.credentials.upsert(owner, &hash, false, now).await?;
        Ok(owner)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Acknowledged, Authenticated, AuthenticationService, DispatchOutcome, ServiceError,
        TokenLifetime,
    };
    use crate::error::AuthError;
    use crate::password::{
        Argon2idParameters, PasswordHash, PasswordPolicy, PasswordService, StaticBlocklist,
    };
    use crate::repository::{
        CredentialRecord, CredentialRepository, Registration, UserRecord, UserRepository,
    };
    use crate::subject::UserId;
    use chrono::{DateTime, TimeZone as _, Utc};
    use renvor_core::observe::entropy::FixedEntropy;
    use renvor_database::DatabaseError;
    use std::sync::Mutex;

    /// An in-memory user store.
    ///
    /// The four-row suite already proves the real repositories against real servers; these tests
    /// are about the **operation's** behaviour, and a fake keeps them deterministic and fast.
    #[derive(Debug, Default)]
    struct Users {
        rows: Mutex<Vec<UserRecord>>,
        /// How many times an email lookup was performed, so a test can assert the path taken.
        lookups: Mutex<usize>,
    }

    impl UserRepository for Users {
        async fn register(
            &self,
            email: &str,
            _now: DateTime<Utc>,
        ) -> Result<Registration, DatabaseError> {
            let mut rows = self.rows.lock().expect("not poisoned");
            if rows.iter().any(|row| row.email == email) {
                return Ok(Registration::AlreadyRegistered);
            }
            let id = UserId::from_bytes([u8::try_from(rows.len()).expect("small") + 1; 16]);
            rows.push(UserRecord {
                id,
                email: email.to_owned(),
                email_verified_at: None,
            });
            Ok(Registration::Created(id))
        }

        async fn find_by_email(&self, email: &str) -> Result<Option<UserRecord>, DatabaseError> {
            *self.lookups.lock().expect("not poisoned") += 1;
            Ok(self
                .rows
                .lock()
                .expect("not poisoned")
                .iter()
                .find(|row| row.email == email)
                .cloned())
        }

        async fn find_by_id(&self, id: UserId) -> Result<Option<UserRecord>, DatabaseError> {
            Ok(self
                .rows
                .lock()
                .expect("not poisoned")
                .iter()
                .find(|row| row.id == id)
                .cloned())
        }
    }

    /// An in-memory credential store that counts verifications reaching it.
    #[derive(Debug, Default)]
    struct Credentials {
        rows: Mutex<Vec<CredentialRecord>>,
        /// How many credential lookups happened, so a test can assert the path taken.
        lookups: Mutex<usize>,
    }

    impl CredentialRepository for Credentials {
        async fn upsert(
            &self,
            user_id: UserId,
            hash: &PasswordHash,
            must_change: bool,
            _now: DateTime<Utc>,
        ) -> Result<(), DatabaseError> {
            let mut rows = self.rows.lock().expect("not poisoned");
            rows.retain(|row| row.user_id != user_id);
            rows.push(CredentialRecord {
                user_id,
                password_hash: hash.clone(),
                must_change,
            });
            Ok(())
        }

        async fn find(&self, user_id: UserId) -> Result<Option<CredentialRecord>, DatabaseError> {
            *self.lookups.lock().expect("not poisoned") += 1;
            Ok(self
                .rows
                .lock()
                .expect("not poisoned")
                .iter()
                .find(|row| row.user_id == user_id)
                .cloned())
        }
    }

    /// An in-memory single-use token store.
    #[derive(Debug, Default)]
    struct Tokens {
        rows: Mutex<
            Vec<(
                UserId,
                crate::opaque::SecretDigest,
                DateTime<Utc>,
                Option<DateTime<Utc>>,
            )>,
        >,
    }

    impl crate::repository::SingleUseTokenRepository for Tokens {
        async fn issue(
            &self,
            user_id: UserId,
            digest: &crate::opaque::SecretDigest,
            expires_at: DateTime<Utc>,
        ) -> Result<(), DatabaseError> {
            self.rows
                .lock()
                .expect("not poisoned")
                .push((user_id, *digest, expires_at, None));
            Ok(())
        }

        async fn invalidate_all_for(
            &self,
            user_id: UserId,
            now: DateTime<Utc>,
        ) -> Result<u64, DatabaseError> {
            let mut rows = self.rows.lock().expect("not poisoned");
            let mut swept = 0_u64;
            for row in rows.iter_mut() {
                if row.0 == user_id && row.3.is_none() {
                    row.3 = Some(now);
                    swept += 1;
                }
            }
            Ok(swept)
        }

        async fn consume(
            &self,
            digest: &crate::opaque::SecretDigest,
            now: DateTime<Utc>,
        ) -> Result<Option<UserId>, DatabaseError> {
            let mut rows = self.rows.lock().expect("not poisoned");
            for row in rows.iter_mut() {
                if row.1.matches(digest) && row.3.is_none() && row.2 > now {
                    row.3 = Some(now);
                    return Ok(Some(row.0));
                }
            }
            Ok(None)
        }
    }

    fn entropy() -> FixedEntropy {
        FixedEntropy::new(vec![0x5A, 0x3C, 0x91, 0x02])
    }

    /// Entropy that yields a **different** value on each call.
    ///
    /// `FixedEntropy` is deliberately fixed — that is what makes opacity checkable without
    /// probability — so it cannot be used where a test needs two issued tokens to differ. This is
    /// still deterministic: the same sequence every run, just not the same value every call.
    #[derive(Debug, Default)]
    struct CountingEntropy {
        calls: std::sync::atomic::AtomicU8,
    }

    impl renvor_core::observe::entropy::EntropySource for CountingEntropy {
        fn fill(
            &self,
            destination: &mut [u8],
        ) -> Result<(), renvor_core::observe::entropy::EntropyUnavailable> {
            let nth = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            for (index, slot) in destination.iter_mut().enumerate() {
                *slot = nth
                    .wrapping_mul(31)
                    .wrapping_add(u8::try_from(index & 0xff).unwrap_or(0));
            }
            Ok(())
        }
    }

    /// A service whose entropy varies per call, for tests that compare two issued tokens.
    fn service_with_varying_entropy() -> AuthenticationService<Users, Credentials, StaticBlocklist>
    {
        AuthenticationService::new(
            Users::default(),
            Credentials::default(),
            StaticBlocklist::new(["correct horse battery staple".to_owned()]),
            PasswordService::new(Argon2idParameters {
                memory_kib: 8,
                iterations: 1,
                lanes: 1,
            }),
            PasswordPolicy::default(),
            std::sync::Arc::new(CountingEntropy::default()),
        )
        .expect("the service builds")
    }

    /// Cheap Argon2id parameters. The shipped defaults are asserted in `password::tests`.
    fn service() -> AuthenticationService<Users, Credentials, StaticBlocklist> {
        AuthenticationService::new(
            Users::default(),
            Credentials::default(),
            StaticBlocklist::new(["correct horse battery staple".to_owned()]),
            PasswordService::new(Argon2idParameters {
                memory_kib: 8,
                iterations: 1,
                lanes: 1,
            }),
            PasswordPolicy::default(),
            std::sync::Arc::new(entropy()),
        )
        .expect("the service builds")
    }

    fn moment() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap()
    }

    fn refusal(error: ServiceError) -> AuthError {
        match error {
            ServiceError::Refused(inner) => inner,
            ServiceError::Storage(_) => panic!("expected a refusal, got a storage failure"),
        }
    }

    #[tokio::test]
    async fn an_unknown_account_and_a_wrong_password_are_indistinguishable() {
        // FR-012 and FR-052. Two different failures must produce ONE observable answer, or the
        // login endpoint tells an attacker which addresses have accounts.
        let service = service();
        service
            .register("ada@example.test", "a long enough passphrase", moment())
            .await
            .expect("registers");

        let unknown = refusal(
            service
                .log_in("nobody@example.test", "a long enough passphrase")
                .await
                .expect_err("an unknown account must not authenticate"),
        );
        let wrong = refusal(
            service
                .log_in("ada@example.test", "the wrong passphrase entirely")
                .await
                .expect_err("a wrong password must not authenticate"),
        );

        assert_eq!(unknown, wrong, "the two failures must be the same value");
        assert_eq!(unknown, AuthError::InvalidCredentials);
        // ...and the rendered form must not differ either — a caller that logs `{error}` would
        // otherwise publish the distinction the values were careful to hide.
        assert_eq!(unknown.to_string(), wrong.to_string());
    }

    #[tokio::test]
    async fn an_unknown_account_still_performs_a_verification() {
        // THE STRUCTURAL HALF OF FR-012, and it is asserted through the CREDENTIAL LOOKUP rather
        // than through timing — a timing-equality test is flaky by construction and this project
        // has already recorded what that costs (finding F-3).
        //
        // The observable consequence of doing the work is that the unknown-account path reaches
        // the same credential-store lookup a known account does. An implementation that returns
        // early on `find_by_email` yielding `None` never gets there.
        let service = service();
        service
            .register("ada@example.test", "a long enough passphrase", moment())
            .await
            .expect("registers");

        let before = *service.credentials.lookups.lock().expect("not poisoned");
        let _ = service
            .log_in("nobody@example.test", "a long enough passphrase")
            .await;
        let after = *service.credentials.lookups.lock().expect("not poisoned");

        assert_eq!(
            after,
            before + 1,
            "an unknown account must still reach the credential lookup, or the two paths differ"
        );
    }

    #[tokio::test]
    async fn a_correct_password_authenticates() {
        // POSITIVE CONTROL. Without it, a service that refused everything would satisfy both tests
        // above while breaking every login.
        let service = service();
        let Registration::Created(id) = service
            .register("ada@example.test", "a long enough passphrase", moment())
            .await
            .expect("registers")
        else {
            panic!("created")
        };

        let Authenticated {
            subject,
            must_change_password,
        } = service
            .log_in("ada@example.test", "a long enough passphrase")
            .await
            .expect("the correct password must authenticate");
        assert_eq!(subject.user_id(), id);
        assert!(!must_change_password);
    }

    #[tokio::test]
    async fn a_password_failing_policy_is_refused_before_the_database_is_touched() {
        // Hashing a password policy will refuse spends 64 MiB answering a question already
        // answered, from an unauthenticated endpoint.
        let service = service();
        let error = refusal(
            service
                .register("ada@example.test", "short", moment())
                .await
                .expect_err("a 5-character password must be refused"),
        );
        assert_eq!(error, AuthError::PasswordRejected);
        assert!(
            service.users.rows.lock().expect("not poisoned").is_empty(),
            "no account may be created for a refused password"
        );
    }

    #[tokio::test]
    async fn a_blocklisted_password_is_refused_and_creates_nothing() {
        // NIST §3.1.1.2's blocklist SHALL, at the operation boundary.
        let service = service();
        let error = refusal(
            service
                .register("ada@example.test", "correct horse battery staple", moment())
                .await
                .expect_err("a blocklisted password must be refused"),
        );
        assert_eq!(error, AuthError::PasswordRejected);
        assert!(service.users.rows.lock().expect("not poisoned").is_empty());
    }

    #[tokio::test]
    async fn a_duplicate_registration_reports_the_same_outcome_without_an_identity() {
        // FR-080 at the operation layer, and FR-052: the second caller learns that the address is
        // taken and NOT which account holds it.
        let service = service();
        let first = service
            .register("ada@example.test", "a long enough passphrase", moment())
            .await
            .expect("registers");
        assert!(matches!(first, Registration::Created(_)));

        let second = service
            .register("ada@example.test", "a different long passphrase", moment())
            .await
            .expect("a duplicate is an outcome, not an error");
        assert_eq!(second, Registration::AlreadyRegistered);

        // ...and the second password did NOT overwrite the first credential.
        assert!(
            service
                .log_in("ada@example.test", "a long enough passphrase")
                .await
                .is_ok(),
            "the original password must still work"
        );
        assert!(
            service
                .log_in("ada@example.test", "a different long passphrase")
                .await
                .is_err(),
            "a duplicate registration must NOT reset the password — that would be account takeover"
        );
    }

    #[tokio::test]
    async fn the_must_change_flag_travels_with_a_successful_login() {
        // NIST §3.1.1.2: "verifiers SHALL force a change if there is evidence that the
        // authenticator has been compromised." The flag rides the success so a caller cannot
        // authenticate and forget to ask.
        let service = service();
        let Registration::Created(id) = service
            .register("ada@example.test", "a long enough passphrase", moment())
            .await
            .expect("registers")
        else {
            panic!("created")
        };
        let hash = service
            .passwords
            .hash("a long enough passphrase", &entropy())
            .expect("hashes");
        service
            .credentials
            .upsert(id, &hash, true, moment())
            .await
            .expect("marks compromised");

        let authenticated = service
            .log_in("ada@example.test", "a long enough passphrase")
            .await
            .expect("authenticates");
        assert!(authenticated.must_change_password);
    }

    #[test]
    fn a_storage_failure_exposes_no_driver_text() {
        // FR-013. `Error::source` is walked by every error reporter, so a database error hanging
        // off it would publish whatever the driver put in its message.
        use std::error::Error as _;
        let error = ServiceError::Storage(DatabaseError::new(
            renvor_database::DatabaseErrorKind::StatementRejected,
        ));
        assert_eq!(error.to_string(), "a storage operation failed");
        assert!(
            error.source().is_none(),
            "a storage failure must not expose the driver error through Error::source"
        );
        // POSITIVE CONTROL: a refusal DOES chain, so the absence above is a decision rather than
        // an unimplemented method.
        let refused = ServiceError::Refused(AuthError::InvalidCredentials);
        assert!(refused.source().is_some());
    }
    // ---------------------------------------------------------------- batch E

    use crate::mail::RecordingMailSink;

    fn day() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap()
    }

    async fn registered() -> (
        AuthenticationService<Users, Credentials, StaticBlocklist>,
        Tokens,
        RecordingMailSink,
    ) {
        let service = service();
        service
            .register("ada@example.test", "a long enough passphrase", moment())
            .await
            .expect("registers");
        (service, Tokens::default(), RecordingMailSink::new())
    }

    #[tokio::test]
    async fn a_forgot_password_request_answers_identically_for_a_known_and_unknown_address() {
        // FR-052. The public answer is `Acknowledged` on both paths, and it is a ONE-VARIANT type,
        // so it cannot carry the difference even by accident.
        let (service, tokens, mail) = registered().await;
        let lifetime = TokenLifetime::default();

        let (known, known_outcome) = service
            .forgot_password(&tokens, &mail, "ada@example.test", lifetime, day())
            .await
            .expect("no fault");
        let (unknown, unknown_outcome) = service
            .forgot_password(&tokens, &mail, "nobody@example.test", lifetime, day())
            .await
            .expect("no fault");

        assert_eq!(known, unknown, "the public answers must be identical");
        assert_eq!(format!("{known:?}"), format!("{unknown:?}"));
        // ...and the operator DOES learn the difference, on the other value.
        assert_eq!(known_outcome, DispatchOutcome::Delivered);
        assert_eq!(unknown_outcome, DispatchOutcome::NoAccount);
    }

    #[tokio::test]
    async fn an_unknown_address_issues_no_token_and_sends_no_mail() {
        let (service, tokens, mail) = registered().await;
        service
            .forgot_password(
                &tokens,
                &mail,
                "nobody@example.test",
                TokenLifetime::default(),
                day(),
            )
            .await
            .expect("no fault");
        assert_eq!(tokens.rows.lock().expect("not poisoned").len(), 0);
        assert_eq!(mail.delivered(), 0);
    }

    #[tokio::test]
    async fn a_delivery_failure_is_reported_to_the_operator_and_not_to_the_requester() {
        // THE ORACLE THIS DESIGN EXISTS TO CLOSE. If a delivery failure were an `Err`, then with
        // the transport down every KNOWN address would error and every unknown one would succeed —
        // and the difference is the user table.
        let (service, tokens, mail) = registered().await;
        mail.fail_next_delivery();

        let (public, outcome) = service
            .forgot_password(
                &tokens,
                &mail,
                "ada@example.test",
                TokenLifetime::default(),
                day(),
            )
            .await
            .expect("a delivery failure must NOT be an error to the requester");

        assert_eq!(public, Acknowledged, "the public answer is unchanged");
        assert_eq!(
            outcome,
            DispatchOutcome::DeliveryFailed,
            "the operator is told, loudly"
        );
        assert_eq!(mail.delivered(), 0, "nothing was sent");
        // The token was still issued, and that is deliberate: nobody holds it, and it expires.
        assert_eq!(tokens.rows.lock().expect("not poisoned").len(), 1);
    }

    #[tokio::test]
    async fn a_resend_invalidates_the_previous_token() {
        // At most one live token per purpose per account. A resend that left the earlier link
        // working would multiply the number of valid secrets sitting in inboxes.
        //
        // Varying entropy, because `FixedEntropy` yields the SAME bytes every call — which is what
        // makes it useful elsewhere and useless here.
        let service = service_with_varying_entropy();
        service
            .register("ada@example.test", "a long enough passphrase", moment())
            .await
            .expect("registers");
        let (tokens, mail) = (Tokens::default(), RecordingMailSink::new());
        let lifetime = TokenLifetime::default();

        service
            .send_verification(&tokens, &mail, "ada@example.test", lifetime, day())
            .await
            .expect("no fault");
        let first = mail.last().expect("sent").token().expose();

        service
            .send_verification(&tokens, &mail, "ada@example.test", lifetime, day())
            .await
            .expect("no fault");
        let second = mail.last().expect("sent").token().expose();
        assert_ne!(first, second, "a resend must issue a NEW token");

        // The FIRST token no longer works.
        assert!(
            service
                .confirm_verification(&tokens, &first, day())
                .await
                .is_err(),
            "the superseded token must be dead"
        );
        // POSITIVE CONTROL: the second one does.
        assert!(
            service
                .confirm_verification(&tokens, &second, day())
                .await
                .is_ok(),
            "the current token must work"
        );
    }

    #[tokio::test]
    async fn a_reset_token_cannot_be_replayed() {
        let (service, tokens, mail) = registered().await;
        service
            .forgot_password(
                &tokens,
                &mail,
                "ada@example.test",
                TokenLifetime::default(),
                day(),
            )
            .await
            .expect("no fault");
        let token = mail.last().expect("sent").token().expose();

        assert!(
            service
                .reset_password(&tokens, &token, "a brand new long passphrase", day())
                .await
                .is_ok()
        );
        // The SAME token, with a perfectly valid password, must fail.
        let replay = refusal(
            service
                .reset_password(&tokens, &token, "another brand new passphrase", day())
                .await
                .expect_err("a consumed token must not work twice"),
        );
        assert_eq!(replay, AuthError::CredentialNoLongerValid);
    }

    #[tokio::test]
    async fn a_reset_changes_only_the_account_the_token_belongs_to() {
        // The signature accepts NO caller-supplied identity: the owner comes back from consuming
        // the token. A reset that took an email beside the token would be a way to change somebody
        // else's password with your own link.
        let (service, tokens, mail) = registered().await;
        let Registration::Created(bob) = service
            .register("bob@example.test", "bob's long enough passphrase", moment())
            .await
            .expect("registers")
        else {
            panic!("created")
        };

        // Ada requests a reset; Ada's token comes back.
        service
            .forgot_password(
                &tokens,
                &mail,
                "ada@example.test",
                TokenLifetime::default(),
                day(),
            )
            .await
            .expect("no fault");
        let ada_token = mail.last().expect("sent").token().expose();

        let changed = service
            .reset_password(&tokens, &ada_token, "ada's replacement passphrase", day())
            .await
            .expect("resets");
        assert_ne!(changed, bob, "Ada's token must never change Bob's account");

        // Bob's password is untouched.
        assert!(
            service
                .log_in("bob@example.test", "bob's long enough passphrase")
                .await
                .is_ok(),
            "Bob's credential must be unaffected"
        );
        // ...and Ada's changed.
        assert!(
            service
                .log_in("ada@example.test", "ada's replacement passphrase")
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn an_expired_token_is_refused_against_the_injected_clock() {
        let (service, tokens, mail) = registered().await;
        let lifetime = TokenLifetime::new(chrono::Duration::hours(1)).expect("inside the ceiling");
        service
            .send_verification(&tokens, &mail, "ada@example.test", lifetime, day())
            .await
            .expect("no fault");
        let token = mail.last().expect("sent").token().expose();

        let later = day() + chrono::Duration::hours(1) + chrono::Duration::seconds(1);
        assert!(
            service
                .confirm_verification(&tokens, &token, later)
                .await
                .is_err(),
            "an expired token must be refused"
        );
        // POSITIVE CONTROL: inside the window the same token works.
        assert!(
            service
                .confirm_verification(&tokens, &token, day())
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn a_verification_token_cannot_be_used_to_reset_a_password() {
        // Purpose binding, from `SecretDigest`'s kind. Two tables and two kinds mean a link from
        // one flow is inert in the other even if it were somehow presented there.
        let (service, tokens, mail) = registered().await;
        service
            .send_verification(
                &tokens,
                &mail,
                "ada@example.test",
                TokenLifetime::default(),
                day(),
            )
            .await
            .expect("no fault");
        let verification = mail.last().expect("sent").token().expose();

        let refused = refusal(
            service
                .reset_password(&tokens, &verification, "a brand new long passphrase", day())
                .await
                .expect_err("a verification token must not reset a password"),
        );
        assert_eq!(refused, AuthError::CredentialNoLongerValid);
    }

    #[tokio::test]
    async fn a_token_lifetime_above_the_nist_ceiling_is_refused() {
        // NIST §4.2.1.2: an email-delivered code SHALL be valid for at most 24 hours. Refused
        // rather than clamped — clamping lets an operator believe they configured something else.
        assert!(TokenLifetime::new(chrono::Duration::hours(25)).is_err());
        assert!(TokenLifetime::new(chrono::Duration::zero()).is_err());
        assert!(TokenLifetime::new(chrono::Duration::hours(-1)).is_err());
        // POSITIVE CONTROL: the ceiling itself is allowed, and the default sits inside it.
        assert!(TokenLifetime::new(TokenLifetime::CEILING).is_ok());
        assert!(TokenLifetime::default().get() < TokenLifetime::CEILING);
    }

    #[tokio::test]
    async fn the_store_holds_a_digest_and_the_raw_token_never_reaches_it() {
        // FR-047/FR-048 at the operation layer: what is persisted must not be presentable.
        let (service, tokens, mail) = registered().await;
        service
            .send_verification(
                &tokens,
                &mail,
                "ada@example.test",
                TokenLifetime::default(),
                day(),
            )
            .await
            .expect("no fault");
        let raw = mail.last().expect("sent").token().expose();

        let rows = tokens.rows.lock().expect("not poisoned");
        let stored = format!("{:?}", rows[0].1);
        assert!(
            !stored.contains(&raw),
            "the raw token reached the store: {stored}"
        );
        // POSITIVE CONTROL: the stored value is a digest OF that token, not of something else.
        let rebuilt =
            crate::opaque::Opaque::from_wire(crate::opaque::OpaqueKind::Verification, &raw)
                .expect("round-trips");
        assert!(
            rows[0]
                .1
                .matches(&crate::opaque::SecretDigest::of(&rebuilt))
        );
    }

    #[tokio::test]
    async fn a_policy_rejected_reset_still_spends_the_token() {
        // A DELIBERATE CHOICE, and the mutation that found this test missing is why it is written
        // down. The token is consumed BEFORE the new password is admitted, so a reset link is
        // single-**attempt**, not merely single-use-on-success.
        //
        // The cost is real and is accepted: a user who mistypes a too-short password loses the link
        // and must request another. The gain is that a stolen link can be exercised exactly once,
        // whatever is sent with it, and there is no path that inspects a caller-supplied password
        // while leaving the token live.
        //
        // `a_reset_refuses_a_blocklisted_new_password...` does NOT cover this: the blocklist check
        // sits after `consume` either way, so reordering the *policy* check left it green.
        // Mutation E-M4 survived on exactly that.
        let (service, tokens, mail) = registered().await;
        service
            .forgot_password(
                &tokens,
                &mail,
                "ada@example.test",
                TokenLifetime::default(),
                day(),
            )
            .await
            .expect("no fault");
        let token = mail.last().expect("sent").token().expose();

        let refused = refusal(
            service
                .reset_password(&tokens, &token, "short", day())
                .await
                .expect_err("a password below the policy minimum must be refused"),
        );
        assert_eq!(refused, AuthError::PasswordRejected);

        // THE POINT: the token is gone, even though the attempt failed on policy.
        let retry = refusal(
            service
                .reset_password(&tokens, &token, "a perfectly fine passphrase", day())
                .await
                .expect_err("the token was spent by the refused attempt"),
        );
        assert_eq!(retry, AuthError::CredentialNoLongerValid);

        // ...and nothing was half-changed: the original password still works.
        assert!(
            service
                .log_in("ada@example.test", "a long enough passphrase")
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn a_reset_refuses_a_blocklisted_new_password_after_consuming_the_token() {
        // The token is spent even though the password was refused, and that is deliberate: a
        // caller that could retry a consumed token with different passwords would have an oracle
        // for the blocklist.
        let (service, tokens, mail) = registered().await;
        service
            .forgot_password(
                &tokens,
                &mail,
                "ada@example.test",
                TokenLifetime::default(),
                day(),
            )
            .await
            .expect("no fault");
        let token = mail.last().expect("sent").token().expose();

        let refused = refusal(
            service
                .reset_password(&tokens, &token, "correct horse battery staple", day())
                .await
                .expect_err("a blocklisted password must be refused"),
        );
        assert_eq!(refused, AuthError::PasswordRejected);
        assert!(
            service
                .reset_password(&tokens, &token, "a perfectly fine passphrase", day())
                .await
                .is_err(),
            "the token was spent by the refused attempt and must not be reusable"
        );
        // ...and the original password still works, so nothing was half-changed.
        assert!(
            service
                .log_in("ada@example.test", "a long enough passphrase")
                .await
                .is_ok()
        );
    }
}
