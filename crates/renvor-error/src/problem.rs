//! The RFC 9457 Problem Details document.
//!
//! # Redaction is enforced by the types, not by discipline
//!
//! Three properties below are guaranteed by construction rather than by review:
//!
//! | Guarantee | How the type enforces it |
//! |---|---|
//! | `detail` cannot carry runtime data | it is `&'static str` — there is no runtime value that inhabits the type |
//! | An invalid parameter cannot carry the rejected value | [`InvalidParam`] has **no field** a value could occupy |
//! | A reason cannot be a library's message | [`InvalidParam::reason`] is `&'static str`, so a formatted string cannot be stored |
//!
//! The third guarantee exists because a real validator produced
//! `"not an object" is not of type "object"` during this phase's tooling spike — a message that
//! embeds the rejected value in its text. Any design that rendered a validator's `Display` into a
//! response would have leaked it.
//!
//! # There is no open extension map
//!
//! RFC 9457 §3.2 permits arbitrary extension members. Renvor declares **three** — `code`,
//! `correlationId`, and `invalidParams` — as typed fields. An open map would be a channel through
//! which an unreviewed value reaches a response, which is the single thing this module exists to
//! prevent.

use core::fmt;

use serde::Serialize;

use crate::code::ApiErrorCode;

/// The media type RFC 9457 §3 defines for these documents.
///
/// **Not** `application/json`. A consumer distinguishes a problem document from a successful
/// payload by media type, and serving one as the other defeats that.
pub const PROBLEM_MEDIA_TYPE: &str = "application/problem+json";

/// The maximum length of a structural pointer.
///
/// A pointer is built from **declared** member names, so a legitimate one is short. The bound
/// exists because a bug upstream must not be able to put an unbounded string into a response.
pub const MAX_POINTER_BYTES: usize = 512;

/// Where an input arrived.
///
/// A closed enum. `cookie` is deliberately absent: a cookie is an authentication and session
/// carrier, and Phase 009 owns authentication. Declaring cookie validation in a phase with no
/// opinion about a cookie's meaning would invite an author to validate a session cookie's shape
/// without validating its authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Location {
    /// A path parameter captured by the route pattern.
    Path,
    /// A query-string parameter.
    Query,
    /// A request header.
    Header,
    /// The request body.
    Body,
}

impl Location {
    /// The wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::Query => "query",
            Self::Header => "header",
            Self::Body => "body",
        }
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a [`Pointer`] could not be built.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointerError {
    /// The pointer contained a control character.
    ControlCharacter,
    /// The pointer exceeded [`MAX_POINTER_BYTES`].
    TooLong {
        /// The observed length in bytes.
        length: usize,
        /// The bound.
        limit: usize,
    },
}

impl fmt::Display for PointerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ControlCharacter => f.write_str(
                "a structural pointer may not contain a control character; a pointer reaches a \
                 response, and a control character there can reshape what a consumer renders",
            ),
            Self::TooLong { length, limit } => write!(
                f,
                "a structural pointer of {length} bytes exceeds the {limit}-byte bound"
            ),
        }
    }
}

impl core::error::Error for PointerError {}

/// A structural pointer to the input that failed.
///
/// For a body this is an RFC 6901 JSON Pointer; for a path, query, or header parameter it is the
/// declared parameter name.
///
/// # Why this is a newtype with a fallible constructor
///
/// The invariant that a pointer is built only from **declared** names is enforced where pointers
/// are produced, in `renvor-validation`. This type is the second line: even a defect upstream
/// cannot put a control character or an unbounded string into a response, because the only way to
/// obtain a `Pointer` is through a constructor that refuses both.
///
/// Belt and braces, deliberately — the same reasoning the repository's ignore rules record.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Pointer(String);

impl Pointer {
    /// Builds a pointer.
    ///
    /// # Errors
    ///
    /// - [`PointerError::ControlCharacter`] if the text contains a control character.
    /// - [`PointerError::TooLong`] if it exceeds [`MAX_POINTER_BYTES`].
    pub fn new(text: impl Into<String>) -> Result<Self, PointerError> {
        let text = text.into();
        if text.len() > MAX_POINTER_BYTES {
            return Err(PointerError::TooLong {
                length: text.len(),
                limit: MAX_POINTER_BYTES,
            });
        }
        if text.chars().any(char::is_control) {
            return Err(PointerError::ControlCharacter);
        }
        Ok(Self(text))
    }

    /// The pointer text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Pointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One input that failed, identified without being quoted.
///
/// # There is no `value` field, and that is the point
///
/// FR-004 forbids a rejected value from reaching a response. A design that carried the value and
/// omitted it at serialisation time would rest on the omission being remembered. This type has
/// nowhere to put one, so adding one would be a visible change to a public type rather than an
/// invisible change to a format string.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InvalidParam {
    /// Where the input arrived.
    pub location: Location,
    /// Which input, structurally.
    pub pointer: Pointer,
    /// Why it was refused.
    ///
    /// A `&'static str` drawn from a closed vocabulary — never a formatted message, and never a
    /// third-party validator's `Display`, which quotes the offending input.
    pub reason: &'static str,
}

