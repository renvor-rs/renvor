//! Authorization: who may do what to which thing (FR-056 … FR-061).
//!
//! # Deny-by-default is a type, not a default value
//!
//! FR-056 says policies are deny-by-default. A `fn allows(..) -> bool` cannot deliver that,
//! whatever it returns when unsure: the failure mode is **forgetting to call it**, and no default
//! value prevents a call that never happens. Middleware makes it worse rather than better — an
//! edge that checked something is not the operation that acted.
//!
//! So the permission is a **value only this module can mint**:
//!
//! ```text
//! authorize(policy, subject, Option<resource>)  ->  Result<Authorized<R>, AuthError>
//!                                                          ^ no public constructor
//! operation(.., authorized: Authorized<Document>)   <- cannot be called without one
//! ```
//!
//! An operation that takes [`Authorized<R>`] **cannot run** unless a policy said yes, because
//! there is no other way to obtain the argument. That is deny-by-default as a compile error rather
//! than as a convention, and it is FR-057 in the same stroke: the value is a parameter of the
//! operation, so the check cannot live only at the edge.
//!
//! It is the argument [`crate::subject::AuthenticatedSubject`] already makes — its constructor is
//! `pub(crate)`, so a transport cannot assert that authentication happened — applied one level up
//! to authorization. Together they are FR-061: a transport adapter can construct **neither** the
//! proof of authentication nor the proof of permission.
//!
//! # The resource travels with the permission
//!
//! [`Authorized`] owns the resource it was granted for. A permission that were merely a marker
//! could be minted for document A and passed to an operation acting on document B — a confused
//! deputy with a type signature. Handing back the pair means separating them is a deliberate act
//! rather than an oversight.
//!
//! # Absence and refusal are the same answer
//!
//! FR-060: *a policy failure MUST NOT disclose whether the resource exists.* That is why
//! [`authorize`] takes an **`Option<R>`** rather than an `&R`. A caller loads the resource, hands
//! over whatever it found, and gets one error for "there is no such thing", "you are not signed
//! in", and "you may not touch it". There is no arm in this function that can answer differently,
//! so the property does not depend on a caller remembering to collapse two errors into one.
//!
//! [`crate::AuthError::NotPermitted`] is fieldless, so the refusal carries no resource identity
//! either.

use core::marker::PhantomData;

use crate::error::AuthError;
use crate::subject::{AuthenticatedSubject, Subject, UserId};

/// A resource that belongs to someone.
///
/// The single trait [`OwnedBySubject`] needs. Kept separate from [`Policy`] so an application can
/// implement ownership once and still write other policies over the same type.
pub trait Owned {
    /// Whose it is.
    fn owner(&self) -> UserId;
}

/// A decision procedure for resources of type `R`.
///
/// # The default denies
///
/// [`Self::permits`] has a default body returning `false`. An `impl Policy<Thing> for MyPolicy {}`
/// with nothing in it therefore compiles and **refuses everything** — the safe direction. A
/// required method would force the author to write something, and what an author writes under
/// pressure to make it compile is `true`.
///
/// # Why `bool` is enough here
///
/// A bare boolean is usually the wrong shape for a security answer, because a caller can ignore
/// it. This one has exactly one caller — [`authorize`] — and the value it produces cannot be
/// constructed any other way. The boolean never leaves this module.
pub trait Policy<R> {
    /// Whether `subject` may act on `resource`.
    ///
    /// Called only by [`authorize`], and only after the subject has been established as
    /// authenticated. An implementation therefore never has to consider the anonymous case.
    fn permits(&self, subject: AuthenticatedSubject, resource: &R) -> bool {
        let _ = (subject, resource);
        false
    }
}

