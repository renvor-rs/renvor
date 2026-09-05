//! The gate an application operation puts in front of itself: scope, then policy, then audit.
//!
//! # What this module closes, and why the three belong together
//!
//! Three requirements are one code path:
//!
//! - **FR-043 / T038** — a scope restriction is enforced **at the operation**, not at a router.
//! - **FR-057** — the policy check lives **inside the application operation**.
//! - **FR-062** — the policy decision is **auditable**, without recording credential material.
//!
//! They are one function because splitting them is how each becomes optional. A scope checked in
//! middleware is a scope some other route forgot; a policy consulted next to an operation is a
//! policy the next operation does not consult; an audit record written by the caller is a record
//! the refusal path skips.
//!
//! # Every refusal is the same value, from one place
//!
//! [`Permit::for_operation`](crate::operation::Permit::for_operation) refuses for four reasons — anonymous subject, absent resource,
//! insufficient scope, policy denial — and returns [`AuthError::NotPermitted`] for all of them.
//! That is FR-060 (*"a policy failure MUST NOT disclose whether the resource exists"*) extended to
//! cover the scope check, because a distinguishable *"you lack the scope"* would tell a caller the
//! resource is there and only the grant is missing.
//!
//! [`crate::policy::authorize`] already collapses three of those into one `else`. This collapses
//! the fourth into the same one.
//!
//! # The audit record is written on both paths
//!
//! Permitted and refused both emit [`AuditAction::PolicyDecision`], with the same fields and the
//! same actor and subject shape. Writing only on refusal would make the *presence* of a record the
//! oracle the refusal itself is not — and writing only on success would make the audit trail an
//! account of what worked.
//!
//! A sink failure is **propagated**, identically on both paths. See
//! [`crate::audit::AuditSink`] for why uniformity there is a security property rather than tidiness.
//!
//! # Scopes are token mode, so this module is behind `tokens`
//!
//! [`ScopeSet`](crate::token::ScopeSet) exists only there. A session-authenticated operation has no scope to check and
//! uses [`crate::policy::authorize`] directly.

use chrono::{DateTime, Utc};

use crate::audit::{
    AuditAction, AuditActor, AuditEvent, AuditOutcome, AuditSink, AuditSubject, CorrelationId,
};
use crate::error::AuthError;
use crate::policy::{Authorized, Policy};
use crate::repository::{UserRecord, UserRepository};
use crate::service::ServiceError;
use crate::subject::{Subject, UserId};
use crate::token::{Scope, ScopeSet};

/// The scope an operation requires before it will run.
///
/// A newtype rather than a bare [`Scope`], so an operation's *requirement* cannot be confused at a
/// call site with the set a caller was *granted*. Passing the two the wrong way round is the
/// mistake this type exists to make a compile error.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct RequiredScope(Scope);

impl RequiredScope {
    /// Names the scope.
    #[must_use]
    pub const fn new(scope: Scope) -> Self {
        Self(scope)
    }

    /// The scope required.
    #[must_use]
    pub const fn get(&self) -> &Scope {
        &self.0
    }
}

/// The gate itself.
///
/// Holds nothing but the audit sink, because everything else about a decision is an argument to
/// it. A guard that held the policy would be a guard per policy.
#[derive(Clone, Copy, Debug)]
pub struct Permit<'a, A> {
    audit: &'a A,
    correlation: CorrelationId,
}

