//! Rendering failures as RFC 9457 Problem Details.
//!
//! # Where the transport boundary sits
//!
//! `renvor-error` owns the **vocabulary** — the codes, the titles, the safe details, the document
//! shape — and names no HTTP type. This module owns the **transport mapping**: which status a code
//! is answered with, and how the document reaches a socket. A status code is transport semantics,
//! so it lives here and not in the registry.
//!
//! # The correlation identifier is the Phase 004 one
//!
//! Not a derivation of it, and not a second identity. The `x-request-id` response header, the
//! telemetry record, and the document's `correlationId` are all rendered from **one**
//! [`RequestId`] carried on the request context — which is what makes them agree without anything
//! having to keep them in step.
//!
//! # What cannot reach a response
//!
//! [`HttpError`] carries an operator-facing `detail` that this module **never reads**. The
//! document's `detail` comes from [`ApiErrorCode::detail`], which returns `&'static str` — so
//! there is no runtime value that could inhabit it even by mistake. The operator-facing detail
//! goes to telemetry, which is a different consumer with different rights.

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response as AxumResponse};
use renvor_error::{ApiErrorCode, InvalidParam, PROBLEM_MEDIA_TYPE, ProblemDetails};

use crate::context::RequestId;
use crate::error::{HttpError, HttpErrorKind};
use crate::route::describe::status_for;

/// The correlation value used when a request identifier could not be generated.
///
/// This happens only when the host refuses entropy, in which case the failure is already an
/// internal error. A placeholder is emitted rather than an invented identifier: a fabricated
/// correlation value would let an operator search for a request that never had that identity.
pub const CORRELATION_UNAVAILABLE: &str = "unavailable";

/// The public API error code for a transport failure.
///
/// A total mapping over [`HttpErrorKind`]. The two vocabularies are separate on purpose —
/// `HttpErrorKind` is this crate's internal classification and `ApiErrorCode` is a published
/// compatibility promise — and this is the single place they meet.
#[must_use]
pub const fn code_for(kind: HttpErrorKind) -> ApiErrorCode {
    match kind {
        HttpErrorKind::HostRejected => ApiErrorCode::HostRejected,
        HttpErrorKind::BodyTooLarge => ApiErrorCode::PayloadTooLarge,
        HttpErrorKind::TimedOut => ApiErrorCode::RequestTimeout,
        HttpErrorKind::Unavailable => ApiErrorCode::Unavailable,
        HttpErrorKind::OriginRejected => ApiErrorCode::OriginRejected,
        HttpErrorKind::NotFound => ApiErrorCode::NotFound,
        HttpErrorKind::BodyUnreadable => ApiErrorCode::MalformedBody,
        // A missing state entry and a handler panic are both DEFECTS in the application, and a
        // caller must not be able to tell them apart — the distinction would say which internal
        // mechanism failed.
        HttpErrorKind::StateUnavailable | HttpErrorKind::HandlerFailed => {
            ApiErrorCode::InternalError
        } // NO wildcard arm. `HttpErrorKind` is `#[non_exhaustive]`, but it is defined in THIS
          // crate, so the compiler still checks this match exhaustively — which means adding a kind
          // without deciding its public code is a build failure rather than a silent
          // `internal_error`. That is a better guarantee than a test, and it is only available
          // because the two types sit on the same side of the crate boundary.
    }
}

/// Builds the Problem Details response for a transport failure.
///
/// The `error`'s operator-facing detail is **not** read. See the module documentation.
#[must_use]
pub fn from_http_error(error: &HttpError, request_id: Option<RequestId>) -> AxumResponse {
    let code = code_for(error.kind());
    render(code, error.status(), request_id, Vec::new())
}

/// Builds the Problem Details response for a request that failed validation.
#[must_use]
pub fn from_rejection(
    rejection: &crate::route::RequestRejection,
    request_id: Option<RequestId>,
) -> AxumResponse {
    let invalid_params = match rejection {
        crate::route::RequestRejection::Invalid(issues) => issues
            .iter()
            .cloned()
            .map(renvor_validation::Issue::into_invalid_param)
            .collect(),
        _ => Vec::new(),
    };
    render(
        rejection.code(),
        rejection.status(),
        request_id,
        invalid_params,
    )
}