/// A resource, and the proof that a policy permitted this subject to act on it.
///
/// **There is no public constructor.** [`authorize`] is the only producer, in this crate or any
/// other, which is what makes an operation that takes one impossible to reach without a decision.
///
/// # FR-061, proven by the compiler
///
/// A transport adapter lives in another crate. It cannot build this:
///
/// ```compile_fail
/// use renvor_auth::Authorized;
/// // The fields are private, so a struct literal is not available outside the module.
/// let forged: Authorized<u8> = Authorized { subject: todo!(), resource: 7, marker: todo!() };
/// ```
///
/// Nor can it forge the authentication the permission is built on:
///
/// ```compile_fail
/// use renvor_auth::subject::AuthenticatedSubject;
/// use renvor_auth::subject::UserId;
/// // `AuthenticatedSubject::new` is `pub(crate)`.
/// let forged = AuthenticatedSubject::new(UserId::from_bytes([0; 16]));
/// ```
///
/// These are `compile_fail` doctests rather than prose because a claim about what does not compile
/// is only worth what a compiler says about it. They run under `cargo test`, so a change that made
/// either constructor reachable would fail the gate — a `compile_fail` block that starts compiling
/// is a **test failure**, which is the direction that matters here.
///
/// # The control, and why it is not an error code
///
/// A bare `compile_fail` passes when the snippet fails for **any** reason — a typo, a renamed
/// import, a moved module — so it can go on reporting success long after it has stopped testing
/// anything.
///
/// The obvious remedy is `compile_fail,E0451`, pinning "field is private". **It does not work on
/// stable, and that was measured here rather than assumed**: annotating the first block with
/// `E0308` — a type-mismatch code it cannot possibly produce — left both doctests passing on
/// 1.94.0. rustdoc parses the code and ignores it off nightly, so the annotation would have been
/// decoration that read like a guarantee.
///
/// So the reason is pinned by a block that **does** compile:
///
/// ```
/// use renvor_auth::Authorized;
/// use renvor_auth::subject::{Subject, UserId};
///
/// // Naming these types from another crate is fine. The two blocks above fail because they
/// // CONSTRUCT them, not because the paths are wrong.
/// fn operation(_: Authorized<u8>) {}
/// let _anonymous = Subject::Anonymous;
/// let _id = UserId::from_bytes([0_u8; 16]);
/// let _ = operation;
/// ```
///
/// Without it, renaming `Authorized` would turn both `compile_fail` blocks into permanent passes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Authorized<R> {
    subject: AuthenticatedSubject,
    resource: R,
    /// Present so that adding a lifetime or a variance change later is not a breaking change.
    marker: PhantomData<fn() -> R>,
}

impl<R> Authorized<R> {
    /// Who was permitted.
    #[must_use]
    pub const fn subject(&self) -> AuthenticatedSubject {
        self.subject
    }

    /// The resource this permission was granted for.
    #[must_use]
    pub const fn resource(&self) -> &R {
        &self.resource
    }

    /// Takes the resource out.
    ///
    /// Consumes the permission with it: an operation that has unwrapped its authorization cannot
    /// then hand the same proof to a second operation.
    #[must_use]
    pub fn into_resource(self) -> R {
        self.resource
    }
}

/// Decides whether `subject` may act on `resource`, and mints the proof if so.
///
/// `resource` is an `Option` on purpose — see the module documentation. All three refusals are the
/// same value.
///
/// # Errors
///
/// [`AuthError::NotPermitted`] when the subject is anonymous, when the resource is absent, or when
/// the policy refuses. **The caller cannot tell which**, and neither can anything the caller
/// returns: the variant is fieldless.
pub fn authorize<R, P: Policy<R> + ?Sized>(
    policy: &P,
    subject: &Subject,
    resource: Option<R>,
) -> Result<Authorized<R>, AuthError> {
    // ONE `else` for all three refusals. There is no arm here that can answer differently, which
    // is what makes FR-060 a property of this function rather than of every call site.
    let (Subject::Authenticated(subject), Some(resource)) = (subject, resource) else {
        return Err(AuthError::NotPermitted);
    };
    if !policy.permits(*subject, &resource) {
        return Err(AuthError::NotPermitted);
    }
    Ok(Authorized {
        subject: *subject,
        resource,
        marker: PhantomData,
    })
}

/// The ownership policy (FR-058): a subject may act on what belongs to them, and nothing else.
///
/// Deliberately the whole of it. An ownership check with an administrator escape hatch is two
/// policies, and the second one is the one that gets a bug.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct OwnedBySubject;

impl<R: Owned> Policy<R> for OwnedBySubject {
    fn permits(&self, subject: AuthenticatedSubject, resource: &R) -> bool {
        resource.owner() == subject.user_id()
    }
}

#[cfg(test)]
mod tests {
    use super::{Authorized, Owned, OwnedBySubject, Policy, authorize};
    use crate::error::AuthError;
    use crate::subject::{AuthenticatedSubject, Subject, UserId};

    const ALICE: [u8; 16] = [1_u8; 16];
    const BOB: [u8; 16] = [2_u8; 16];

    fn alice() -> UserId {
        UserId::from_bytes(ALICE)
    }

    fn bob() -> UserId {
        UserId::from_bytes(BOB)
    }

    fn signed_in(user: UserId) -> Subject {
        Subject::Authenticated(AuthenticatedSubject::new(user))
    }

