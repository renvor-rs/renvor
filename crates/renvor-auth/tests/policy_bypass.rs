//! FR-061: a transport adapter cannot bypass a policy — asserted from outside the crate.
//!
//! # Why this file exists alongside `policy.rs`'s own tests
//!
//! Those tests are *inside* the module, so they can see private fields and therefore cannot
//! demonstrate that anything is inaccessible. An integration test is a **separate crate**: it
//! stands exactly where `renvor-http`, `renvor-sqlx`, or an application's own transport stands, and
//! it can reach only `renvor-auth`'s public surface.
//!
//! # What is proven here, and what is proven by the compiler
//!
//! The **compile-time** half is two `compile_fail` doctests on `renvor_auth::policy::Authorized`:
//! neither `Authorized` nor `AuthenticatedSubject` can be constructed from another crate. A
//! `compile_fail` block that starts compiling is a test failure, which is the direction that
//! matters.
//!
//! This file is the **run-time** half: everything a transport legitimately can do, ending at a
//! refusal.
//!
//! # What is NOT proven here
//!
//! The *granted* path. Reaching it needs an `AuthenticatedSubject`, which this crate cannot build —
//! that is the whole point — so it would have to come from `AuthenticationService::log_in` or
//! `SessionService::authenticate`, each of which needs a repository fake this file does not have.
//! The granted path is exercised by `policy.rs`'s own tests. Stated rather than implied, because a
//! test named for a path it does not take is worse than no test.

use renvor_auth::AuthError;
use renvor_auth::policy::{Authorized, Owned, OwnedBySubject, Policy, authorize};
use renvor_auth::subject::{AuthenticatedSubject, Subject, UserId};

const ALICE: [u8; 16] = [1_u8; 16];
const BOB: [u8; 16] = [2_u8; 16];

/// A resource defined out here, which proves an application's own types work through the public
/// trait rather than only the crate's internal ones.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Document {
    owner: UserId,
}

impl Owned for Document {
    fn owner(&self) -> UserId {
        self.owner
    }
}

/// An application operation. **It cannot be called without a decision**: the only value satisfying
/// its parameter is one `authorize` produced.
fn read(authorized: Authorized<Document>) -> UserId {
    authorized.resource().owner()
}

#[test]
fn a_transport_holding_only_a_request_cannot_reach_the_operation() {
    // The most a transport has before authentication is "somebody sent something". It cannot turn
    // that into `Subject::Authenticated` from here, so the best it can construct is the anonymous
    // case — and that is refused.
    let document = Document {
        owner: UserId::from_bytes(ALICE),
    };
    let refused = authorize(&OwnedBySubject, &Subject::Anonymous, Some(document))
        .expect_err("an anonymous subject must be refused");
    assert_eq!(refused, AuthError::NotPermitted);

    // There is no second route. This line cannot be written in this crate at all:
    //
    //     read(Authorized { subject: .., resource: document, marker: .. });
    //
    // The fields are private. The `compile_fail` doctest on `Authorized` is where that is asserted
    // rather than asserted-in-a-comment.
    let _operation_exists_but_is_unreachable_from_here = read;
}

#[test]
fn a_permissive_policy_still_cannot_admit_an_anonymous_caller() {
    // THE CONTROL. A policy that permits everything is defined out here, so the refusal above
    // cannot be blamed on `OwnedBySubject` being strict: the subject is checked BEFORE the policy
    // is consulted, and no policy gets a say in the anonymous case.
    struct Everything;
    impl Policy<Document> for Everything {
        fn permits(&self, _subject: AuthenticatedSubject, _resource: &Document) -> bool {
            true
        }
    }

    assert_eq!(
        authorize(
            &Everything,
            &Subject::Anonymous,
            Some(Document {
                owner: UserId::from_bytes(BOB),
            })
        ),
        Err(AuthError::NotPermitted),
        "a permissive policy admitted an anonymous caller"
    );
}

#[test]
fn an_absent_resource_is_refused_identically_from_out_here_too() {
    // FR-060 across the crate boundary: a transport that looked up a resource and found nothing
    // gets the same value, and the same rendering, as one that found somebody else's.
    let absent = authorize::<Document, _>(&OwnedBySubject, &Subject::Anonymous, None)
        .expect_err("absent is refused");
    let present = authorize(
        &OwnedBySubject,
        &Subject::Anonymous,
        Some(Document {
            owner: UserId::from_bytes(ALICE),
        }),
    )
    .expect_err("present-but-not-permitted is refused");

    assert_eq!(absent, present);
    assert_eq!(absent.to_string(), present.to_string());
    assert_eq!(format!("{absent:?}"), format!("{present:?}"));
}
