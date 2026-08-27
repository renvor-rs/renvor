//! Authentication failures, in a shape that cannot carry a credential.
//!
//! # The detail is a closed set, and that is inherited rather than invented
//!
//! Phase 008 landed a correction making the operator-facing `HttpError` detail a **fieldless
//! enum**, because a `String` detail is a place a credential can end up. This crate complies with
//! that decision instead of reopening it: every variant below is fieldless, so there is no
//! `AuthError` value that can hold a password, a token, or a rejected input.
//!
//! `renvor-error` makes the same argument about `InvalidParam`, which "has **no field** a rejected
//! value could occupy". This is that argument applied to authentication.

use thiserror::Error;

/// Why an authentication or authorization operation failed.
///
/// **Every variant is fieldless.** That is the guarantee: there is nowhere to put a secret, so no
/// review is required to establish that none is there.
///
/// The messages are deliberately coarse. `InvalidCredentials` covers "no such account" *and*
/// "wrong password" with one value, because distinguishing them at the type level is how an
/// enumeration oracle gets built by accident three layers up.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
#[non_exhaustive]
pub enum AuthError {
    /// The supplied credentials did not authenticate.
    ///
    /// **Covers an unknown account and a wrong password identically** (FR-012, FR-052). Two
    /// variants here would be an enumeration oracle with a type signature.
    #[error("the supplied credentials did not authenticate")]
    InvalidCredentials,

    /// A session, token, or single-use link is expired, consumed, or revoked.
    ///
    /// One variant for all three, for the same reason: which one it was is information the caller
    /// does not need and an attacker would like.
    #[error("the credential is no longer valid")]
    CredentialNoLongerValid,

    /// The subject is not permitted to perform the operation.
    ///
    /// Carries no resource identity, because FR-060 forbids a policy failure from disclosing
    /// whether the resource exists.
    #[error("the operation is not permitted")]
    NotPermitted,

    /// A bound was exceeded — attempts, resends, or concurrent sessions.
    ///
    /// **Does not say which** (FR-070). Naming the dimension tells an attacker which control they
    /// tripped and therefore which one to work around.
    #[error("too many attempts")]
    TooManyAttempts,

    /// The password was refused by policy — too short, too long, or known compromised.
    ///
    /// Fieldless, so the rejected password cannot travel with the refusal. Which rule was broken is
    /// reported to the *user* through validation, not through this error.
    #[error("the password does not meet policy")]
    PasswordRejected,

    /// Randomness was unavailable, so no credential could be generated.
    ///
    /// **There is no fallback.** `renvor_core::observe::entropy` refuses to substitute a weaker
    /// source and this refuses to proceed without one.
    #[error("secure randomness is unavailable")]
    EntropyUnavailable,
}
