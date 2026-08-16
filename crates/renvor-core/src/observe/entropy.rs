//! The entropy port, its production implementation, and the fixed source used to make opacity
//! deterministically testable.
//!
//! # Why entropy is a port at all
//!
//! FR-043 requires the run identifier to be opaque **by construction**, and SC-019(b) requires
//! that guarantee to be verified **deterministically** rather than statistically. Those two
//! demands together force this design.
//!
//! A statistical test — generate a thousand identifiers, assert no collisions — is a probabilistic
//! statement about a random source. It can fail on a *correct* implementation, and a gate that
//! fails by chance teaches a team to re-run gates rather than to trust them. The only way to
//! assert "nothing but entropy reaches the identifier" without probability is to **hold the
//! entropy fixed** and vary everything else. That requires the randomness to be injectable, which
//! is what [`EntropySource`] is for.
//!
//! # The production constructor takes no inputs
//!
//! SC-019(c) requires that **1 of 1** production entropy sources is the operating-system CSPRNG,
//! verified by a test asserting the constructor accepts **0** inputs other than that source.
//! [`OsEntropy::new`] therefore takes no arguments — there is no seed parameter, no configuration
//! struct, and no builder. A constructor that accepted a seed would be a place where a hostname,
//! a clock reading, or a counter could enter, and the requirement exists precisely to close that
//! door.
//!
//! # Failure is loud
//!
//! [`EntropySource::fill`] is fallible and there is **no fallback**. If the OS CSPRNG is
//! unavailable, the kernel fails rather than substituting a weaker source — contract C-E4 and
//! constitution principle III both forbid degrading a required capability. Silently falling back
//! to a timestamp seed is the classic version of this bug, and it is unavailable here because
//! nothing in this module can produce bytes except the source itself.

use core::fmt;

/// Why entropy could not be obtained.
///
/// Deliberately opaque about the underlying cause in its `Display`: the operating system's error
/// text for a CSPRNG failure is not actionable for an application author, and reproducing it
/// verbatim invites treating this as a recoverable condition. It is not.
#[derive(Debug)]
pub struct EntropyUnavailable {
    source: Box<dyn std::error::Error + Send + Sync + 'static>,
}

impl EntropyUnavailable {
    /// Reports that a source could not supply the bytes it was asked for.
    ///
    /// Public because [`EntropySource`] is a public, **fallible** trait: an implementor outside
    /// this module has to be able to say it failed. Without this, the trait could be named from
    /// anywhere and implemented only from here — and the fallible half of it, the half C-E4 cares
    /// about, could never be exercised by a test double. Found by writing that test.
    #[must_use]
    pub fn new(source: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> Self {
        Self {
            source: source.into(),
        }
    }
}

impl fmt::Display for EntropyUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the operating-system random source is unavailable")
    }
}

impl std::error::Error for EntropyUnavailable {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// A source of bytes used to construct a run identifier.
///
/// The trait exists so opacity can be tested by holding entropy fixed while varying every other
/// input. It is **not** a general-purpose randomness abstraction and should not grow one: the
/// only implementations that should ever exist are the OS CSPRNG and a test double.
pub trait EntropySource: fmt::Debug + Send + Sync {
    /// Fills `destination` completely with entropy.
    ///
    /// # Errors
    ///
    /// Returns [`EntropyUnavailable`] if the source cannot supply the requested bytes. Callers
    /// **must** propagate this. There is no correct fallback.
    fn fill(&self, destination: &mut [u8]) -> Result<(), EntropyUnavailable>;
}

/// The production entropy source: the operating-system CSPRNG, and nothing else.
///
/// SC-019(c) is satisfied structurally — this type has no fields, so there is nowhere for another
/// input to be stored, and [`Self::new`] takes no arguments, so there is nowhere for one to enter.
#[derive(Clone, Copy, Debug, Default)]
pub struct OsEntropy;

impl OsEntropy {
    /// Creates the production entropy source.
    ///
    /// Takes **0** inputs. This signature is load-bearing, not incidental — see the module
    /// documentation and SC-019(c).
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl EntropySource for OsEntropy {
    fn fill(&self, destination: &mut [u8]) -> Result<(), EntropyUnavailable> {
        getrandom::fill(destination).map_err(EntropyUnavailable::new)
    }
}

/// A test entropy source that returns a fixed byte sequence.
///
/// This is what makes SC-019(b) deterministic: with the bytes pinned, the acceptance test varies
/// hostname, clock, process id, counter, and the entire configuration, and asserts **0** bytes of
/// the resulting identifier change.
///
/// Available outside `cfg(test)` on purpose — `renvor-testkit` needs it, and an author writing a
/// deterministic test of their own boot sequence needs it too.
#[derive(Clone, Debug)]
pub struct FixedEntropy {
    bytes: Vec<u8>,
}

impl FixedEntropy {
    /// Creates a source that yields `bytes`, repeating cyclically if more are requested.
    ///
    /// # Panics
    ///
    /// Panics if `bytes` is empty. An empty fixed source would fill destinations with nothing and
    /// silently produce an all-zero identifier, which looks like a working test and is not one.
    #[must_use]
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        let bytes = bytes.into();
        assert!(
            !bytes.is_empty(),
            "FixedEntropy needs at least one byte; an empty source yields a silently all-zero \
             identifier that reads as a passing test"
        );
        Self { bytes }
    }
}

impl EntropySource for FixedEntropy {
    fn fill(&self, destination: &mut [u8]) -> Result<(), EntropyUnavailable> {
        for (slot, byte) in destination.iter_mut().zip(self.bytes.iter().cycle()) {
            *slot = *byte;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{EntropySource, FixedEntropy, OsEntropy};

    #[test]
    fn the_production_source_yields_bytes() {
        let mut buffer = [0_u8; 32];
        OsEntropy::new()
            .fill(&mut buffer)
            .expect("the OS CSPRNG is available in a test environment");

        // NOT a statistical assertion — SC-019(d) keeps those out of the gating set. This only
        // asserts the buffer was written at all, which distinguishes "filled" from "ignored".
        // A 32-byte all-zero result from a working CSPRNG is possible but would also be the exact
        // signature of a no-op implementation, so it is worth failing on.
        assert!(
            buffer.iter().any(|&b| b != 0),
            "the OS source left the buffer untouched"
        );
    }

    #[test]
    fn the_fixed_source_is_reproducible() {
        let source = FixedEntropy::new([1, 2, 3, 4]);
        let mut first = [0_u8; 8];
        let mut second = [0_u8; 8];
        source.fill(&mut first).expect("fixed source cannot fail");
        source.fill(&mut second).expect("fixed source cannot fail");
        assert_eq!(first, second, "the same source must yield the same bytes");
        assert_eq!(first, [1, 2, 3, 4, 1, 2, 3, 4], "it cycles, in order");
    }

    #[test]
    fn different_fixed_sources_yield_different_bytes() {
        // POSITIVE CONTROL for the reproducibility test above: an implementation that always
        // wrote the same constant would satisfy that test and fail this one.
        let mut a = [0_u8; 4];
        let mut b = [0_u8; 4];
        FixedEntropy::new([9_u8]).fill(&mut a).unwrap();
        FixedEntropy::new([7_u8]).fill(&mut b).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    #[should_panic(expected = "at least one byte")]
    fn an_empty_fixed_source_is_rejected_rather_than_silently_producing_zeros() {
        let _ = FixedEntropy::new(Vec::new());
    }
}
