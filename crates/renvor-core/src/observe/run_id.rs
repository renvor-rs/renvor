//! The run identifier: the single generation site, and the encoding that makes opacity checkable.
//!
//! # What FR-043 actually demands
//!
//! An identifier that encodes facts becomes an **unreviewed disclosure channel**. A run id built
//! from `hostname-pid-timestamp` leaks the host, the process, and the boot time to every consumer
//! of every log line, and nobody ever decided to publish those. FR-043 closes that by requiring
//! the identifier to carry no meaning at all.
//!
//! "Carries no meaning" is not testable as stated. What *is* testable is the constructive version:
//! the identifier is a **pure function of supplied entropy bytes and nothing else**. That is the
//! property this module implements; the unit tests below assert it against fixed entropy, and
//! `tests/run_id.rs` (T100) adds the full SC-019(b) matrix that varies hostname, clock, process
//! identifier, counter, and configuration while holding the entropy fixed.
//!
//! # Exactly one generation site
//!
//! [`RunIdentifier::generate`] is the only function in the crate that turns entropy into an
//! identifier. There is no `RunIdentifier::from_parts`, no `new_with_prefix`, and no `Default`.
//! Reviewing opacity therefore means reviewing one function, which is what makes SC-019(c)'s
//! "review of the single generation site" a bounded exercise rather than an audit of the codebase.
//!
//! # Why hex, and why 16 bytes
//!
//! 16 bytes is the same width as a UUID, which is the size operators already expect an opaque
//! correlation token to be. Lowercase hex is chosen over base64 or base32 because it has **one**
//! canonical form — no padding, no alphabet variants, no case ambiguity — so "the same bytes yield
//! the identical identifier in 100% of runs" is a property of the bytes rather than of the encoder
//! settings.
//!
//! Deliberately **not** a UUID: a v4 UUID spends six bits on version and variant markers, and a v7
//! UUID encodes a **timestamp**, which FR-043 forbids outright. Borrowing the format would invite
//! someone to later "upgrade" to v7 and silently reintroduce the disclosure.

use core::fmt;

use crate::observe::entropy::{EntropySource, EntropyUnavailable};

/// The number of entropy bytes an identifier carries.
pub const RUN_IDENTIFIER_BYTES: usize = 16;

/// The number of characters in the encoded form: two hex digits per byte.
pub const RUN_IDENTIFIER_LEN: usize = RUN_IDENTIFIER_BYTES * 2;

/// An opaque token identifying one application run.
///
/// Attached to every emitted record for the lifetime of the run (C-O3). Encodes **nothing** —
/// no hostname, timestamp, process identifier, counter, or configuration value (FR-043).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunIdentifier {
    bytes: [u8; RUN_IDENTIFIER_BYTES],
}

impl RunIdentifier {
    /// **The single generation site.** Produces an identifier from `source` and nothing else.
    ///
    /// Every input to the returned value passes through `source.fill`. There is no clock read, no
    /// hostname lookup, no process identifier, and no counter in this function — and because it is
    /// the only constructor, there is none anywhere.
    ///
    /// # Errors
    ///
    /// Returns [`EntropyUnavailable`] if the source cannot supply bytes. This propagates rather
    /// than falling back to a weaker source (C-E4).
    pub fn generate(source: &dyn EntropySource) -> Result<Self, EntropyUnavailable> {
        let mut bytes = [0_u8; RUN_IDENTIFIER_BYTES];
        source.fill(&mut bytes)?;
        Ok(Self { bytes })
    }

    /// The raw bytes behind the identifier.
    ///
    /// Exposed so a test can assert the encoding is a pure function of them. Callers should
    /// prefer the [`fmt::Display`] form for anything user-facing.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; RUN_IDENTIFIER_BYTES] {
        &self.bytes
    }

    /// The encoded form: 32 lowercase hex characters.
    ///
    /// Written by hand rather than via a formatting helper so the encoding has no configuration
    /// and no dependency — the mapping from bytes to characters is visible in one place and takes
    /// no input but `self.bytes`.
    #[must_use]
    pub fn encode(&self) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(RUN_IDENTIFIER_LEN);
        for byte in self.bytes {
            out.push(DIGITS[usize::from(byte >> 4)] as char);
            out.push(DIGITS[usize::from(byte & 0x0f)] as char);
        }
        out
    }
}

