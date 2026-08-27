//! Who is acting, in a shape that cannot be confused with "maybe nobody".

use core::fmt;

/// A user's stable identity.
///
/// Opaque on purpose: an identifier that encodes a signup order or a timestamp is an unreviewed
/// disclosure channel, the same argument `renvor_core::observe::run_id` makes at length.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct UserId(uuid_bytes::UuidBytes);

/// A minimal 16-byte identifier, so this crate does not take a `uuid` dependency for a newtype.
mod uuid_bytes {
    /// Sixteen bytes, rendered as lowercase hex.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct UuidBytes(pub [u8; 16]);

    impl core::fmt::Debug for UuidBytes {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            const DIGITS: &[u8; 16] = b"0123456789abcdef";
            for byte in self.0 {
                f.write_fmt(format_args!(
                    "{}{}",
                    DIGITS[usize::from(byte >> 4)] as char,
                    DIGITS[usize::from(byte & 0x0f)] as char
                ))?;
            }
            Ok(())
        }
    }
}

impl UserId {
    /// Builds an identity from raw bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(uuid_bytes::UuidBytes(bytes))
    }

    /// The raw bytes, for persistence.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

/// A subject whose identity has been established.
///
/// **Constructing one is the act of asserting authentication happened.** The constructor is
/// deliberately not `pub` outside this crate: an `AuthenticatedSubject` that a transport adapter
/// could mint from a header would make FR-061's "a transport cannot bypass a policy" unenforceable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AuthenticatedSubject {
    user_id: UserId,
}

impl AuthenticatedSubject {
    /// Creates the assertion that `user_id` authenticated.
    ///
    /// `pub(crate)` on purpose — see the type documentation.
    ///
    /// # The suppression that used to stand here is gone, and it removed itself
    ///
    /// Until batch D nothing **produced** an authenticated subject, so this carried an
    /// `expect(dead_code)` whose reason read *"batch D's login operation is the first producer;
    /// expect fails when it lands"*.
    ///
    /// `expect` rather than `allow`, deliberately: `expect` **fails once the lint stops firing**.
    /// When `AuthenticationService::log_in` landed, `-D warnings` reported *"this lint expectation
    /// is unfulfilled"* and the annotation had to go. A suppression that cannot outlive its reason
    /// is the argument `008/L-1` makes about lychee exclusions — working, rather than asserted.
    pub(crate) const fn new(user_id: UserId) -> Self {
        Self { user_id }
    }

    /// The authenticated identity.
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }
}

/// Who is making a request.
///
/// **Not an `Option<UserId>`, and that is the requirement** (FR-059). An `Option` invites
/// `unwrap`, `unwrap_or_default`, and `is_some()` checks scattered across call sites; a two-variant
/// enum makes the anonymous case something the compiler asks about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Subject {
    /// Nobody has authenticated.
    Anonymous,
    /// Someone has.
    Authenticated(AuthenticatedSubject),
}

impl Subject {
    /// The authenticated identity, if there is one.
    ///
    /// Returns `Option` deliberately at the *edge* — a caller that wants the identity must still
    /// say what happens when there is none. What FR-059 forbids is *storing* the subject as an
    /// option, not ever producing one.
    #[must_use]
    pub const fn user_id(&self) -> Option<UserId> {
        match self {
            Self::Anonymous => None,
            Self::Authenticated(subject) => Some(subject.user_id()),
        }
    }

    /// Whether anyone authenticated.
    #[must_use]
    pub const fn is_authenticated(&self) -> bool {
        matches!(self, Self::Authenticated(_))
    }
}

#[cfg(test)]
mod tests {
    use super::{AuthenticatedSubject, Subject, UserId};

    const ALICE: [u8; 16] = [1_u8; 16];
    const BOB: [u8; 16] = [2_u8; 16];

    #[test]
    fn an_anonymous_subject_yields_no_identity() {
        // FR-059's point: the anonymous case is a variant the compiler asks about, not a `None`
        // somebody remembered to check.
        let subject = Subject::Anonymous;
        assert_eq!(subject.user_id(), None);
        assert!(!subject.is_authenticated());
    }

    #[test]
    fn an_authenticated_subject_carries_the_identity_that_authenticated() {
        let alice = UserId::from_bytes(ALICE);
        let subject = Subject::Authenticated(AuthenticatedSubject::new(alice));
        assert_eq!(subject.user_id(), Some(alice));
        assert!(subject.is_authenticated());
    }

    #[test]
    fn two_subjects_are_distinct() {
        // POSITIVE CONTROL for the test above: identity is carried, not defaulted. Without this,
        // an implementation that returned a fixed user for everyone would pass.
        let alice = Subject::Authenticated(AuthenticatedSubject::new(UserId::from_bytes(ALICE)));
        let bob = Subject::Authenticated(AuthenticatedSubject::new(UserId::from_bytes(BOB)));
        assert_ne!(alice.user_id(), bob.user_id());
    }

    #[test]
    fn a_user_identity_renders_as_hex_and_round_trips() {
        let alice = UserId::from_bytes(ALICE);
        assert_eq!(alice.to_string(), "01".repeat(16));
        assert_eq!(alice.as_bytes(), &ALICE);
    }
}