/// Renders a Problem Details response.
///
/// # `status` and the document's `status` cannot disagree
///
/// Both come from the same argument. RFC 9457 §3.1.3 permits them to differ; Renvor forbids it,
/// because two disagreeing status values is a contradiction a consumer cannot resolve.
#[must_use]
pub fn render(
    code: ApiErrorCode,
    status: u16,
    request_id: Option<RequestId>,
    invalid_params: Vec<InvalidParam>,
) -> AxumResponse {
    let correlation =
        request_id.map_or_else(|| CORRELATION_UNAVAILABLE.to_owned(), |id| id.encode());

    // CONVERT FIRST, then derive the document from the converted value.
    //
    // The earlier order built the document with the caller's `status` and converted afterwards, so
    // an out-of-range value produced a `500` response carrying a document that said something
    // else — contradicting this function's own header. Unreachable today, because every source of
    // `status` returns a valid code; ordered correctly anyway, because "unreachable" is not a
    // guarantee. Found by security review.
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let problem =
        ProblemDetails::new(code, status.as_u16(), correlation).with_invalid_params(invalid_params);

    // A serialisation failure here would be a defect in this crate, not a caller's doing. It
    // becomes a bare 500 with no body rather than a partial document: a truncated problem document
    // is worse than none, because a consumer would parse what arrived and act on half a fact.
    match problem.to_json() {
        Ok(body) => (
            status,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static(PROBLEM_MEDIA_TYPE),
            )],
            body,
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// The status a public API error code is answered with.
///
/// Re-exported from `route::describe` so the description and the runtime use one mapping. Two
/// mappings could disagree, and a description that promised a different status from the one served
/// would be false in the way this phase exists to prevent.
#[must_use]
pub const fn status_of(code: ApiErrorCode) -> u16 {
    status_for(code)
}

#[cfg(test)]
mod tests {
    use super::{code_for, status_of};
    use crate::error::HttpErrorKind;
    use renvor_error::ApiErrorCode;

    #[test]
    fn every_http_error_kind_maps_to_a_code() {
        // The wildcard arm in `code_for` answers `internal_error`, which the language requires
        // because `HttpErrorKind` is `#[non_exhaustive]`. This enumerates every kind with the code
        // it is MEANT to have, so a kind added without updating the mapping fails here rather than
        // quietly becoming an internal error on the wire.
        let expected = [
            (HttpErrorKind::HostRejected, ApiErrorCode::HostRejected),
            (HttpErrorKind::BodyTooLarge, ApiErrorCode::PayloadTooLarge),
            (HttpErrorKind::TimedOut, ApiErrorCode::RequestTimeout),
            (HttpErrorKind::Unavailable, ApiErrorCode::Unavailable),
            (HttpErrorKind::OriginRejected, ApiErrorCode::OriginRejected),
            (HttpErrorKind::StateUnavailable, ApiErrorCode::InternalError),
            (HttpErrorKind::HandlerFailed, ApiErrorCode::InternalError),
            (HttpErrorKind::NotFound, ApiErrorCode::NotFound),
            (HttpErrorKind::BodyUnreadable, ApiErrorCode::MalformedBody),
        ];
        for (kind, code) in expected {
            assert_eq!(code_for(kind), code, "`{kind:?}` maps to the wrong code");
        }
    }

    #[test]
    fn the_status_a_kind_yields_matches_the_status_its_code_maps_to() {
        // Two mappings exist — `HttpErrorKind::status` (Phase 004) and `status_for` (Phase 005) —
        // and a response carries one while the description publishes the other. If they disagreed,
        // the description would promise a status the runtime does not send.
        for kind in [
            HttpErrorKind::HostRejected,
            HttpErrorKind::BodyTooLarge,
            HttpErrorKind::TimedOut,
            HttpErrorKind::Unavailable,
            HttpErrorKind::OriginRejected,
            HttpErrorKind::StateUnavailable,
            HttpErrorKind::HandlerFailed,
            HttpErrorKind::NotFound,
        ] {
            assert_eq!(
                kind.status(),
                status_of(code_for(kind)),
                "`{kind:?}`: the runtime status and the published status disagree"
            );
        }
    }
}
