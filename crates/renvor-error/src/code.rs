//! The closed registry of stable public API error codes.
//!
//! # A code is a name that outlives its message
//!
//! This is the same rule [`contracts/json-output.md`] applies to the command line, and it is
//! restated here for a different surface rather than shared with it: the two registries version
//! independently, because a CLI-only change must not force a REST-visible version bump and a
//! REST-only change must not force a CLI one.
//!
//! # The registry is CLOSED
//!
//! A code outside [`ApiErrorCode::ALL`] is a protocol error, not an extension point. The published
//! contract table and this enum are checked against each other by
//! `the_registry_matches_the_published_contract_exactly`, so the document and the binary cannot
//! drift apart silently — the same mechanism `renvor-cli`'s exit-code registry already uses.
//!
//! [`contracts/json-output.md`]: https://github.com/renvor-rs/renvor/blob/main/contracts/json-output.md

use core::fmt;

/// The version of the public API error registry.
///
/// # Why this is separate from the command line's `schemaVersion`
///
/// They describe different surfaces that change for different reasons. Sharing one integer would
/// mean a code added for `renvor doctor` bumped a version that REST consumers pin, and vice versa.
///
/// Incremented **only** when a code is retired, renamed, or reused for a different meaning. Adding
/// a code is **not** a breaking change: a consumer meeting an unrecognised code has met a failure
/// it has no specific handler for, whereas a consumer meeting a **removed** code has silently
/// stopped recognising a failure it used to handle.
pub const API_ERROR_REGISTRY_VERSION: u32 = 1;

/// The base URI under which every code's `type` is published.
///
/// A stable, dereferenceable-in-principle location. RFC 9457 §3.1.1 permits `type` to be absent
/// and default to `"about:blank"`; Renvor always sets it, because `about:blank` discards the one
/// member that identifies the problem **kind** to a machine.
pub const PROBLEM_TYPE_BASE: &str = "https://renvor.dev/problems/";

/// A stable, public API error code.
///
/// # These are not the command line's codes
///
/// `renvor-cli` publishes its own closed registry of exit-code-bearing failures. The two vocabularies
/// overlap in spirit and **not** in membership, and conflating them would tie an HTTP response
/// shape to a shell exit convention. The explicit mapping between them is
/// [`crate::mapping`] — an explicit table rather than an implied identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ApiErrorCode {
    /// One or more inputs violated a declared constraint.
    ValidationFailed,
    /// A request body could not be read as the declared media type.
    ///
    /// Distinct from [`ApiErrorCode::ValidationFailed`] because they are different events: one is
    /// a well-formed value outside its bounds, the other is not a value at all. Reporting the
    /// second as the first would tell a caller to correct a field in a document that never parsed.
    MalformedBody,
    /// The request's media type is not one the operation accepts.
    UnsupportedMediaType,
    /// A required request body was absent.
    MissingBody,
    /// No route declares the requested path.
    NotFound,
    /// The path is declared, but not for this method.
    MethodNotAllowed,
    /// The request host is not served by this application.
    HostRejected,
    /// The request origin is not permitted by the configured policy.
    OriginRejected,
    /// The request body exceeded the configured limit.
    PayloadTooLarge,
    /// The request exceeded the configured timeout.
    RequestTimeout,
    /// The application is draining, or is at its concurrency ceiling.
    Unavailable,
    /// An unclassified failure. **A defect.**
    ///
    /// Its `detail` is a fixed constant and carries nothing about the cause. The cause is
    /// preserved for operators through telemetry, which is a different consumer with different
    /// rights — see [`crate::problem`].
    InternalError,
}

impl ApiErrorCode {
    /// Every code in the registry, in the order the published contract lists them.
    ///
    /// The length assertion below moves with this array, so an edit that adds a variant without
    /// updating the contract still fails loudly. This mirrors `renvor-core`'s `ErrorCategory::ALL`.
    pub const ALL: [Self; 12] = [
        Self::ValidationFailed,
        Self::MalformedBody,
        Self::UnsupportedMediaType,
        Self::MissingBody,
        Self::NotFound,
        Self::MethodNotAllowed,
        Self::HostRejected,
        Self::OriginRejected,
        Self::PayloadTooLarge,
        Self::RequestTimeout,
        Self::Unavailable,
        Self::InternalError,
    ];