    /// A resource with an owner and a value, so a test can assert the RIGHT one came back.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct Document {
        owner: UserId,
        serial: u32,
    }

    impl Owned for Document {
        fn owner(&self) -> UserId {
            self.owner
        }
    }

    fn document(owner: UserId, serial: u32) -> Document {
        Document { owner, serial }
    }

    /// A policy that permits nothing, by writing nothing.
    struct Empty;
    impl Policy<Document> for Empty {}

    /// A policy that permits everything, as the control for the one above.
    struct Everything;
    impl Policy<Document> for Everything {
        fn permits(&self, _subject: AuthenticatedSubject, _resource: &Document) -> bool {
            true
        }
    }

    #[test]
    fn an_owner_is_permitted_and_receives_the_resource_they_asked_for() {
        let mine = document(alice(), 7);
        let granted = authorize(&OwnedBySubject, &signed_in(alice()), Some(mine))
            .expect("an owner may act on their own resource");
        assert_eq!(granted.subject().user_id(), alice());
        assert_eq!(
            granted.resource().serial,
            7,
            "a different resource came back"
        );
        assert_eq!(granted.into_resource(), mine);
    }

    #[test]
    fn a_non_owner_is_refused() {
        // FR-058's actual content. Without this, an ownership policy that ignored the owner would
        // pass the test above.
        let theirs = document(bob(), 7);
        assert_eq!(
            authorize(&OwnedBySubject, &signed_in(alice()), Some(theirs)),
            Err(AuthError::NotPermitted)
        );
    }

    #[test]
    fn an_anonymous_subject_is_refused_without_the_policy_being_consulted() {
        // `Everything` permits every authenticated subject. If the anonymous case reached it, this
        // would be granted — so the assertion is about the ORDER of the check, not just its result.
        let anyones = document(alice(), 7);
        assert_eq!(
            authorize(&Everything, &Subject::Anonymous, Some(anyones)),
            Err(AuthError::NotPermitted)
        );
    }

    #[test]
    fn a_policy_that_implements_nothing_permits_nothing() {
        // FR-056 at the trait level: `impl Policy<Document> for Empty {}` compiles, and denies.
        let mine = document(alice(), 7);
        assert_eq!(
            authorize(&Empty, &signed_in(alice()), Some(mine)),
            Err(AuthError::NotPermitted)
        );
        // THE CONTROL. Without it, a broken `authorize` that refused everything would pass above.
        assert!(authorize(&Everything, &signed_in(alice()), Some(mine)).is_ok());
    }

    #[test]
    fn an_absent_resource_and_a_refused_one_are_indistinguishable() {
        // FR-060. Three refusals, one value — and the error is fieldless, so nothing built from it
        // can carry the difference either.
        let absent = authorize::<Document, _>(&OwnedBySubject, &signed_in(alice()), None)
            .expect_err("an absent resource is refused");
        let refused = authorize(
            &OwnedBySubject,
            &signed_in(alice()),
            Some(document(bob(), 7)),
        )
        .expect_err("someone else's resource is refused");
        let anonymous = authorize(
            &OwnedBySubject,
            &Subject::Anonymous,
            Some(document(alice(), 7)),
        )
        .expect_err("an anonymous subject is refused");

        assert_eq!(absent, refused);
        assert_eq!(refused, anonymous);
        // The RENDERING too: an operator's log line must not distinguish them either.
        assert_eq!(absent.to_string(), refused.to_string());
        assert_eq!(refused.to_string(), anonymous.to_string());
        assert_eq!(format!("{absent:?}"), format!("{refused:?}"));
    }

    #[test]
    fn a_permission_carries_the_resource_it_was_granted_for() {
        // The confused-deputy shape: two documents the same subject owns. The permission for one
        // cannot be used to reach the other, because it OWNS the one it was granted for.
        let first = authorize(
            &OwnedBySubject,
            &signed_in(alice()),
            Some(document(alice(), 1)),
        )
        .expect("granted");
        let second = authorize(
            &OwnedBySubject,
            &signed_in(alice()),
            Some(document(alice(), 2)),
        )
        .expect("granted");
        assert_eq!(first.resource().serial, 1);
        assert_eq!(second.resource().serial, 2);
        assert_ne!(first, second);
    }

    /// FR-061, as far as a same-crate test can reach.
    ///
    /// # What this proves, and what only the compiler can
    ///
    /// The real guarantee is that `Authorized` has **no public constructor** and its fields are
    /// private, so no code outside this module can build one — including every transport adapter,
    /// which is in a different crate entirely. A test inside this module cannot demonstrate that:
    /// it is *in* the module, so it can see the fields.
    ///
    /// What it can do is pin the surface a bypass would need. The compile-fail direction is
    /// covered by `crates/renvor-auth/tests/policy_bypass.rs`, which lives outside this module and
    /// therefore stands where a transport stands.
    #[test]
    fn the_only_route_to_a_permission_is_a_decision() {
        // Every public associated function on `Authorized` CONSUMES or BORROWS one. None returns
        // one. If that ever stops being true this test is the place it is noticed.
        let granted: Authorized<Document> = authorize(
            &OwnedBySubject,
            &signed_in(alice()),
            Some(document(alice(), 1)),
        )
        .expect("granted");
        let _subject = granted.subject();
        let _borrowed = granted.resource();
        let _taken = granted.into_resource();
    }
}
