//! Generated credential secrets, and the digests stored in their place.
//!
//! # What makes this type safe is what it does *not* have
//!
//! [`Opaque`] carries a session identifier, a verification token, a reset token, or a refresh
//! token — every value in this phase whose disclosure is a compromise. The guarantees are
//! structural, in the manner `renvor-error` established:
//!
//! | Absent | Why |
//! |---|---|
//! | derived `Debug` | it prints the bytes. This module's first pass had one, and its own test caught it |
//! | `Display` rendering the value | a secret in a format string is the commonest way one reaches a log |
//! | `Serialize` | serialising a credential is a compile error, not a review finding |
//! | `PartialEq` | equality would be byte-comparison, which is variable-time. Comparison goes through [`SecretDigest`] and `subtle` |
//! | `Deref`, `AsRef`, `Into<String>` | [`Opaque::expose`] is the only way out, so every disclosure has a visible call site |
//!
//! That table is the same argument `renvor_config::Secret` makes, and it is repeated here rather
//! than delegated because `Secret<T>` is a *configuration* type: it wraps a value someone supplied,
//! whereas this one is *generated* and must also be verifiable in constant time.
//!
//! # The digest is what the database sees
//!
//! FR-016, FR-041 and FR-048 all say the same thing about different tables: the stored form must
//! not be usable. So the secret goes to the caller **once** and [`SecretDigest`] goes to the
//! database. A stolen backup yields digests, and a digest cannot be presented.

use core::fmt;

use renvor_core::observe::entropy::EntropySource;
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;

use crate::error::AuthError;

/// How many entropy bytes every generated credential carries.
///
/// 32, not 16. `renvor_core::observe::run_id` uses 16 for a *correlation* token, which is public
/// by design; this is a bearer secret, where the width is the attack cost.
pub const OPAQUE_BYTES: usize = 32;

/// The rendered width of a credential: two hex characters per byte.
pub const OPAQUE_LEN: usize = OPAQUE_BYTES * 2;

/// What every output form of a credential renders as.
///
/// One constant, so `Debug`, `Display`, and the tests cannot disagree about it — the same
/// arrangement `renvor_config::secret::REDACTED` uses.
pub const REDACTED: &str = "[redacted]";

/// Which kind of credential a secret is.
///
/// A closed set. One kind per purpose, so a verification token cannot be presented where a reset
/// token is expected — FR-053's "invalidated after relevant account changes" is per-purpose, and a
/// shared type would make that unenforceable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum OpaqueKind {
    /// A cookie session identifier.
    Session,
    /// An email-verification token.
    Verification,
    /// A password-reset token.
    PasswordReset,
    /// A refresh token.
    Refresh,
}

/// A generated credential secret.
///
/// See the module header for what this type deliberately cannot do.
#[derive(Clone)]
pub struct Opaque {
    kind: OpaqueKind,
    bytes: [u8; OPAQUE_BYTES],
}

impl Opaque {
    /// **The single generation site.** Produces a credential from `source` and nothing else.
    ///
    /// No clock read, no counter, no user identifier enters here — the same property
    /// `renvor_core::observe::run_id::RunIdentifier::generate` asserts, and for a stronger reason:
    /// a session identifier that encoded a user id would be forgeable.
    ///
    /// # Errors
    ///
    /// [`AuthError::EntropyUnavailable`] if the source cannot supply bytes. **There is no
    /// fallback** — `renvor_core`'s entropy port refuses to substitute a weaker source, and this
    /// refuses to proceed without one.
    pub fn generate(kind: OpaqueKind, source: &dyn EntropySource) -> Result<Self, AuthError> {
        let mut bytes = [0_u8; OPAQUE_BYTES];
        source
            .fill(&mut bytes)
            .map_err(|_| AuthError::EntropyUnavailable)?;
        Ok(Self { kind, bytes })
    }

    /// Rebuilds a credential a caller presented, for verification.
    ///
    /// Returns `None` when the text is not exactly [`OPAQUE_LEN`] lowercase hex characters. A
    /// malformed credential is refused here rather than reaching a database query.
    #[must_use]
    pub fn from_wire(kind: OpaqueKind, wire: &str) -> Option<Self> {
        if wire.len() != OPAQUE_LEN {
            return None;
        }
        let mut bytes = [0_u8; OPAQUE_BYTES];
        for (index, pair) in wire.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            let high = hex_value(pair[0])?;
            let low = hex_value(pair[1])?;
            bytes[index] = (high << 4) | low;
        }
        Some(Self { kind, bytes })
    }

    /// Which kind of credential this is.
    #[must_use]
    pub const fn kind(&self) -> OpaqueKind {
        self.kind
    }

    /// The wire form, handed to the caller **once**.
    ///
    /// Named to be conspicuous, for the reason `renvor_config::Secret::expose` gives: there is no
    /// `Deref` and no `Into<String>`, so every disclosure is visible at its call site in review.
    #[must_use]
    pub fn expose(&self) -> String {
        let mut out = String::with_capacity(OPAQUE_LEN);
        for byte in self.bytes {
            out.push(hex_digit(byte >> 4));
            out.push(hex_digit(byte & 0x0f));
        }
        out
    }
}