impl fmt::Display for RunIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.encode())
    }
}

/// Renders the encoded identifier, not the byte array.
///
/// A derived `Debug` would print `RunIdentifier { bytes: [17, 34, ...] }`, which is the same
/// information in a form nobody can correlate against a log line. Matching `Display` keeps a
/// debug-printed identifier greppable.
impl fmt::Debug for RunIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RunIdentifier({})", self.encode())
    }
}

#[cfg(test)]
mod tests {
    use super::{RUN_IDENTIFIER_BYTES, RUN_IDENTIFIER_LEN, RunIdentifier};
    use crate::observe::entropy::{FixedEntropy, OsEntropy};

    #[test]
    fn the_identifier_is_a_pure_function_of_the_supplied_bytes() {
        // SC-019(b), the deterministic half. Generating twice from the same fixed source yields
        // an identical identifier — no clock, counter, or process input can be involved, because
        // any of them would differ between the two calls.
        let source = FixedEntropy::new([0xde, 0xad, 0xbe, 0xef]);
        let first = RunIdentifier::generate(&source).unwrap();
        let second = RunIdentifier::generate(&source).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.encode(), second.encode());
        assert_eq!(first.encode(), "deadbeefdeadbeefdeadbeefdeadbeef");
    }

    #[test]
    fn different_entropy_yields_a_different_identifier() {
        // POSITIVE CONTROL for the purity test: an implementation returning a constant would
        // satisfy it. This proves the entropy actually reaches the output.
        let a = RunIdentifier::generate(&FixedEntropy::new([0x01])).unwrap();
        let b = RunIdentifier::generate(&FixedEntropy::new([0x02])).unwrap();
        assert_ne!(a, b);
        assert_ne!(a.encode(), b.encode());
    }

    #[test]
    fn the_encoding_is_canonical_and_fixed_width() {
        let id = RunIdentifier::generate(&FixedEntropy::new([0x0f])).unwrap();
        let encoded = id.encode();
        assert_eq!(encoded.len(), RUN_IDENTIFIER_LEN);
        assert_eq!(encoded.len(), 32);
        assert!(
            encoded
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "the encoding must have one canonical form: {encoded}"
        );
        // Leading zeros survive. A numeric encoding would drop them and make the identifier
        // variable-width, which breaks alignment in logs and equality against a stored string.
        let zeroed = RunIdentifier::generate(&FixedEntropy::new([0x00])).unwrap();
        assert_eq!(zeroed.encode(), "0".repeat(32));
    }

    #[test]
    fn every_entropy_byte_reaches_the_encoding() {
        // If the encoder dropped or duplicated a byte, the purity tests above would still pass.
        // This walks each position and asserts a change there changes the output.
        let baseline = RunIdentifier::generate(&FixedEntropy::new([0x00])).unwrap();
        for position in 0..RUN_IDENTIFIER_BYTES {
            let mut bytes = [0_u8; RUN_IDENTIFIER_BYTES];
            bytes[position] = 0xff;
            let varied = RunIdentifier::generate(&FixedEntropy::new(bytes.to_vec())).unwrap();
            assert_ne!(
                varied.encode(),
                baseline.encode(),
                "changing entropy byte {position} did not change the identifier"
            );
        }
    }

    #[test]
    fn debug_and_display_agree_so_an_identifier_stays_greppable() {
        let id = RunIdentifier::generate(&FixedEntropy::new([0xab])).unwrap();
        assert!(format!("{id:?}").contains(&id.to_string()));
        assert_eq!(id.to_string(), id.encode());
    }

    #[test]
    fn the_production_source_wires_into_the_same_single_site() {
        // SC-019(c): the production path uses the OS CSPRNG through the identical constructor.
        // There is no separate production code path to review.
        let id = RunIdentifier::generate(&OsEntropy::new()).expect("OS CSPRNG available");
        assert_eq!(id.encode().len(), RUN_IDENTIFIER_LEN);
    }
}