impl<'a, A> Permit<'a, A>
where
    A: AuditSink,
{
    /// Builds a gate bound to one request's correlation identifier.
    #[must_use]
    pub const fn new(audit: &'a A, correlation: CorrelationId) -> Self {
        Self { audit, correlation }
    }

    /// Decides whether one operation may run on one resource, and records the decision.
    ///
    /// The returned [`Authorized`] is the **only** way to reach the resource, and it has no public
    /// constructor — so an operation whose signature takes one cannot be called without a decision
    /// having been made. That is FR-061's structural enforcement, reused rather than restated.
    ///
    /// # Errors
    ///
    /// - [`ServiceError::Refused`] with [`AuthError::NotPermitted`] — **one value for all four
    ///   refusals**: an anonymous subject, an absent resource, a granted set that does not cover
    ///   `required`, and a policy that denies.
    /// - [`ServiceError::Refused`] with [`AuthError::PolicyMisconfigured`] when the audit sink
    ///   refuses the record. Fail-closed: an operation whose decision could not be recorded does
    ///   not run, and it fails the same way whether the decision was to permit or to refuse.
    pub async fn for_operation<R, P>(
        &self,
        policy: &P,
        subject: &Subject,
        granted: &ScopeSet,
        required: &RequiredScope,
        resource: Option<R>,
        now: DateTime<Utc>,
    ) -> Result<Authorized<R>, ServiceError>
    where
        P: Policy<R> + ?Sized,
    {
        // THE SCOPE GATE, at the operation. `granted` came from a verified token's claims — there
        // is no path by which a requester supplies it directly.
        //
        // Checked BEFORE the policy, so a caller without the scope never causes the policy to
        // inspect the resource. The two refusals are still indistinguishable to that caller,
        // because both are the same `AuthError::NotPermitted` value and both drop the resource
        // without mentioning it.
        let decision = if granted.grants(required.get()) {
            crate::policy::authorize(policy, subject, resource)
        } else {
            Err(AuthError::NotPermitted)
        };

        // THE AUDIT RECORD, on BOTH paths, with the same shape. The actor is the subject when
        // there is one; the audit subject is the ACTOR's account and never the resource, because
        // naming the resource would put "it exists" into the trail that the refusal keeps out of
        // the response.
        let outcome = if decision.is_ok() {
            AuditOutcome::Permitted
        } else {
            AuditOutcome::Refused
        };
        let actor = match subject {
            Subject::Anonymous => AuditActor::Anonymous,
            Subject::Authenticated(authenticated) => AuditActor::Account(authenticated.user_id()),
        };
        self.audit
            .record(AuditEvent::new(
                AuditAction::PolicyDecision,
                outcome,
                actor,
                match actor {
                    AuditActor::Anonymous => AuditSubject::Unspecified,
                    AuditActor::Account(user) => AuditSubject::Account(user),
                },
                self.correlation,
                now,
            ))
            .await
            .map_err(|_| ServiceError::Refused(AuthError::PolicyMisconfigured))?;

        decision.map_err(ServiceError::Refused)
    }
}

/// A `UserRecord` is owned by the account it describes.
///
/// This is what makes [`crate::OwnedBySubject`] usable on it, and it is the ownership relation
/// FR-058 asks for — stated once, on the type, rather than re-derived at each call site.
impl crate::policy::Owned for UserRecord {
    fn owner(&self) -> UserId {
        self.id
    }
}

/// The scope [`read_account`] requires.
///
/// A `const fn` rather than a `const`, because [`Scope::new`] validates and validation is not
/// something a `const` initialiser can do here. Built once per call and never from caller input.
///
/// # Panics
///
/// Never. `"account:read"` is a literal that satisfies [`Scope`]'s rules, and a test asserts it.
#[must_use]
pub fn account_read_scope() -> RequiredScope {
    RequiredScope::new(Scope::new("account:read").expect("a literal that satisfies Scope's rules"))
}

/// Reads an account.
///
/// # This is the operation FR-043 and FR-057 are about
///
/// Not a router, not middleware, not a decorator. The scope check and the policy check are inside
/// the function that does the work, and the function cannot produce a `UserRecord` without both
/// having passed — because the only path to the record is through an [`Authorized`] that
/// [`Permit::for_operation`] alone can mint.
///
/// # A missing account, a missing scope, a wrong owner: one answer
///
/// The repository lookup produces `Option<UserRecord>`, and the `None` travels **into** the gate
/// rather than being branched on here. A caller therefore cannot tell an account that does not
/// exist from one they may not read — FR-060 — and neither can the audit trail, which records the
/// actor rather than the target.
///
/// # Errors
///
/// [`ServiceError::Refused`] with [`AuthError::NotPermitted`] for all four refusals, or
/// [`ServiceError::Storage`].
pub async fn read_account<U, P, A>(
    users: &U,
    permit: &Permit<'_, A>,
    policy: &P,
    subject: &Subject,
    granted: &ScopeSet,
    target: UserId,
    now: DateTime<Utc>,
) -> Result<UserRecord, ServiceError>
where
    U: UserRepository,
    P: Policy<UserRecord> + ?Sized,
    A: AuditSink,
{
    // The lookup happens BEFORE the decision, and its absence is not branched on. Returning early
    // on `None` here is exactly the enumeration oracle FR-060 forbids: it would answer faster, or
    // differently, for an account that is not there.
    let record = users.find_by_id(target).await?;

    permit
        .for_operation(policy, subject, granted, &account_read_scope(), record, now)
        .await
        .map(Authorized::into_resource)
}

#[cfg(test)]
mod tests {
    use super::{Permit, RequiredScope, account_read_scope, read_account};
    use crate::audit::{
        AuditAction, AuditActor, AuditOutcome, AuditSubject, CorrelationId, RecordingAuditSink,
    };
    use crate::error::AuthError;
    use crate::policy::{OwnedBySubject, Policy};
    use crate::repository::{Registration, UserRecord, UserRepository};
    use crate::service::ServiceError;
    use crate::subject::{AuthenticatedSubject, Subject, UserId};
    use crate::token::{Scope, ScopeSet};
    use chrono::{DateTime, TimeZone as _, Utc};
    use renvor_database::DatabaseError;