/// One lowercase hex digit.
const fn hex_digit(nibble: u8) -> char {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    DIGITS[nibble as usize] as char
}

/// The value of one lowercase hex character.
const fn hex_value(character: u8) -> Option<u8> {
    match character {
        b'0'..=b'9' => Some(character - b'0'),
        b'a'..=b'f' => Some(character - b'a' + 10),
        _ => None,
    }
}

/// Renders the placeholder. **Never the value.**
impl fmt::Debug for Opaque {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The KIND is shown and the bytes are not: a diagnostic that hides which credential failed
        // is not diagnosable, which is the same trade `renvor_config::Secret` makes by keeping its
        // configuration key visible.
        write!(f, "Opaque({:?}, {REDACTED})", self.kind)
    }
}

/// Renders the placeholder. **Never the value.**
impl fmt::Display for Opaque {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

/// The SHA-256 digest stored in a secret's place.
///
/// A plain hash rather than a password hash, deliberately: these are **32 bytes of entropy**, not
/// a human-chosen password, so there is nothing to brute-force and a slow KDF would buy nothing
/// while making every session lookup expensive. Argon2id is for passwords, and batch B applies it
/// there.
#[derive(Clone, Copy)]
pub struct SecretDigest([u8; 32]);

impl SecretDigest {
    /// Digests a credential.
    #[must_use]
    pub fn of(secret: &Opaque) -> Self {
        let mut hasher = Sha256::new();
        // The kind is bound into the digest, so a token stolen from one table cannot be replayed
        // against another even if the bytes were somehow reused.
        hasher.update([secret.kind as u8]);
        hasher.update(secret.bytes);
        Self(hasher.finalize().into())
    }

    /// Whether two digests match, in **constant time**.
    ///
    /// `Opaque` deliberately has no `PartialEq`, so this is the only comparison available. A
    /// variable-time comparison of a session digest leaks its prefix to a patient attacker.
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }

    /// Rebuilds a digest from stored bytes.
    ///
    /// # Why this exists and why it is not a hole
    ///
    /// A persistence adapter reads `token_hash` back out of a row and must return it through a
    /// port typed on this. Safe because **a digest is not a secret**: holding one lets you look a
    /// row up, which is what the adapter already did to obtain it. The constructor that would be a
    /// hole is one for [`Opaque`], and that is [`Opaque::from_wire`], which validates.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The stored form.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Shows the digest, which is safe: it is what the database already holds.
impl fmt::Debug for SecretDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretDigest(")?;
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        f.write_str(")")
    }
}

#[cfg(test)]
mod tests {
    use super::{OPAQUE_BYTES, OPAQUE_LEN, Opaque, OpaqueKind, REDACTED, SecretDigest};
    use renvor_core::observe::entropy::FixedEntropy;

    fn fixture() -> Opaque {
        let source = FixedEntropy::new(vec![0xAB, 0xCD, 0xEF, 0x12]);
        Opaque::generate(OpaqueKind::Session, &source).expect("fixed entropy always succeeds")
    }

    #[test]
    fn debug_does_not_render_the_secret() {
        // FR-026: no session secret in a log. `{:?}` is the commonest way one gets there — a
        // struct logged in an error context, a `dbg!` left behind, a tracing field.
        //
        // THIS TEST FAILED ON THE FIRST IMPLEMENTATION, which used a derived `Debug` and printed
        // `bytes: [171, 205, 239, 18, ...]`. That is the RED this test exists to have caught.
        //
        // NO DIAGNOSTIC BELOW INTERPOLATES THE RENDERING, THE PROBE, OR THE SECRET. A message
        // that printed `rendered` would put the very credential this test defends into the test
        // log at exactly the moment the defence failed — reporting the leak by leaking. Each
        // assertion is identified by a static description, and the loop by its probe index, so a
        // failure says which check broke without saying what escaped.
        let secret = fixture();
        let rendered = format!("{secret:?}");
        assert!(
            rendered.contains(REDACTED),
            "Debug omitted the redaction placeholder"
        );
        assert!(
            !rendered.contains(&secret.expose()),
            "Debug rendered the secret in full"
        );
        // The raw bytes in BOTH renderings a derived Debug could produce.
        for (probe, leaked) in ["171", "205", "239", "abcdef"].into_iter().enumerate() {
            assert!(
                !rendered.contains(leaked),
                "Debug leaked the raw bytes matched by probe {probe}"
            );
        }
        // ...and the kind is still visible, or the diagnostic is useless.
        assert!(rendered.contains("Session"), "Debug did not name the kind");
    }

