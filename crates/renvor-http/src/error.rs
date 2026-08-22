//! Transport errors, and the boundary that keeps causes out of responses.
//!
//! # Two audiences, two renderings
//!
//! An error here is read by two parties with opposite needs. The operator needs the cause; the
//! caller must not have it. So [`HttpError`] carries a **detail** that reaches telemetry and a
//! **status** that reaches the wire, and the two are separate fields rather than one string that
//! someone remembers not to send.
//!
//! [`HttpError::public_message`] is what a caller sees, and it is derived from the **kind** alone.
//! It cannot include the detail, because it never receives it.

use core::fmt;

/// What went wrong, at the granularity a caller is allowed to know.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HttpErrorKind {
    /// The `Host` header was absent, malformed, or not configured.
    HostRejected,
    /// The request body exceeded the configured limit.
    BodyTooLarge,
    /// The request exceeded the configured timeout.
    TimedOut,
    /// The application is draining, or is at its concurrency ceiling.
    Unavailable,
    /// The origin is not permitted by the CORS policy.
    OriginRejected,
    /// Required application state was missing or of an incompatible type.
    StateUnavailable,
    /// A handler panicked and was contained.
    HandlerFailed,
}

impl HttpErrorKind {
    /// The HTTP status for this kind.
    #[must_use]
    pub const fn status(self) -> u16 {
        match self {
            Self::HostRejected | Self::OriginRejected => 400,
            Self::BodyTooLarge => crate::limits::STATUS_PAYLOAD_TOO_LARGE,
            Self::TimedOut => crate::limits::STATUS_REQUEST_TIMEOUT,
            Self::Unavailable => crate::limits::STATUS_UNAVAILABLE,
            Self::StateUnavailable | Self::HandlerFailed => 500,
        }
    }

    /// The stable code an operator can match on.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::HostRejected => "host_rejected",
            Self::BodyTooLarge => "body_too_large",
            Self::TimedOut => "request_timeout",
            Self::Unavailable => "unavailable",
            Self::OriginRejected => "origin_rejected",
            Self::StateUnavailable => "state_unavailable",
            Self::HandlerFailed => "handler_failed",
        }
    }
}

/// A transport failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpError {
    kind: HttpErrorKind,
    /// **Never reaches a response.** Operator-facing only.
    detail: String,
}

impl HttpError {
    /// Builds an error whose `detail` is for operators and never for callers.
    #[must_use]
    pub fn new(kind: HttpErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> HttpErrorKind {
        self.kind
    }

    /// The status to send.
    #[must_use]
    pub const fn status(&self) -> u16 {
        self.kind.status()
    }

    /// The operator-facing detail. **Do not put this in a response.**
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// What a caller is told.
    ///
    /// Derived from [`HttpErrorKind`] **alone**. This function does not read `self.detail`, so a
    /// path leak or a configuration value cannot reach a response even if one is in the detail —
    /// which is the point of separating them.
    #[must_use]
    pub const fn public_message(&self) -> &'static str {
        match self.kind {
            HttpErrorKind::HostRejected => "the request host is not served by this application",
            HttpErrorKind::BodyTooLarge => "the request body exceeds the configured limit",
            HttpErrorKind::TimedOut => "the request exceeded the configured timeout",
            HttpErrorKind::Unavailable => "the application is not accepting requests",
            HttpErrorKind::OriginRejected => "the request origin is not permitted",
            HttpErrorKind::StateUnavailable | HttpErrorKind::HandlerFailed => {
                "the request could not be completed"
            }
        }
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.code(), self.detail)
    }
}

impl core::error::Error for HttpError {}

#[cfg(test)]
mod tests {
    use super::{HttpError, HttpErrorKind};

    #[test]
    fn a_public_message_cannot_carry_the_detail() {
        // FR-041. The detail names a filesystem path and a configuration value, both of which a
        // caller must never see.
        let error = HttpError::new(
            HttpErrorKind::StateUnavailable,
            "/srv/app/secrets/db.toml missing key `password`",
        );

        assert!(!error.public_message().contains("/srv"));
        assert!(!error.public_message().contains("password"));

        // POSITIVE CONTROL: the detail IS retained for operators, so the absence above is a
        // boundary rather than the detail never having been recorded.
        assert!(error.detail().contains("/srv/app/secrets/db.toml"));
    }

    #[test]
    fn statuses_match_the_published_contract() {
        assert_eq!(HttpErrorKind::BodyTooLarge.status(), 413);
        assert_eq!(HttpErrorKind::TimedOut.status(), 408);
        assert_eq!(HttpErrorKind::Unavailable.status(), 503);
        assert_eq!(HttpErrorKind::HostRejected.status(), 400);
    }

    #[test]
    fn every_kind_has_a_distinct_stable_code() {
        let kinds = [
            HttpErrorKind::HostRejected,
            HttpErrorKind::BodyTooLarge,
            HttpErrorKind::TimedOut,
            HttpErrorKind::Unavailable,
            HttpErrorKind::OriginRejected,
            HttpErrorKind::StateUnavailable,
            HttpErrorKind::HandlerFailed,
        ];
        let mut codes: Vec<&str> = kinds.iter().map(|kind| kind.code()).collect();
        codes.sort_unstable();
        let count = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), count, "two kinds share a code");
    }
}
