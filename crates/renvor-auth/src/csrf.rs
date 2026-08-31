//! CSRF protection: OWASP's signed double-submit cookie, bound to the session.
//!
//! # The construction is quoted, not invented
//!
//! OWASP's *Cross-Site Request Forgery Prevention Cheat Sheet*, "Signed Double-Submit Cookie":
//!
//! > `message = sessionID.length + "!" + sessionID + "!" + randomValue.length + "!" +
//! > randomValue.toHex()`
//!
//! > `csrfToken = hmac.toHex() + "." + randomValue.toHex()`
//!
//! and on binding: *"which explicitly ties tokens to the user's authenticated session (e.g.,
//! session ID)"*.
//!
//! The delimiters are not decoration. Without them `12` + `34` and `1` + `234` produce the same
//! message, so a token minted for one session could verify against another. This module reproduces
//! the format literally rather than substituting a binary length prefix, because a construction
//! copied from a primary source is reviewable against that source.
//!
//! # It is the **digest** that is bound, never the session secret
//!
//! OWASP writes `sessionID`; this uses [`SecretDigest`] — the value the server already stores in
//! the session's place. Binding to the raw identifier would mean the CSRF code handled the live
//! session secret, and a token is a value that gets copied into forms, headers, and logs. The
//! digest gives the same uniqueness with nothing to lose.
//!
//! # Rotation is automatic, and that is the point
//!
//! FR-030 requires CSRF tokens to rotate on the events that rotate a session. Because the MAC is
//! taken over the session's digest, **rotating the session invalidates every token bound to the
//! old one** — there is no second mechanism to remember to call, and no window in which a rotated
//! session still accepts its predecessor's tokens.
//!
//! # What is constant-time here, and what is not
//!
//! Exactly one comparison is constant-time: [`Mac::verify_slice`], which compares the recomputed
//! MAC with the presented one. **Nothing else in this module is**, and none of it is claimed to
//! be — the hex decode, the length checks, and the split on `.` all return as soon as they can.
//! They are checks on a value an attacker already holds, and the secret they protect is the key,
//! which no timing on those paths reveals.
//!
//! # `Origin` is not made unnecessary by this
//!
//! A common overstatement is that signing removes the need for `Origin`/`Referer` validation.
//! OWASP does not say that: it recommends origin verification as a **separate** defence-in-depth
//! layer. What is true is narrower and is all this module claims — the security property here does
//! not *depend* on an `Origin` check. The check itself belongs in the transport adapter, which is
//! the only layer that sees the header.

use hmac::digest::KeyInit as _;
use hmac::{Hmac, Mac};
use renvor_config::Secret;
use renvor_core::observe::entropy::EntropySource;
use sha2::Sha256;

use crate::error::AuthError;
use crate::opaque::SecretDigest;

/// The MAC this module uses. SHA-256, paired with the `sha2` already in this crate.
type HmacSha256 = Hmac<Sha256>;

/// How many bytes of the random half a token carries.
const RANDOM_BYTES: usize = 32;

/// The signing key for CSRF tokens.
///
/// Wraps [`Secret`], so it is redacted in `Debug` **and** `Display`, zeroized on drop, and has no
/// `Serialize` — serialising one is a compile error rather than a promise not to.
pub struct CsrfKey(Secret<[u8; 32]>);

impl CsrfKey {
    /// Generates a key from the entropy port.
    ///
    /// # Errors
    ///
    /// [`AuthError::EntropyUnavailable`]. **There is no fallback**; a CSRF key from a weak source
    /// is a forgeable one.
    pub fn generate(source: &dyn EntropySource) -> Result<Self, AuthError> {
        let mut bytes = [0_u8; 32];
        source
            .fill(&mut bytes)
            .map_err(|_| AuthError::EntropyUnavailable)?;
        Ok(Self(Secret::new("csrf-signing-key", bytes)))
    }

    /// Rebuilds a key an operator supplied, so a fleet of processes signs alike.
    ///
    /// A single process generating its own key would invalidate every outstanding token whenever
    /// it restarted, and every token minted by a sibling.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Secret::new("csrf-signing-key", bytes))
    }
}