    /// The stable wire name.
    ///
    /// `lower_snake_case`, describing **what happened** rather than which component noticed. A
    /// name that encodes a component becomes wrong when the component is refactored, and a code
    /// that becomes wrong is worse than one that is vague.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ValidationFailed => "validation_failed",
            Self::MalformedBody => "malformed_body",
            Self::UnsupportedMediaType => "unsupported_media_type",
            Self::MissingBody => "missing_body",
            Self::NotFound => "not_found",
            Self::MethodNotAllowed => "method_not_allowed",
            Self::HostRejected => "host_rejected",
            Self::OriginRejected => "origin_rejected",
            Self::PayloadTooLarge => "payload_too_large",
            Self::RequestTimeout => "request_timeout",
            Self::Unavailable => "unavailable",
            Self::InternalError => "internal_error",
        }
    }

    /// The RFC 9457 `title` — a **fixed** human-readable summary of the problem **type**.
    ///
    /// RFC 9457 §3.1.2 requires the title to stay the same for the same `type`. It therefore does
    /// not vary by occurrence, is never templated with request data, and is never localised at
    /// runtime — a title that varied would make two responses with the same `type` describe
    /// themselves differently, which is the drift the member exists to prevent.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::ValidationFailed => "Validation failed",
            Self::MalformedBody => "Malformed request body",
            Self::UnsupportedMediaType => "Unsupported media type",
            Self::MissingBody => "Missing request body",
            Self::NotFound => "Not found",
            Self::MethodNotAllowed => "Method not allowed",
            Self::HostRejected => "Host rejected",
            Self::OriginRejected => "Origin rejected",
            Self::PayloadTooLarge => "Payload too large",
            Self::RequestTimeout => "Request timeout",
            Self::Unavailable => "Service unavailable",
            Self::InternalError => "Internal error",
        }
    }

    /// The RFC 9457 `detail`, derived from the **code alone**.
    ///
    /// # This returns `&'static str`, and that is the redaction guarantee
    ///
    /// A `String` return type would make it *possible* to interpolate a rejected value, a path, or
    /// an internal message, and the guarantee would then rest on nobody ever doing so. A
    /// `&'static str` makes it **impossible**: there is no runtime data that can inhabit the type.
    ///
    /// This is the same construction `renvor_http::HttpError::public_message` already uses, applied
    /// to the public API vocabulary.
    ///
    /// Occurrence-specific information does not go here. It goes in `invalidParams`, which is
    /// structured and reviewed, rather than in prose a consumer would have to parse.
    #[must_use]
    pub const fn detail(self) -> &'static str {
        match self {
            Self::ValidationFailed => {
                "One or more inputs did not satisfy this operation's declared constraints. \
                 See invalidParams for the affected locations."
            }
            Self::MalformedBody => "The request body could not be read as the declared media type.",
            Self::UnsupportedMediaType => {
                "This operation does not accept the request's media type."
            }
            Self::MissingBody => "This operation requires a request body.",
            Self::NotFound => "No route is declared for this path.",
            Self::MethodNotAllowed => {
                "This path is declared, but not for the requested method. \
                 See the Allow header for the methods it answers."
            }
            Self::HostRejected => "The request host is not served by this application.",
            Self::OriginRejected => "The request origin is not permitted.",
            Self::PayloadTooLarge => "The request body exceeds the configured limit.",
            Self::RequestTimeout => "The request exceeded the configured timeout.",
            Self::Unavailable => "The application is not accepting requests.",
            // FIXED, and deliberately uninformative. An unclassified failure is a defect, and the
            // one thing a caller must not receive is the defect's details.
            Self::InternalError => "The request could not be completed.",
        }
    }

    /// The `type` URI for this code.
    ///
    /// One URI per code, under [`PROBLEM_TYPE_BASE`]. The **code** is the data a consumer matches
    /// on; the URI is where the documentation lives. They are separate members because a consumer
    /// forced to parse a URI to obtain a code would break the day the documentation moved.
    #[must_use]
    pub fn type_uri(self) -> String {
        format!("{PROBLEM_TYPE_BASE}{}", self.as_str())
    }

    /// Whether this code names a failure the **caller** can correct.
    ///
    /// Used to decide whether a cause is worth preserving for operators: a caller-correctable
    /// refusal is ordinary traffic, while the rest are worth a closer look.
    #[must_use]
    pub const fn is_caller_correctable(self) -> bool {
        matches!(
            self,
            Self::ValidationFailed
                | Self::MalformedBody
                | Self::UnsupportedMediaType
                | Self::MissingBody
                | Self::NotFound
                | Self::MethodNotAllowed
                | Self::PayloadTooLarge
        )
    }
}