/// An RFC 9457 Problem Details document.
///
/// Member semantics follow RFC 9457 exactly; the three extension members are documented on their
/// fields. Build one with [`ProblemDetails::new`] — every member is then derived from the code, so
/// a document cannot be constructed with a `title` that disagrees with its `type`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProblemDetails {
    /// RFC 9457 §3.1.1 `type` — a URI identifying the problem **kind**.
    #[serde(rename = "type")]
    pub problem_type: String,

    /// RFC 9457 §3.1.2 `title` — fixed for the type, never occurrence-specific.
    pub title: &'static str,

    /// RFC 9457 §3.1.3 `status`.
    ///
    /// The RFC makes this advisory and permits it to differ from the response's actual status.
    /// **Renvor forbids that**: two disagreeing status values is a contradiction a consumer cannot
    /// resolve, and `renvor-http` asserts equality where the response is rendered.
    pub status: u16,

    /// RFC 9457 §3.1.4 `detail`.
    ///
    /// `&'static str`, derived from the code. See [`ApiErrorCode::detail`] for why the type is the
    /// guarantee.
    pub detail: &'static str,

    /// RFC 9457 §3.1.5 `instance` — identifies this **occurrence**.
    ///
    /// Derived from the correlation identifier, **never** from the request path. A path would echo
    /// attacker-controlled input into the document, which is the disclosure the rest of this
    /// module is built to prevent.
    pub instance: String,

    /// **Extension.** The stable Renvor code.
    ///
    /// An extension member rather than an overload of `type` or `title`, because both of those
    /// have defined RFC semantics that overloading would break, and because a consumer should not
    /// have to parse a URI to obtain a code.
    pub code: &'static str,

    /// **Extension.** The request identifier this failure belongs to.
    ///
    /// The **same value** the `x-request-id` response header and the telemetry record carry. Not a
    /// derivation of it and not a second identity — see `renvor-http`'s request context.
    #[serde(rename = "correlationId")]
    pub correlation_id: String,

    /// **Extension.** The inputs that failed, when the failure was a validation failure.
    ///
    /// Omitted from the wire entirely when empty, rather than serialised as `[]`: an empty array
    /// invites a consumer to conclude that a non-validation failure had zero invalid parameters,
    /// which is a different statement from "this failure is not about parameters".
    #[serde(rename = "invalidParams", skip_serializing_if = "Vec::is_empty")]
    pub invalid_params: Vec<InvalidParam>,
}

impl ProblemDetails {
    /// Builds a document for `code` at `status`, correlated to `correlation_id`.
    ///
    /// `title`, `detail`, `type`, and `code` are all derived from `code`, so they cannot disagree.
    /// The caller supplies only the two things the code does not determine: which status was
    /// actually sent, and which request this was.
    #[must_use]
    pub fn new(code: ApiErrorCode, status: u16, correlation_id: impl Into<String>) -> Self {
        let correlation_id = correlation_id.into();
        Self {
            problem_type: code.type_uri(),
            title: code.title(),
            status,
            detail: code.detail(),
            // A URN rather than a path. There is no endpoint that serves an occurrence, and a
            // URI shaped like a route would promise one that does not exist.
            instance: format!("urn:renvor:request:{correlation_id}"),
            code: code.as_str(),
            correlation_id,
            invalid_params: Vec::new(),
        }
    }

    /// Attaches the inputs that failed.
    #[must_use]
    pub fn with_invalid_params(mut self, params: Vec<InvalidParam>) -> Self {
        self.invalid_params = params;
        self
    }

