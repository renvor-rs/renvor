//! The opaque, versioned page cursor.
//!
//! # Opaque means opaque
//!
//! A cursor's internal structure is **not** a public contract. A consumer that decodes one is
//! relying on something Renvor does not promise, and the encoding may change without a
//! compatibility event — which is the whole reason the version byte exists.
//!
//! What *is* promised: a cursor is URL-safe, it round-trips, and a cursor this build does not
//! understand is **refused by name** rather than interpreted on a best-effort basis.
//!
//! # Why refusing beats guessing
//!
//! `contracts/http-routing.md` settled this for the route-dump payload:
//!
//! > *"A consumer checks `protocol` before reading the payload and refuses a version it does not
//! > understand, by name. Parsing an unknown version on a best-effort basis would let a route
//! > table silently lose a column."*
//!
//! A page is the same hazard with worse consequences: a best-effort decode of a foreign cursor
//! yields a position that is syntactically fine and semantically meaningless, so the caller
//! receives a page from nowhere and cannot tell.
//!
//! # This module executes no query
//!
//! A cursor carries a **position in an ordering**. Giving that position a storage meaning is
//! Phase 006's work. Nothing here opens a connection or builds a statement.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use crate::reason::Reason;

/// The current cursor encoding version.
///
/// A single byte, first in the encoded form, so a decoder reads it before anything else. Bumped
/// when the payload's meaning changes; a decoder meeting an unknown value refuses.
pub const CURSOR_VERSION: u8 = 1;

/// The maximum encoded cursor length, in bytes.
///
/// A cursor is a position, not a payload. The bound refuses an over-long value **before** it is
/// decoded, so a caller cannot make this process allocate by sending a very long parameter.
pub const MAX_CURSOR_BYTES: usize = 1024;

/// Why a cursor could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CursorError {
    /// The text was not a decodable cursor: wrong alphabet, truncated, or empty.
    Invalid,
    /// The cursor declared an encoding version this build does not understand.
    ///
    /// Distinct from [`CursorError::Invalid`] because they call for different actions: the first
    /// means the caller sent rubbish, the second means the caller has a cursor from a different
    /// build and should start the collection again.
    UnsupportedVersion {
        /// The version the cursor declared.
        declared: u8,
    },
    /// The encoded form exceeded [`MAX_CURSOR_BYTES`].
    TooLong {
        /// The observed length.
        length: usize,
        /// The bound.
        limit: usize,
    },
}

impl CursorError {
    /// The validation reason this maps to.
    #[must_use]
    pub const fn reason(self) -> Reason {
        match self {
            Self::Invalid | Self::TooLong { .. } => Reason::CursorInvalid,
            Self::UnsupportedVersion { .. } => Reason::CursorVersionUnsupported,
        }
    }
}

impl core::fmt::Display for CursorError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Invalid => f.write_str("the cursor could not be decoded"),
            Self::UnsupportedVersion { declared } => write!(
                f,
                "the cursor declares encoding version {declared}; this build understands \
                 {CURSOR_VERSION}. Refusing rather than guessing at a position it does not know"
            ),
            Self::TooLong { length, limit } => {
                write!(
                    f,
                    "the cursor is {length} bytes, above the {limit}-byte bound"
                )
            }
        }
    }
}

impl core::error::Error for CursorError {}

/// A position in a total ordering.
///
/// The payload is whatever the operation's ordering needs to resume — opaque to callers, and
/// opaque to this type, which neither interprets it nor gives it a storage meaning.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cursor {
    position: Vec<u8>,
}

impl Cursor {
    /// Builds a cursor over `position`.
    #[must_use]
    pub fn new(position: impl Into<Vec<u8>>) -> Self {
        Self {
            position: position.into(),
        }
    }

    /// The position bytes.
    ///
    /// Available to the code that produced them. A caller never reaches this — a cursor arrives as
    /// text and leaves as text.
    #[must_use]
    pub fn position(&self) -> &[u8] {
        &self.position
    }

