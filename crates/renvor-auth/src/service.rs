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

#[cfg(test)]
mod tests {
    use super::{Authenticated, AuthenticationService, ServiceError};
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

    fn entropy() -> FixedEntropy {
        FixedEntropy::new(vec![0x5A, 0x3C, 0x91, 0x02])
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
}
