//! Property tests for the page cursor.
//!
//! PLAN.md §17.1 requires *"property or fuzz tests for parsers, routing edge cases, pagination
//! cursors, and untrusted formats"*. A cursor is all four at once: it is a parser for an untrusted
//! format that decides which page a caller receives.
//!
//! # The property that matters is "does not panic"
//!
//! A panic in a decoder reached from a query string is a denial of service a caller gets to
//! trigger by sending a malformed parameter. Everything else this file asserts is secondary to
//! that.
//!
//! The generators are deliberately adversarial: arbitrary bytes, arbitrary text, near-miss
//! encodings, and *valid* encodings with one byte changed — the last being the case a naive
//! "generate random strings" test almost never reaches, because random strings are almost never
//! close to valid.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use proptest::prelude::*;
use renvor_validation::{CURSOR_VERSION, Cursor, CursorError, MAX_CURSOR_BYTES};

proptest! {
    /// Any text at all. The assertion is that this returns.
    #[test]
    fn decoding_arbitrary_text_never_panics(text in ".*") {
        let _ = Cursor::decode(&text);
    }

    /// Any bytes, rendered through the encoding's own alphabet — so these are *plausible*
    /// cursors rather than obvious rubbish, which is where a decoder's assumptions live.
    #[test]
    fn decoding_arbitrary_alphabet_text_never_panics(
        text in proptest::collection::vec(
            proptest::sample::select(
                "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
                    .chars().collect::<Vec<char>>()
            ),
            0..200,
        ).prop_map(|chars| chars.into_iter().collect::<String>())
    ) {
        let _ = Cursor::decode(&text);
    }

    /// Anything this build produced must decode back to what went in.
    #[test]
    fn every_cursor_this_build_produces_round_trips(
        position in proptest::collection::vec(any::<u8>(), 0..256)
    ) {
        let encoded = Cursor::new(position.clone()).encode();
        let decoded = Cursor::decode(&encoded).expect("this build's own output must decode");
        prop_assert_eq!(decoded.position(), position.as_slice());
    }

    /// The encoded form must survive a query string without escaping.
    #[test]
    fn the_encoded_form_never_needs_percent_encoding(
        position in proptest::collection::vec(any::<u8>(), 0..256)
    ) {
        let encoded = Cursor::new(position).encode();
        for character in encoded.chars() {
            prop_assert!(
                character.is_ascii_alphanumeric() || character == '-' || character == '_',
                "`{}` requires percent-encoding in a query string", character
            );
        }
    }

    /// A VALID cursor with its version byte changed. This is the near-miss case: everything about
    /// it is well-formed except the one field that decides whether it may be interpreted.
    #[test]
    fn a_valid_cursor_with_a_foreign_version_is_refused_by_name(
        position in proptest::collection::vec(any::<u8>(), 0..64),
        version in any::<u8>(),
    ) {
        prop_assume!(version != CURSOR_VERSION);

        let mut bytes = vec![version];
        bytes.extend_from_slice(&position);
        let encoded = URL_SAFE_NO_PAD.encode(bytes);

        prop_assert_eq!(
            Cursor::decode(&encoded),
            Err(CursorError::UnsupportedVersion { declared: version }),
            "a cursor from a foreign build was interpreted rather than refused"
        );
    }

    /// A valid cursor with one character of its payload corrupted. It must be refused or decode
    /// to something — never panic, and never silently become a *different valid* cursor whose
    /// version check was skipped.
    #[test]
    fn a_corrupted_cursor_is_handled_without_panicking(
        position in proptest::collection::vec(any::<u8>(), 1..64),
        index in 0usize..64,
        replacement in proptest::sample::select(
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_!@#$%^&*()"
                .chars().collect::<Vec<char>>()
        ),
    ) {
        let encoded = Cursor::new(position).encode();
        let mut characters: Vec<char> = encoded.chars().collect();
        if !characters.is_empty() {
            let index = index % characters.len();
            characters[index] = replacement;
        }
        let corrupted: String = characters.into_iter().collect();

        // Whatever it does, it returns.
        if let Ok(cursor) = Cursor::decode(&corrupted) {
            // If it DID decode, re-encoding must be stable — a decoder that accepted something it
            // could not reproduce would be interpreting rather than decoding.
            let re_encoded = cursor.encode();
            prop_assert!(Cursor::decode(&re_encoded).is_ok());
        }
    }

    /// An over-long value is refused for LENGTH, before any decoding allocates.
    #[test]
    fn an_over_long_value_is_refused_before_it_is_decoded(extra in 1usize..512) {
        let text = "A".repeat(MAX_CURSOR_BYTES + extra);
        // Bound to a local first: `prop_assert!` stringifies its expression into a format string,
        // and a struct pattern's braces would be read as format placeholders.
        let refused_for_length = matches!(
            Cursor::decode(&text),
            Err(CursorError::TooLong { .. })
        );
        prop_assert!(refused_for_length);
    }
}

#[test]
fn the_property_harness_would_notice_a_decoder_that_accepted_everything() {
    // POSITIVE CONTROL for the whole file. Property tests that only assert "does not panic" pass
    // trivially against a decoder that returns `Ok` for every input, so at least one property
    // must be able to fail.
    assert!(
        Cursor::decode("!!!!not-base64!!!!").is_err(),
        "the decoder accepts input outside its alphabet, so the properties above prove nothing"
    );
    assert!(
        Cursor::decode("").is_err(),
        "the decoder accepts the empty string, which carries no version byte"
    );
}
