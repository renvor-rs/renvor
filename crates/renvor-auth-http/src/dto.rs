//! The wire shapes: what a request body carries in, and what a response body carries out.
//!
//! # Every type here is checked against one question
//!
//! *"If this were written to an access log, a proxy log, or a browser's network panel, what would
//! be in it?"*
//!
//! Requests carry credentials because they must — a password has to travel to be verified. What
//! this module refuses is the **reverse** direction: no response type here contains a password, a
//! session identifier, a verification token, or a reset link. The two secrets that do leave are
//! handed over exactly once and are named to say so.
//!
//! # `Deserialize` is derived; `Serialize` is not, except where a response needs it
//!
//! An inbound type that could also be serialised is one `{:?}`-adjacent mistake away from being
//! echoed back. `LogInRequest` has no `Serialize`, so echoing one is a compile error rather than a
//! review finding — the same argument `renvor-config`'s `Secret<T>` makes.

use serde::{Deserialize, Serialize};

/// `POST /auth/register`.
#[derive(Deserialize)]
pub struct RegisterRequest {
    /// The address to register.
    pub email: String,
    /// The password to set. **No `Serialize`** — see the module header.
    pub password: String,
}

/// `POST /auth/login`.
#[derive(Deserialize)]
pub struct LogInRequest {
    /// The address to authenticate.
    pub email: String,
    /// The password to verify. **No `Serialize`.**
    pub password: String,
}

/// `POST /auth/verification/resend` and `POST /auth/password/forgot`.
///
/// One type for both, because the two flows take the same input and — critically — return the same
/// [`AcknowledgedResponse`]. A second type would be a place for the two to drift apart, and their
/// being indistinguishable is the requirement.
#[derive(Deserialize)]
pub struct AddressRequest {
    /// The address the requester named. It may not exist, and the response does not say.
    pub email: String,
}

/// `POST /auth/verification/confirm`.
#[derive(Deserialize)]
pub struct TokenRequest {
    /// The single-use token, as it was mailed.
    pub token: String,
}

/// `POST /auth/password/reset`.
#[derive(Deserialize)]
pub struct ResetRequest {
    /// The single-use token. **The token decides whose password changes** — there is deliberately
    /// no `email` field, because one would be a way to reset somebody else's password with your
    /// own link.
    pub token: String,
    /// The new password.
    pub password: String,
}

/// `POST /auth/token/refresh`.
#[cfg(feature = "tokens")]
#[derive(Deserialize)]
pub struct RefreshRequest {
    /// The refresh token presented for rotation.
    pub refresh_token: String,
}

/// The response every mailed flow gives, whether or not the address exists.
///
/// # This type having exactly one shape is the enumeration defence
///
/// FR-052 requires a generic response. `renvor-auth` already returns `Acknowledged` for a known
/// address and for an unknown one; this is that value crossing the wire, and it has **no field**
/// that could differ between the two. Not "the same value is chosen" — the same value is the only
/// one expressible.
#[derive(Serialize)]
pub struct AcknowledgedResponse {
    /// Always `true`. Present so the body is an object rather than an empty document, which is
    /// easier for a client to branch on than a zero-length body.
    pub acknowledged: bool,
}

impl AcknowledgedResponse {
    /// The only value this type has.
    #[must_use]
    pub const fn new() -> Self {
        Self { acknowledged: true }
    }
}

impl Default for AcknowledgedResponse {
    fn default() -> Self {
        Self::new()
    }
}

/// `GET /auth/me`.
#[derive(Serialize)]
pub struct CurrentUserResponse {
    /// The account identifier, rendered.
    pub id: String,
    /// The address on file.
    pub email: String,
    /// Whether the address has been verified.
    pub email_verified: bool,
}

/// `POST /auth/register` and `POST /auth/login`.
///
/// Carries **no session identifier**: the session leaves in a `Set-Cookie` header, which is the
/// only place it can be marked `HttpOnly`. Putting it in a body would make it readable by script,
/// which is the property the cookie boundary exists to deny.
#[derive(Serialize)]
pub struct AuthenticatedResponse {
    /// The account identifier, rendered.
    pub id: String,
}

/// `POST /auth/token/refresh`.
///
/// # The two secrets that do leave, named so the call site is visible
///
/// A rotation hands over a new access token and a new refresh token, once. There is no way to
/// return them without returning them — but the fields are named for what they are, and this is
/// the only response type in the crate that carries a credential.
#[cfg(feature = "tokens")]
#[derive(Serialize)]
pub struct RotatedResponse {
    /// The new access token.
    pub access_token: String,
    /// The new refresh token. The previous one is now consumed.
    pub refresh_token: String,
    /// The wire scheme the access token is presented under.
    pub token_type: &'static str,
}