impl core::fmt::Debug for CsrfKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("CsrfKey([redacted])")
    }
}

/// A CSRF token: `hmac_hex "." random_hex`, exactly OWASP's `csrfToken`.
///
/// Not a session secret, but holding one lets its bearer act on the session it is bound to, so it
/// is redacted for the same reason [`crate::Opaque`] is.
#[derive(Clone)]
pub struct CsrfToken(String);

impl CsrfToken {
    /// The wire form, handed over **once** and conspicuously.
    #[must_use]
    pub fn expose(self) -> String {
        self.0
    }
}

impl core::fmt::Debug for CsrfToken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("CsrfToken([redacted])")
    }
}

/// Why a CSRF check failed.
///
/// **Operator-facing.** FR-034 requires a CSRF failure be indistinguishable to a requester from an
/// authorization failure, so none of this reaches a response body; the caller maps every variant
/// to the same refusal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum CsrfRejection {
    /// No token was presented.
    Absent,
    /// More than one was. Choosing among them would be choosing arbitrarily.
    Duplicated,
    /// The token was not `hmac_hex "." random_hex` with both halves 64 lowercase hex characters.
    Malformed,
    /// The token was well-formed and its MAC did not verify — a different session, a different
    /// key, or a forgery. **One variant for all three**, because which it was tells an attacker
    /// which half of the construction to attack next.
    Mismatched,
}

/// What authenticated a request.
///
/// # Why the CSRF requirement keys on this and not on the presence of a cookie
///
/// FR-033: a bearer token is not attached by the browser automatically, so a cross-site page
/// cannot cause one to be sent — which is the entire mechanism CSRF depends on. **This is
/// engineering judgement, not a citable requirement.** OWASP's cheat sheet does not discuss
/// non-cookie authentication at all, and NIST §5.1 states a blanket rule with no carve-out. It is
/// documented as reasoning and never cited to a standard.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum Credential {
    /// A cookie the user agent attached on its own. **CSRF applies.**
    Cookie,
    /// A bearer token the caller placed deliberately. CSRF does not apply.
    BearerToken,
}

/// Whether a request changes state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RequestKind {
    /// Defined as safe by RFC 9110 §9.2.1.
    Safe,
    /// Everything else — **including a method this crate does not recognise**.
    StateChanging,
}

/// Classifies an HTTP method.
///
/// # Unrecognised methods are state-changing, and the comparison is case-sensitive
///
/// RFC 9110 §9.1: *"The method token is case-sensitive."* So `get` is not `GET`; it is a method
/// this function does not recognise, and an unrecognised method is treated as state-changing.
/// Both halves are deliberate — the failure a lenient classifier produces is an unprotected
/// state-changing request, which is the thing the module exists to prevent.
#[must_use]
pub fn classify(method: &str) -> RequestKind {
    match method {
        "GET" | "HEAD" | "OPTIONS" | "TRACE" => RequestKind::Safe,
        _ => RequestKind::StateChanging,
    }
}

/// Whether a request must carry a CSRF token.
///
/// FR-028's set — `POST`, `PUT`, `PATCH`, `DELETE` — falls out of [`classify`] rather than being
/// listed again, so a fifth unsafe method is covered the day it exists.
#[must_use]
pub fn is_required(method: &str, credential: Credential) -> bool {
    matches!(credential, Credential::Cookie) && classify(method) == RequestKind::StateChanging
}

/// Builds the message OWASP specifies, from the two hex halves.
///
/// `sessionID.length + "!" + sessionID + "!" + randomValue.length + "!" + randomValue.toHex()`.
/// The delimiters carry the security property: without them `12` + `34` and `1` + `234` are the
/// same message, and a token minted for one session would verify against another.
fn message(session_hex: &str, random_hex: &str) -> String {
    format!(
        "{}!{}!{}!{}",
        session_hex.len(),
        session_hex,
        random_hex.len(),
        random_hex
    )
}

/// Renders bytes as lowercase hex.
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    out
}

