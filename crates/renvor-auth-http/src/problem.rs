//! Turning an authentication refusal into an RFC 9457 Problem Details response.
//!
//! # FR-081's "without leaking" is this file
//!
//! Everything that can go wrong in `renvor-auth` arrives here as a [`ServiceError`], and
//! [`ServiceError`] is already shaped so that it cannot carry a secret: its refusal payload is
//! [`AuthError`], every variant of which is **fieldless**. This module's job is to keep that true
//! across the boundary — to choose a code and a status **from the variant alone**, and to put
//! nothing else in the document.
//!
//! The one thing that does travel is the correlation identifier, and it is opaque by construction.
//!
//! # Two collapses that look like imprecision and are not
//!
//! **`InvalidCredentials` and `CredentialNoLongerValid` share a code.** `renvor-auth` already
//! collapsed "no such account" and "wrong password" into one *value*, on the reasoning that
//! *"distinguishing them at the type level is how an enumeration oracle gets built by accident
//! three layers up"*. This is three layers up. Giving them separate wire codes would rebuild the
//! oracle in the place the type was shaped to prevent.
//!
//! **`NotPermitted` is 403 and never 404.** FR-060 forbids a policy failure from disclosing whether
//! the resource exists, and choosing between 403 and 404 *is* that disclosure. `authorize` takes an
//! `Option<R>` precisely so an absent resource and a refused one leave through the same arm; a
//! status that split them again would undo it at the last possible moment.

use renvor_auth::AuthError;
use renvor_auth::audit::CorrelationId;
use renvor_auth::service::ServiceError;
use renvor_error::{ApiErrorCode, InvalidParam, ProblemDetails};
use renvor_http::route::Response;

/// The media type RFC 9457 assigns to a problem document.
pub const PROBLEM_MEDIA_TYPE: &str = "application/problem+json";

/// The status and code one refusal renders as.
///
/// Separated from [`render`] so the mapping can be asserted directly, over every variant, without
/// building a response for each.
#[must_use]
pub fn classify(error: &ServiceError) -> (ApiErrorCode, u16) {
    match error {
        // AN OUTAGE, NOT AN ANSWER. The driver's own text is discarded here and never reaches the
        // document: `DatabaseError` carries a `kind`, and even that is not rendered.
        ServiceError::Storage(_) => (ApiErrorCode::InternalError, 500),
        ServiceError::Refused(refusal) => classify_refusal(*refusal),
        // `ServiceError` is `#[non_exhaustive]`, so a variant added upstream lands here. It becomes
        // `internal_error` rather than a guess: a code chosen for a variant this crate has never
        // seen would be a confident wrong answer, and 500 is at least an honest one.
        _ => (ApiErrorCode::InternalError, 500),
    }
}

/// The mapping for a domain refusal.
///
/// Exhaustive over the non-`#[non_exhaustive]` part of [`AuthError`]; the catch-all is required by
/// the attribute and is deliberately the **least** informative code rather than the most.
#[must_use]
pub fn classify_refusal(error: AuthError) -> (ApiErrorCode, u16) {
    match error {
        // ONE ANSWER for an unknown account, a wrong password, an unknown token, a consumed token
        // and an expired token. See the module header.
        AuthError::InvalidCredentials | AuthError::CredentialNoLongerValid => {
            (ApiErrorCode::AuthenticationRequired, 401)
        }
        // 403, never 404. See the module header.
        AuthError::NotPermitted => (ApiErrorCode::NotPermitted, 403),
        // Does not say which bound, because the value cannot: it is fieldless.
        AuthError::TooManyAttempts => (ApiErrorCode::TooManyAttempts, 429),
        // A refused input. `invalid_params` names the FIELD and a reason; `InvalidParam` has no
        // field a rejected value could occupy, which is what makes this safe for a password.
        AuthError::PasswordRejected => (ApiErrorCode::ValidationFailed, 400),
        // Configuration and environment failures. Both are the operator's problem and neither is
        // the requester's business.
        AuthError::PolicyMisconfigured | AuthError::EntropyUnavailable => {
            (ApiErrorCode::InternalError, 500)
        }
        // `AuthError` is `#[non_exhaustive]`, so a variant added upstream lands here. It becomes
        // `internal_error` rather than a guess: a code chosen for a variant this crate has never
        // seen would be a confident wrong answer, and 500 is at least an honest one.
        _ => (ApiErrorCode::InternalError, 500),
    }
}

/// Renders a refusal as a Problem Details response.
///
/// # The two status values cannot disagree
///
/// Both the response's status and the document's `status` member come from [`classify`]. RFC 9457
/// §3.1.3 permits them to differ; Renvor forbids it, and `renvor-http` makes the same choice where
/// it renders its own errors.
#[must_use]
pub fn render(
    error: &ServiceError,
    correlation: CorrelationId,
    invalid_params: Vec<InvalidParam>,
) -> Response {
    let (code, status) = classify(error);
    let problem =
        ProblemDetails::new(code, status, correlation.encode()).with_invalid_params(invalid_params);

    // A serialisation failure here would be a defect in this crate, not a caller's doing. It
    // becomes a bare status with no body rather than a partial document: a truncated problem
    // document is worse than none, because a consumer parses what arrived and acts on half a fact.
    // This mirrors `renvor_http::problem::render`, deliberately.
    let Ok(body) = problem.to_json() else {
        return Response::status(status).unwrap_or_else(|_| Response::text(""));
    };
    Response::status(status)
        .unwrap_or_else(|_| Response::text(""))
        .with_header("content-type", PROBLEM_MEDIA_TYPE)
        .unwrap_or_else(|_| Response::text(""))
        .with_body(body)
}