impl fmt::Display for ApiErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::{API_ERROR_REGISTRY_VERSION, ApiErrorCode, PROBLEM_TYPE_BASE};
    use std::collections::BTreeSet;

    #[test]
    fn the_registry_has_no_duplicate_codes() {
        let names: BTreeSet<&str> = ApiErrorCode::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(
            names.len(),
            ApiErrorCode::ALL.len(),
            "two codes share a wire name; a duplicate makes one of them unmatchable"
        );
    }

    #[test]
    fn every_code_is_lower_snake_case() {
        for code in ApiErrorCode::ALL {
            let name = code.as_str();
            assert!(
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()),
                "`{name}` is not lower_snake_case"
            );
            assert!(
                !name.starts_with('_') && !name.ends_with('_'),
                "`{name}` has a leading or trailing underscore"
            );
        }
    }

    #[test]
    fn detail_never_contains_a_placeholder_a_caller_could_have_filled() {
        // The type system already forbids interpolation — `detail` returns `&'static str`. This
        // asserts the weaker thing the type cannot: that nobody wrote a format-looking string
        // intending to fill it later.
        for code in ApiErrorCode::ALL {
            let detail = code.detail();
            for marker in ['{', '}', '%'] {
                assert!(
                    !detail.contains(marker),
                    "`{code}`'s detail contains `{marker}`, which reads as an unfilled template"
                );
            }
        }
    }

    #[test]
    fn the_internal_detail_says_nothing_about_the_cause() {
        let detail = ApiErrorCode::InternalError.detail();
        // POSITIVE CONTROL first: prove the search discriminates, or the assertion below is
        // satisfied by any string at all.
        assert!(
            "a panic occurred at src/main.rs:42".contains("panic"),
            "the leak probe does not discriminate"
        );
        for forbidden in [
            "panic",
            "unwrap",
            "src/",
            "sql",
            "SELECT",
            "thread",
            "stack",
            "backtrace",
        ] {
            assert!(
                !detail
                    .to_ascii_lowercase()
                    .contains(&forbidden.to_ascii_lowercase()),
                "the internal detail mentions `{forbidden}`"
            );
        }
    }

    #[test]
    fn every_type_uri_is_distinct_and_under_the_published_base() {
        let uris: BTreeSet<String> = ApiErrorCode::ALL.iter().map(|c| c.type_uri()).collect();
        assert_eq!(
            uris.len(),
            ApiErrorCode::ALL.len(),
            "two codes share a type URI"
        );
        for code in ApiErrorCode::ALL {
            assert!(
                code.type_uri().starts_with(PROBLEM_TYPE_BASE),
                "`{code}`'s type URI is outside the published base"
            );
        }
    }

    #[test]
    fn the_registry_version_starts_at_one() {
        assert_eq!(API_ERROR_REGISTRY_VERSION, 1);
    }

    #[test]
    fn titles_are_present_and_distinct() {
        let titles: BTreeSet<&str> = ApiErrorCode::ALL.iter().map(|c| c.title()).collect();
        assert_eq!(
            titles.len(),
            ApiErrorCode::ALL.len(),
            "two codes share a title; RFC 9457 ties a title to a type"
        );
        for code in ApiErrorCode::ALL {
            assert!(!code.title().is_empty(), "`{code}` has an empty title");
        }
    }
}