    /// Renders the document as `application/problem+json`.
    ///
    /// # Errors
    ///
    /// Propagates a serialisation failure rather than substituting an empty document. A consumer
    /// that received a parseable document saying nothing would have no way to tell that from a
    /// failure with no details.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        InvalidParam, Location, MAX_POINTER_BYTES, PROBLEM_MEDIA_TYPE, Pointer, PointerError,
        ProblemDetails,
    };
    use crate::code::ApiErrorCode;

    const CANARY: &str = "CANARY-cf19b7d2e4a08153-REJECTED-VALUE";

    #[test]
    fn the_media_type_is_the_one_rfc_9457_defines() {
        assert_eq!(PROBLEM_MEDIA_TYPE, "application/problem+json");
    }

    #[test]
    fn every_member_is_derived_from_the_code_so_none_can_disagree() {
        let code = ApiErrorCode::ValidationFailed;
        let problem = ProblemDetails::new(code, 400, "0123456789abcdef");

        assert_eq!(problem.code, code.as_str());
        assert_eq!(problem.title, code.title());
        assert_eq!(problem.detail, code.detail());
        assert_eq!(problem.problem_type, code.type_uri());
        assert_eq!(problem.status, 400);
        assert_eq!(problem.correlation_id, "0123456789abcdef");
        assert!(problem.instance.contains("0123456789abcdef"));
    }

    #[test]
    fn the_instance_never_carries_a_request_path() {
        let problem = ProblemDetails::new(ApiErrorCode::NotFound, 404, "0123456789abcdef");
        // A path would be the obvious thing to put here, and is exactly what must not be here.
        assert!(
            !problem.instance.starts_with('/'),
            "the instance looks like a request path"
        );
        assert!(problem.instance.starts_with("urn:renvor:request:"));
    }

    #[test]
    fn a_rejected_value_cannot_reach_the_document() {
        // POSITIVE CONTROL: prove the search finds the canary when it IS present, or the negative
        // assertion below is satisfied by a search that never works.
        let control = format!(r#"{{"detail":"{CANARY}"}}"#);
        assert!(
            control.contains(CANARY),
            "the canary probe does not discriminate"
        );

        let problem = ProblemDetails::new(ApiErrorCode::ValidationFailed, 400, "0123456789abcdef")
            .with_invalid_params(vec![InvalidParam {
                location: Location::Body,
                pointer: Pointer::new("/name").expect("a valid pointer"),
                reason: "too_long",
            }]);

        let rendered = problem.to_json().expect("the document serialises");
        assert!(
            !rendered.contains(CANARY),
            "the rejected value reached the document"
        );
        // And the useful part IS present, so the assertion above is not passing because the
        // document is empty.
        assert!(rendered.contains("/name"), "the pointer is missing");
        assert!(rendered.contains("too_long"), "the reason is missing");
    }

    #[test]
    fn invalid_params_are_omitted_entirely_when_there_are_none() {
        let problem = ProblemDetails::new(ApiErrorCode::NotFound, 404, "0123456789abcdef");
        let rendered = problem.to_json().expect("serialises");
        assert!(
            !rendered.contains("invalidParams"),
            "an empty invalidParams was serialised; `[]` says something different from absence"
        );

        // POSITIVE CONTROL: it IS serialised when there is one.
        let with = problem.with_invalid_params(vec![InvalidParam {
            location: Location::Query,
            pointer: Pointer::new("page").expect("a valid pointer"),
            reason: "out_of_range",
        }]);
        assert!(
            with.to_json()
                .expect("serialises")
                .contains("invalidParams"),
            "invalidParams was omitted when it was non-empty"
        );
    }

    #[test]
    fn the_wire_shape_uses_the_member_names_rfc_9457_defines() {
        let rendered = ProblemDetails::new(ApiErrorCode::Unavailable, 503, "0123456789abcdef")
            .to_json()
            .expect("serialises");
        let value: serde_json::Value = serde_json::from_str(&rendered).expect("parses");
        let object = value.as_object().expect("an object");

        for member in ["type", "title", "status", "detail", "instance"] {
            assert!(object.contains_key(member), "`{member}` is missing");
        }
        for extension in ["code", "correlationId"] {
            assert!(object.contains_key(extension), "`{extension}` is missing");
        }
        // `status` is a NUMBER, per RFC 9457 §3.1.3. A string would need parsing to compare.
        assert!(object["status"].is_number(), "`status` is not a number");
    }

    #[test]
    fn a_pointer_refuses_a_control_character() {
        for hostile in ["/name\n", "/na\rme", "/name\u{0}", "\u{1b}[31m"] {
            assert_eq!(
                Pointer::new(hostile),
                Err(PointerError::ControlCharacter),
                "`{hostile:?}` was accepted as a pointer"
            );
        }
        // POSITIVE CONTROLS: ordinary pointers still build, so the refusal is about control
        // characters rather than about pointers being rejected wholesale.
        for valid in ["/name", "/items/0/quantity", "page", "x-request-id", ""] {
            assert!(
                Pointer::new(valid).is_ok(),
                "`{valid}` was refused, but it is a legitimate pointer"
            );
        }
    }

    #[test]
    fn a_pointer_is_bounded_at_its_exact_boundary() {
        let at = "a".repeat(MAX_POINTER_BYTES);
        assert!(
            Pointer::new(at).is_ok(),
            "a pointer of exactly the bound was refused; the bound is inclusive"
        );

        let over = "a".repeat(MAX_POINTER_BYTES + 1);
        assert!(
            matches!(Pointer::new(over), Err(PointerError::TooLong { .. })),
            "a pointer one byte past the bound was accepted"
        );
    }

    #[test]
    fn a_status_disagreeing_with_the_code_is_the_callers_to_supply_and_is_recorded_as_sent() {
        // The document records the status that was ACTUALLY sent. `renvor-http` is what makes the
        // two agree; this type does not invent a status, because a type that chose one would
        // disagree with the response whenever the transport had a reason to differ.
        let problem = ProblemDetails::new(ApiErrorCode::ValidationFailed, 422, "0123456789abcdef");
        assert_eq!(problem.status, 422);
    }
}