/// Decodes exactly `N` bytes of lowercase hex.
fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 {
        return None;
    }
    let mut out = [0_u8; N];
    for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        let value = |byte: u8| match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        };
        out[index] = (value(pair[0])? << 4) | value(pair[1])?;
    }
    Some(out)
}

/// Computes the MAC over the session/random pair.
fn sign(key: &CsrfKey, session_hex: &str, random_hex: &str) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key.0.expose().as_slice())
        .expect("HMAC accepts a key of any length");
    mac.update(message(session_hex, random_hex).as_bytes());
    mac.finalize().into_bytes().into()
}

/// Mints a token bound to `session`.
///
/// # Errors
///
/// [`AuthError::EntropyUnavailable`].
pub fn issue(
    key: &CsrfKey,
    session: &SecretDigest,
    source: &dyn EntropySource,
) -> Result<CsrfToken, AuthError> {
    let mut random = [0_u8; RANDOM_BYTES];
    source
        .fill(&mut random)
        .map_err(|_| AuthError::EntropyUnavailable)?;
    let session_hex = hex(session.as_bytes());
    let random_hex = hex(&random);
    let mac = sign(key, &session_hex, &random_hex);
    Ok(CsrfToken(format!("{}.{}", hex(&mac), random_hex)))
}

/// Checks the tokens a request presented against the session it authenticated as.
///
/// `presented` is every value the request carried under the CSRF header or field — a slice rather
/// than an `Option<&str>` so *duplicated* is a case the signature forces the caller to have
/// collected, instead of one a transport silently resolves by taking the first.
///
/// # Errors
///
/// [`CsrfRejection`]. Only the MAC comparison is constant-time; see the module header for what is
/// and is not claimed.
pub fn verify(
    key: &CsrfKey,
    session: &SecretDigest,
    presented: &[&str],
) -> Result<(), CsrfRejection> {
    let [token] = presented else {
        return Err(if presented.is_empty() {
            CsrfRejection::Absent
        } else {
            CsrfRejection::Duplicated
        });
    };
    let Some((mac_hex, random_hex)) = token.split_once('.') else {
        return Err(CsrfRejection::Malformed);
    };
    let Some(presented_mac) = unhex::<32>(mac_hex) else {
        return Err(CsrfRejection::Malformed);
    };
    if unhex::<RANDOM_BYTES>(random_hex).is_none() {
        return Err(CsrfRejection::Malformed);
    }

    let mut mac = HmacSha256::new_from_slice(key.0.expose().as_slice())
        .expect("HMAC accepts a key of any length");
    mac.update(message(&hex(session.as_bytes()), random_hex).as_bytes());
    // THE constant-time comparison. `verify_slice` is the crate's own equality, not `==` on the
    // digest bytes, which would leak the first differing byte's position.
    mac.verify_slice(&presented_mac)
        .map_err(|_| CsrfRejection::Mismatched)
}

#[cfg(test)]
mod tests {
    use super::{
        Credential, CsrfKey, CsrfRejection, RequestKind, classify, is_required, issue, message,
        verify,
    };
    use crate::opaque::{Opaque, OpaqueKind, SecretDigest};

    fn entropy() -> renvor_core::observe::entropy::OsEntropy {
        renvor_core::observe::entropy::OsEntropy::new()
    }

    fn key() -> CsrfKey {
        CsrfKey::from_bytes([17; 32])
    }

    fn session(seed: u8) -> SecretDigest {
        let hex: String = core::iter::repeat_n(format!("{seed:02x}"), 32).collect();
        SecretDigest::of(&Opaque::from_wire(OpaqueKind::Session, &hex).expect("valid hex"))
    }

    fn mint(key: &CsrfKey, session: &SecretDigest) -> String {
        issue(key, session, &entropy()).expect("entropy").expose()
    }

    // ---- the property the whole design exists for ------------------------------------------

    #[test]
    fn a_token_verifies_for_the_session_it_was_minted_for() {
        let token = mint(&key(), &session(1));
        verify(&key(), &session(1), &[&token]).expect("the positive control must pass");
    }