    struct Users {
        present: Option<UserRecord>,
    }

    impl UserRepository for Users {
        async fn register(
            &self,
            _email: &str,
            _now: DateTime<Utc>,
        ) -> Result<Registration, DatabaseError> {
            unreachable!("read_account never registers")
        }

        async fn find_by_email(&self, _email: &str) -> Result<Option<UserRecord>, DatabaseError> {
            unreachable!("read_account looks up by identity")
        }

        async fn find_by_id(&self, _id: UserId) -> Result<Option<UserRecord>, DatabaseError> {
            Ok(self.present.clone())
        }

        async fn mark_email_verified(
            &self,
            _id: UserId,
            _now: DateTime<Utc>,
        ) -> Result<(), DatabaseError> {
            unreachable!("read_account never verifies an address")
        }
    }

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0)
            .single()
            .expect("a real instant")
    }

    fn owner() -> UserId {
        UserId::from_bytes([4; 16])
    }

    fn stranger() -> UserId {
        UserId::from_bytes([9; 16])
    }

    fn record() -> UserRecord {
        UserRecord {
            id: owner(),
            email: "ada@example.test".to_owned(),
            email_verified_at: None,
        }
    }

    fn correlation() -> CorrelationId {
        CorrelationId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8])
    }

    fn scopes(names: &[&str]) -> ScopeSet {
        ScopeSet::new(
            names
                .iter()
                .map(|name| Scope::new(name).expect("a legal scope")),
        )
        .expect("a legal set")
    }

    #[test]
    fn the_required_scope_is_a_literal_this_crate_controls() {
        assert_eq!(account_read_scope().get().as_str(), "account:read");
        // The newtype is what stops a call site passing the granted set where the requirement
        // belongs: they are different types.
        let required = RequiredScope::new(Scope::new("other:thing").expect("legal"));
        assert_ne!(required, account_read_scope());
    }

    #[tokio::test]
    async fn the_owner_with_the_scope_reads_the_account_and_the_decision_is_recorded() {
        let sink = RecordingAuditSink::new();
        let permit = Permit::new(&sink, correlation());
        let read = read_account(
            &Users {
                present: Some(record()),
            },
            &permit,
            &OwnedBySubject,
            &Subject::Authenticated(AuthenticatedSubject::new(owner())),
            &scopes(&["account:read", "account:write"]),
            owner(),
            at(),
        )
        .await
        .expect("the owner may read their own account");
        assert_eq!(read.id, owner());

        // FR-062: the decision is auditable, and it names the ACTOR rather than the resource.
        let event = sink.last().expect("a decision was recorded");
        assert_eq!(event.action(), AuditAction::PolicyDecision);
        assert_eq!(event.outcome(), AuditOutcome::Permitted);
        assert_eq!(event.actor(), AuditActor::Account(owner()));
        assert_eq!(event.subject(), AuditSubject::Account(owner()));
        assert_eq!(event.correlation(), correlation());
        assert_eq!(event.at(), at());
    }

    #[tokio::test]
    async fn all_four_refusals_are_the_same_value_and_the_same_record() {
        // T038 and FR-060 in one assertion. A caller must not be able to tell:
        //   - the account does not exist
        //   - from: they are not authenticated
        //   - from: their token lacks `account:read`
        //   - from: the account exists and belongs to somebody else
        let cases: [(&str, Option<UserRecord>, Subject, ScopeSet, UserId); 4] = [
            (
                "no such account",
                None,
                Subject::Authenticated(AuthenticatedSubject::new(owner())),
                scopes(&["account:read"]),
                owner(),
            ),
            (
                "anonymous",
                Some(record()),
                Subject::Anonymous,
                scopes(&["account:read"]),
                owner(),
            ),
            (
                "scope missing",
                Some(record()),
                Subject::Authenticated(AuthenticatedSubject::new(owner())),
                scopes(&["account:write"]),
                owner(),
            ),
            (
                "wrong owner",
                Some(record()),
                Subject::Authenticated(AuthenticatedSubject::new(stranger())),
                scopes(&["account:read"]),
                owner(),
            ),
        ];

        let mut rendered = Vec::new();
        for (label, present, subject, granted, target) in cases {
            let sink = RecordingAuditSink::new();
            let permit = Permit::new(&sink, correlation());
            let outcome = read_account(
                &Users { present },
                &permit,
                &OwnedBySubject,
                &subject,
                &granted,
                target,
                at(),
            )
            .await;
            let error = outcome
                .err()
                .unwrap_or_else(|| panic!("{label} was permitted"));
            assert!(
                matches!(error, ServiceError::Refused(AuthError::NotPermitted)),
                "{label} produced {error:?}"
            );
            rendered.push(format!("{error} | {error:?}"));

            // The refusal was recorded, and the record does not name the resource either.
            let event = sink.last().expect("a decision was recorded");
            assert_eq!(event.outcome(), AuditOutcome::Refused, "{label}");
            assert_eq!(event.action(), AuditAction::PolicyDecision, "{label}");
        }

        // Every rendering is identical, so neither the value, its Display, nor its Debug
        // distinguishes the four.
        assert_eq!(
            rendered
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            1
        );
        assert!(!rendered[0].contains("ada@example.test"));
        assert!(!rendered[0].contains("scope"));
    }

    #[tokio::test]
    async fn the_scope_check_happens_before_the_policy_sees_the_resource() {
        // Without the scope, the policy must not be consulted at all — otherwise a caller with no
        // grant can still make the policy read a resource, and a policy with a side effect (a
        // lookup, a log line, a cache fill) would leak that the resource is there.
        struct Counting(std::cell::Cell<usize>);
        impl Policy<UserRecord> for Counting {
            fn permits(&self, _subject: AuthenticatedSubject, _resource: &UserRecord) -> bool {
                self.0.set(self.0.get() + 1);
                true
            }
        }

        let policy = Counting(std::cell::Cell::new(0));
        let sink = RecordingAuditSink::new();
        let permit = Permit::new(&sink, correlation());
        let outcome = read_account(
            &Users {
                present: Some(record()),
            },
            &permit,
            &policy,
            &Subject::Authenticated(AuthenticatedSubject::new(owner())),
            &scopes(&["account:write"]),
            owner(),
            at(),
        )
        .await;
        assert!(outcome.is_err());
        assert_eq!(
            policy.0.get(),
            0,
            "the policy inspected the resource anyway"
        );

        // POSITIVE CONTROL: WITH the scope, the policy IS consulted — so the zero above is about
        // the scope gate rather than about a policy that is never called.
        read_account(
            &Users {
                present: Some(record()),
            },
            &permit,
            &policy,
            &Subject::Authenticated(AuthenticatedSubject::new(owner())),
            &scopes(&["account:read"]),
            owner(),
            at(),
        )
        .await
        .expect("with the scope it is permitted");
        assert_eq!(policy.0.get(), 1);
    }

    #[tokio::test]
    async fn a_failing_audit_sink_refuses_the_operation_identically_on_both_paths() {
        // Fail-closed, and NOT an oracle: a sink outage must not turn "permitted" and "refused"
        // into two distinguishable failures, or the sink's health becomes the answer.
        let sink = RecordingAuditSink::failing();
        let permit = Permit::new(&sink, correlation());

        let would_permit = read_account(
            &Users {
                present: Some(record()),
            },
            &permit,
            &OwnedBySubject,
            &Subject::Authenticated(AuthenticatedSubject::new(owner())),
            &scopes(&["account:read"]),
            owner(),
            at(),
        )
        .await;
        let would_refuse = read_account(
            &Users { present: None },
            &permit,
            &OwnedBySubject,
            &Subject::Anonymous,
            &scopes(&[]),
            owner(),
            at(),
        )
        .await;

        assert!(matches!(
            would_permit,
            Err(ServiceError::Refused(AuthError::PolicyMisconfigured))
        ));
        assert_eq!(
            format!("{:?}", would_permit.err()),
            format!("{:?}", would_refuse.err()),
            "a sink outage told the caller which decision had been made"
        );
    }

    #[tokio::test]
    async fn a_record_is_written_on_every_call_so_its_presence_is_not_a_signal() {
        // If only refusals were recorded, an operator reading the trail — or anyone who can see
        // its volume — would learn which requests succeeded. Both paths write exactly one.
        for (present, subject) in [
            (
                Some(record()),
                Subject::Authenticated(AuthenticatedSubject::new(owner())),
            ),
            (None, Subject::Anonymous),
        ] {
            let sink = RecordingAuditSink::new();
            let permit = Permit::new(&sink, correlation());
            let _ = read_account(
                &Users { present },
                &permit,
                &OwnedBySubject,
                &subject,
                &scopes(&["account:read"]),
                owner(),
                at(),
            )
            .await;
            assert_eq!(sink.len(), 1, "exactly one record per decision");
        }
    }
}