    #[test]
    fn display_does_not_render_the_secret() {
        let secret = fixture();
        let rendered = format!("{secret}");
        // `assert_eq!` would print both operands, and the left one is the leaked rendering
        // itself. Compared inside `assert!` instead, so the failure names the fault only.
        assert!(
            rendered == REDACTED,
            "Display was not exactly the redaction placeholder"
        );
        assert!(!rendered.contains(&secret.expose()));
    }

    #[test]
    fn the_digest_is_not_the_secret() {
        // AN EARLIER VERSION OF THIS TEST WAS VACUOUS and passed against a `SecretDigest::of` that
        // returned its input verbatim: it compared against the HEX rendering while the digest's
        // Debug printed DECIMAL bytes, so the two could never match whatever the code did.
        //
        // It now compares the actual bytes, which is the thing that must differ.
        let secret = fixture();
        let digest = SecretDigest::of(&secret);
        let mut secret_bytes = [0_u8; OPAQUE_BYTES];
        let exposed = secret.expose();
        let (pairs, _) = exposed.as_bytes().as_chunks::<2>();
        for (index, pair) in pairs.iter().enumerate() {
            secret_bytes[index] =
                u8::from_str_radix(std::str::from_utf8(pair).expect("hex is ascii"), 16)
                    .expect("hex parses");
        }
        // `assert_ne!` prints both operands on failure, and in the failing case they are the
        // raw credential bytes. `assert!` reports the fault without reproducing them.
        assert!(
            digest.as_bytes() != &secret_bytes,
            "the digest IS the secret; storing it would put a usable credential in the database"
        );
    }

    #[test]
    fn a_digest_matches_only_the_credential_it_came_from() {
        let secret = fixture();
        let digest = SecretDigest::of(&secret);

        let presented = Opaque::from_wire(OpaqueKind::Session, &secret.expose())
            .expect("the wire form round-trips");
        assert!(
            digest.matches(&SecretDigest::of(&presented)),
            "the credential the caller presented must verify"
        );

        let other = Opaque::generate(
            OpaqueKind::Session,
            &FixedEntropy::new(vec![0x01, 0x02, 0x03, 0x04]),
        )
        .expect("fixed entropy always succeeds");
        assert!(
            !digest.matches(&SecretDigest::of(&other)),
            "a different credential must not verify"
        );
    }

    #[test]
    fn the_kind_is_bound_into_the_digest() {
        // A token lifted from the verification table must not verify against the reset table, even
        // if the bytes were somehow identical. One purpose per kind (FR-053).
        let source = FixedEntropy::new(vec![0xAB, 0xCD, 0xEF, 0x12]);
        let verification = Opaque::generate(OpaqueKind::Verification, &source).expect("generated");
        let reset = Opaque::generate(OpaqueKind::PasswordReset, &source).expect("generated");

        assert_eq!(
            verification.expose(),
            reset.expose(),
            "the fixture must produce identical BYTES, or this test proves nothing"
        );
        assert!(
            !SecretDigest::of(&verification).matches(&SecretDigest::of(&reset)),
            "identical bytes under different kinds must not share a digest"
        );
    }

    #[test]
    fn generation_is_a_pure_function_of_the_supplied_entropy() {
        // Nothing but the entropy reaches the value, so no clock, hostname, or counter can leak
        // into it — and a session identifier therefore encodes nothing forgeable.
        let a = fixture();
        let b = fixture();
        assert_eq!(
            a.expose(),
            b.expose(),
            "identical entropy must yield an identical credential"
        );

        let different = Opaque::generate(
            OpaqueKind::Session,
            &FixedEntropy::new(vec![0x01, 0x02, 0x03, 0x04]),
        )
        .expect("fixed entropy always succeeds");
        assert_ne!(
            a.expose(),
            different.expose(),
            "different entropy must differ"
        );
    }

    #[test]
    fn a_malformed_credential_is_refused_before_it_reaches_a_query() {
        assert!(
            Opaque::from_wire(OpaqueKind::Session, "").is_none(),
            "empty"
        );
        assert!(
            Opaque::from_wire(OpaqueKind::Session, "abc").is_none(),
            "too short"
        );
        assert!(
            Opaque::from_wire(OpaqueKind::Session, &"a".repeat(OPAQUE_LEN + 1)).is_none(),
            "too long"
        );
        assert!(
            Opaque::from_wire(OpaqueKind::Session, &"g".repeat(OPAQUE_LEN)).is_none(),
            "not hex"
        );
        assert!(
            Opaque::from_wire(OpaqueKind::Session, &"A".repeat(OPAQUE_LEN)).is_none(),
            "uppercase is not the canonical form"
        );
        // POSITIVE CONTROL: a well-formed one IS accepted, so the refusals above are about the
        // input rather than about a parser that refuses everything.
        assert!(
            Opaque::from_wire(OpaqueKind::Session, &fixture().expose()).is_some(),
            "the canonical form must be accepted"
        );
    }
}