    #[test]
    fn a_token_minted_for_another_session_is_refused() {
        let token = mint(&key(), &session(1));
        assert_eq!(
            verify(&key(), &session(2), &[&token]).expect_err("refused"),
            CsrfRejection::Mismatched
        );
    }

    #[test]
    fn a_token_signed_with_another_key_is_refused() {
        // A different deployment, or an attacker who guessed the construction but not the key.
        let token = mint(&CsrfKey::from_bytes([99; 32]), &session(1));
        assert_eq!(
            verify(&key(), &session(1), &[&token]).expect_err("refused"),
            CsrfRejection::Mismatched
        );
    }

    #[test]
    fn rotating_the_session_invalidates_every_token_bound_to_it() {
        // FR-030, and it needs no second mechanism: the MAC covers the session's digest, so a new
        // session digest is a new binding and every outstanding token stops verifying at once.
        let token = mint(&key(), &session(1));
        let rotated = session(2);
        assert_eq!(
            verify(&key(), &rotated, &[&token]).expect_err("refused"),
            CsrfRejection::Mismatched
        );
    }

    #[test]
    fn a_token_reused_within_its_own_session_still_verifies() {
        // STATED RATHER THAN ASSUMED: these are not single-use. A signed double-submit token is
        // valid for the life of its binding; per-request tokens are a different design that breaks
        // parallel requests and the back button. What must never be replayable is a token across
        // SESSIONS, which the test above covers.
        let token = mint(&key(), &session(1));
        for _ in 0..3 {
            verify(&key(), &session(1), &[&token]).expect("still valid");
        }
    }

    #[test]
    fn two_tokens_for_one_session_differ() {
        assert_ne!(mint(&key(), &session(1)), mint(&key(), &session(1)));
    }

    // ---- the five rejection cases ------------------------------------------------------------

    #[test]
    fn no_token_at_all_is_absent() {
        assert_eq!(
            verify(&key(), &session(1), &[]).expect_err("refused"),
            CsrfRejection::Absent
        );
    }

    #[test]
    fn two_tokens_are_refused_rather_than_resolved() {
        let (a, b) = (mint(&key(), &session(1)), mint(&key(), &session(1)));
        assert_eq!(
            verify(&key(), &session(1), &[&a, &b]).expect_err("refused"),
            CsrfRejection::Duplicated
        );
    }

    #[test]
    fn a_duplicate_is_refused_even_when_both_copies_are_valid() {
        // The negative control for the case above: "duplicated" is not a stand-in for "one of them
        // was wrong". Two identical, individually valid tokens are still ambiguous input.
        let token = mint(&key(), &session(1));
        assert_eq!(
            verify(&key(), &session(1), &[&token, &token]).expect_err("refused"),
            CsrfRejection::Duplicated
        );
    }

    #[test]
    fn a_token_without_the_separator_is_malformed() {
        assert_eq!(
            verify(&key(), &session(1), &["deadbeef"]).expect_err("refused"),
            CsrfRejection::Malformed
        );
    }

    #[test]
    fn a_token_with_a_truncated_mac_is_malformed() {
        let token = mint(&key(), &session(1));
        let (mac, random) = token.split_once('.').expect("well formed");
        let truncated = format!("{}.{random}", &mac[..62]);
        assert_eq!(
            verify(&key(), &session(1), &[&truncated]).expect_err("refused"),
            CsrfRejection::Malformed
        );
    }

    #[test]
    fn a_token_with_a_non_hex_random_half_is_malformed() {
        let token = mint(&key(), &session(1));
        let (mac, _) = token.split_once('.').expect("well formed");
        let tampered = format!("{mac}.{}", "z".repeat(64));
        assert_eq!(
            verify(&key(), &session(1), &[&tampered]).expect_err("refused"),
            CsrfRejection::Malformed
        );
    }