    /// The encoded form: URL-safe, unpadded, version-prefixed.
    ///
    /// Unpadded because `=` is percent-encoded in a query string, which would make a cursor's
    /// length depend on where it was carried.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut bytes = Vec::with_capacity(self.position.len() + 1);
        bytes.push(CURSOR_VERSION);
        bytes.extend_from_slice(&self.position);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Decodes an encoded cursor.
    ///
    /// # This function does not panic, for any input
    ///
    /// Cursor text is untrusted and reaches a decoder, which is exactly the case PLAN.md §17.1
    /// names for property testing. `tests/cursor_property.rs` asserts the absence of panics over
    /// generated bytes and strings, including inputs shaped to look almost valid.
    ///
    /// # Errors
    ///
    /// - [`CursorError::TooLong`] before any decoding, so an over-long value is refused rather
    ///   than decoded and then measured.
    /// - [`CursorError::Invalid`] if the text is not valid URL-safe base64, or decodes to nothing.
    /// - [`CursorError::UnsupportedVersion`] if the version byte is not [`CURSOR_VERSION`].
    pub fn decode(text: &str) -> Result<Self, CursorError> {
        // LENGTH FIRST. Measuring after decoding would mean the allocation had already happened,
        // which is the defect `renvor-cli`'s dump bound records about itself one register up.
        if text.len() > MAX_CURSOR_BYTES {
            return Err(CursorError::TooLong {
                length: text.len(),
                limit: MAX_CURSOR_BYTES,
            });
        }

        let bytes = URL_SAFE_NO_PAD
            .decode(text)
            .map_err(|_| CursorError::Invalid)?;

        // An empty decode carries no version byte, so it cannot be checked — and a cursor with no
        // version is exactly what the version exists to refuse.
        let (&declared, position) = bytes.split_first().ok_or(CursorError::Invalid)?;

        // THE VERSION CHECK, BEFORE THE PAYLOAD IS USED.
        if declared != CURSOR_VERSION {
            return Err(CursorError::UnsupportedVersion { declared });
        }

        Ok(Self {
            position: position.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{CURSOR_VERSION, Cursor, CursorError, MAX_CURSOR_BYTES};
    use crate::reason::Reason;

    #[test]
    fn a_cursor_round_trips() {
        for position in [
            &b""[..],
            b"id:42",
            b"created_at=2026-08-23T00:00:00Z,id=7",
            &[0u8, 255, 128, 1][..],
        ] {
            let encoded = Cursor::new(position).encode();
            let decoded = Cursor::decode(&encoded).expect("a cursor this build produced");
            assert_eq!(decoded.position(), position);
        }
    }

    #[test]
    fn the_encoded_form_is_url_safe_and_unpadded() {
        let encoded = Cursor::new(b"\xff\xfe\xfd\xfc".to_vec()).encode();
        for forbidden in ['+', '/', '='] {
            assert!(
                !encoded.contains(forbidden),
                "`{encoded}` contains `{forbidden}`, which a query string percent-encodes"
            );
        }
    }

    #[test]
    fn an_unknown_version_is_refused_by_name_rather_than_interpreted() {
        // A cursor from a hypothetical future build: version 2, otherwise well-formed.
        use base64::Engine as _;
        let future = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([2u8, b'x', b'y']);

        assert_eq!(
            Cursor::decode(&future),
            Err(CursorError::UnsupportedVersion { declared: 2 }),
            "a future cursor was decoded on a best-effort basis"
        );
        assert_eq!(
            CursorError::UnsupportedVersion { declared: 2 }.reason(),
            Reason::CursorVersionUnsupported,
            "an unknown version reports the same reason as malformed input, which would hide it"
        );
    }

    #[test]
    fn malformed_truncated_and_foreign_input_is_refused() {
        for hostile in [
            "",                    // empty
            "!!!!",                // outside the alphabet
            "a",                   // decodes to nothing usable
            "////",                // standard base64, not URL-safe
            "AQ==",                // padded
            "not-a-cursor-at-all", // plausible-looking text
        ] {
            assert!(
                Cursor::decode(hostile).is_err(),
                "`{hostile}` was accepted as a cursor"
            );
        }

        // POSITIVE CONTROL: a genuine cursor still decodes, so the refusals above are about those
        // inputs rather than about decoding failing generally.
        let good = Cursor::new(b"id:1".to_vec()).encode();
        assert!(Cursor::decode(&good).is_ok(), "`{good}` was refused");
    }

    #[test]
    fn the_length_bound_is_enforced_at_its_exact_boundary() {
        let at = "A".repeat(MAX_CURSOR_BYTES);
        // At the bound the length check passes; whether it then decodes is a separate question,
        // and the point is that it is not refused FOR LENGTH.
        assert!(
            !matches!(Cursor::decode(&at), Err(CursorError::TooLong { .. })),
            "a cursor of exactly the bound was refused for length; the bound is inclusive"
        );

        let over = "A".repeat(MAX_CURSOR_BYTES + 1);
        assert!(
            matches!(Cursor::decode(&over), Err(CursorError::TooLong { .. })),
            "a cursor one byte past the bound was not refused for length"
        );
    }

    #[test]
    fn the_version_byte_leads_the_encoded_form() {
        use base64::Engine as _;
        let encoded = Cursor::new(b"payload".to_vec()).encode();
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&encoded)
            .expect("this build's own output decodes");
        assert_eq!(
            raw.first(),
            Some(&CURSOR_VERSION),
            "the version is not the first byte, so a decoder cannot check it first"
        );
    }
}