    #[test]
    fn a_token_whose_random_half_was_swapped_is_refused() {
        // The forgery attempt that matters: keep a valid MAC, change what it covers.
        let mine = mint(&key(), &session(1));
        let theirs = mint(&key(), &session(1));
        let mac = mine.split_once('.').expect("well formed").0;
        let random = theirs.split_once('.').expect("well formed").1;
        assert_eq!(
            verify(&key(), &session(1), &[&format!("{mac}.{random}")]).expect_err("refused"),
            CsrfRejection::Mismatched
        );
    }

    #[test]
    fn the_rejection_carries_nothing_from_the_rejected_token() {
        let rejected = verify(&key(), &session(1), &["zzzz"]).expect_err("refused");
        assert!(!format!("{rejected:?}").contains('z'), "{rejected:?}");
    }

    // ---- the construction --------------------------------------------------------------------

    #[test]
    fn the_delimiters_keep_the_message_unambiguous() {
        // OWASP's format is length-delimited so two different (session, random) pairs cannot
        // produce one message. With today's fixed 64-character halves a collision is impossible
        // anyway — this asserts the property of the CONSTRUCTION, so that if either half ever
        // becomes variable-length the guarantee is already there and already tested.
        assert_ne!(message("ab", "cd"), message("abc", "d"));
        assert_ne!(message("a", "bcd"), message("abc", "d"));
    }

    #[test]
    fn the_message_names_the_lengths_before_the_values() {
        assert_eq!(message("ab", "cdef"), "2!ab!4!cdef");
    }

    // ---- when protection applies -------------------------------------------------------------

    #[test]
    fn every_state_changing_cookie_method_is_protected() {
        for method in ["POST", "PUT", "PATCH", "DELETE"] {
            assert!(
                is_required(method, Credential::Cookie),
                "{method} was left unprotected"
            );
        }
    }

    #[test]
    fn safe_methods_are_not_protected() {
        for method in ["GET", "HEAD", "OPTIONS", "TRACE"] {
            assert!(!is_required(method, Credential::Cookie), "{method}");
            assert_eq!(classify(method), RequestKind::Safe);
        }
    }

    #[test]
    fn a_method_this_crate_does_not_know_is_treated_as_state_changing() {
        // Fail closed. The failure a lenient classifier produces is an unprotected write.
        // The lengths matter as much as the names: F-M31 survived a first run because every
        // method here was six characters or fewer, so a classifier keyed on LENGTH slipped past.
        // `PROPPATCH`, `MKCALENDAR` and `CONNECT` are all real, all longer, and all write.
        for method in [
            "QUERY",
            "LOCK",
            "",
            "post ",
            "CONNECT",
            "PROPPATCH",
            "MKCALENDAR",
            "VERSION-CONTROL",
        ] {
            assert_eq!(
                classify(method),
                RequestKind::StateChanging,
                "{method:?} was classified as safe"
            );
        }
    }

    #[test]
    fn a_lower_case_method_is_not_recognised_as_safe() {
        // RFC 9110 §9.1: "The method token is case-sensitive." `get` is not `GET`.
        assert_eq!(classify("get"), RequestKind::StateChanging);
    }

    #[test]
    fn a_bearer_token_request_is_not_asked_for_a_csrf_token() {
        for method in ["POST", "PUT", "PATCH", "DELETE", "GET"] {
            assert!(
                !is_required(method, Credential::BearerToken),
                "{method} demanded CSRF on a non-cookie credential"
            );
        }
    }

    // ---- what is never exposed ---------------------------------------------------------------

    #[test]
    fn the_key_reveals_nothing_through_debug() {
        let rendered = format!("{:?}", CsrfKey::from_bytes([0xab; 32]));
        assert!(!rendered.contains("ab"), "Debug rendered the key material");
        assert!(
            rendered.contains("[redacted]"),
            "Debug omitted the redaction placeholder"
        );
    }

    #[test]
    fn the_token_reveals_nothing_through_debug() {
        let token = issue(&key(), &session(1), &entropy()).expect("entropy");
        let rendered = format!("{token:?}");
        assert!(
            !rendered.contains('.'),
            "Debug rendered the token's delimiter, and so its parts"
        );
        assert!(
            rendered.contains("[redacted]"),
            "Debug omitted the redaction placeholder"
        );
    }
}
