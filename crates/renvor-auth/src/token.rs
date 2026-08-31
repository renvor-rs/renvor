//! API access tokens: signed JWTs whose verifier chooses the algorithm.
//!
//! Behind the `tokens` feature (FR-035). A build without it resolves neither `jsonwebtoken` nor
//! `aws-lc-rs`, which `xtask` proves from the dependency graph rather than from this flag.
//!
//! # The whole design is one sentence
//!
//! **The verifier decides everything; the token decides nothing.** A JWT arrives carrying its own
//! opinion about which algorithm should check it, which key should be used, and — through `jku`,
//! `x5u`, and an embedded `jwk` — where that key might be fetched from. Every one of those
//! opinions is either compared against a local configuration or refused outright. None of them
//! selects anything.
//!
//! RFC 8725 §3.1 states the rule in a form worth quoting because it is structural, not procedural:
//! *"each key MUST be used with exactly one algorithm, and this MUST be checked when the
//! cryptographic operation is performed."* [`VerifyingKey`](crate::token::VerifyingKey) therefore
//! **owns** its algorithm in a
//! private field set at construction. There is no function in this module that accepts a key and an
//! algorithm as two arguments, so the pairing cannot be got wrong at a call site.
//!
//! # What is deliberately not exposed
//!
//! [`jsonwebtoken::Validation`] never appears in a public signature, and neither does any
//! caller-supplied algorithm collection. `Validation.algorithms` is a `Vec<Algorithm>`, so a token's
//! `alg` selects from among whatever the caller listed — a rule reviewers enforce. Here the vector
//! is built internally and always has exactly one element, taken from the key the `kid` selected.
//! `jsonwebtoken` is an implementation detail of this crate; swapping it would move nothing public.
//!
//! # Time comes from the [`Clock`], not from the operating system
//!
//! `jsonwebtoken` validates `exp` and `nbf` against the system clock, which no test can control and
//! no operator can bound. Both of its time checks are therefore switched **off**, and every
//! comparison in this module is made against the injected [`Clock`] with a bounded
//! [`Skew`](crate::token::Skew). This is
//! why the expiry tests are deterministic rather than timing-dependent.
//!
//! # For callers of the issued token
//!
//! **An access token is an opaque bearer string.** Its claim set is Renvor's to change; a client
//! that decodes it and depends on what it finds is depending on an internal representation. The
//! claims carry **no personal data** — an issuer, an audience, an opaque subject identifier, two
//! instants, a token identifier, and a scope string — and nothing may be added that does.

use std::collections::{BTreeSet, HashSet};
use std::fmt;

use chrono::{DateTime, Duration, Utc};
use renvor_core::observe::entropy::EntropySource;

use crate::clock::Clock;
use crate::error::AuthError;
use crate::subject::{AuthenticatedSubject, UserId};

/// The placeholder every redacted rendering in this module uses.
const REDACTED: &str = "[redacted]";

/// The `typ` header this crate issues and the only one it accepts.
///
/// # Why this is not `at+jwt`
///
/// RFC 9068 defines `at+jwt` for an **OAuth 2.0 authorization server**: it requires a `client_id`
/// claim, mandates RS256 support, and fixes claim semantics that assume an OAuth deployment Renvor
/// does not impose on its users. Stamping `at+jwt` on a token that does not implement the profile
/// would advertise a conformance that does not exist, and inventing a `client_id` to reach it would
/// be fabricating a claim to satisfy a label. RFC 7519, RFC 8725 and RFC 9068 informed this design;
/// only the first two are *implemented*, so only a Renvor-specific, collision-resistant type is
/// claimed.
///
/// RFC 8725 §3.11 asks for explicit typing precisely so that one kind of token cannot be replayed
/// as another. A private type name serves that better than a shared one.
pub const ACCESS_TOKEN_TYP: &str = "renvor-access+jwt";

/// The largest number of verifying keys one ring may hold.
///
/// A ring exists so that a key can be rotated without a restart, which needs the old key and the
/// new one — plus room for an overlap. It is **bounded** because an unbounded ring turns `kid` into
/// a lookup key an attacker can make expensive.
pub const MAX_RING_KEYS: usize = 8;

/// The largest clock skew an operator may configure: **five minutes**.
///
/// Skew exists for imperfectly synchronised machines, not for extending a token's life. Five
/// minutes is Renvor's ceiling, and this is stated as Renvor's choice: no primary source verified
/// for this phase fixes a numeric maximum, and inventing a citation for a number would be worse
/// than owning it.
pub const MAX_SKEW: Duration = Duration::minutes(5);

/// The longest an access token may be configured to live: **one hour**.
///
/// FR-036 requires "short-lived" without a number, and — checked rather than recalled — neither
/// NIST SP 800-63B-4 nor the ASVS sections cited by this phase states one for an access token. The
/// ceiling is therefore Renvor's, chosen so that a stolen token expires within the working hour and
/// a refresh is required to continue. Like [`crate::service::TokenLifetime`], a configuration above
/// it is **refused rather than clamped**: clamping would let an operator believe they had
/// configured something they had not.
pub const MAX_ACCESS_LIFETIME: Duration = Duration::hours(1);

// ---------------------------------------------------------------------------------------------
// Algorithms and keys
// ---------------------------------------------------------------------------------------------

/// A signature algorithm this crate supports.
///
/// # Only asymmetric algorithms exist here, and that is the point
///
/// There is no `HS256` variant, so no code path in this crate can verify an HMAC-signed token —
/// not because a check rejects it, but because the value cannot be constructed. A shared MAC
/// secret would let every party that can *verify* a token also *mint* one, and RFC 8725 §3.5
/// forbids deriving such a key from a human-memorizable string, which is the shape a `JWT_SECRET`
/// environment variable takes in practice.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
#[non_exhaustive]
pub enum TokenAlgorithm {
    /// EdDSA over Curve25519 (RFC 8037).
    Ed25519,
    /// ECDSA over NIST P-256 with SHA-256 (RFC 7518 `ES256`).
    Es256,
}

impl TokenAlgorithm {
    /// The `alg` header value, as this algorithm is named on the wire.
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Ed25519 => "EdDSA",
            Self::Es256 => "ES256",
        }
    }

    /// The backend's enumeration. Private: it is `jsonwebtoken`'s type, not ours.
    const fn backend(self) -> jsonwebtoken::Algorithm {
        match self {
            Self::Ed25519 => jsonwebtoken::Algorithm::EdDSA,
            Self::Es256 => jsonwebtoken::Algorithm::ES256,
        }
    }
}

impl fmt::Display for TokenAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire_name())
    }
}

/// The identifier a `kid` header must match to select a key from the ring.
///
/// A key identifier is **not** a secret — it travels in the clear in every token header — so it
/// renders in full. It is validated on construction so that a `kid` from a token can be compared
/// with it as a plain string, with no normalisation step in between that could make two different
/// identifiers compare equal.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct KeyId(String);

impl KeyId {
    /// The longest identifier accepted.
    pub const MAX_LEN: usize = 64;

    /// Builds a key identifier.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when `id` is empty, longer than [`Self::MAX_LEN`], or
    /// contains anything outside `[A-Za-z0-9._-]`. The charset is restricted so that an identifier
    /// is unambiguous in a JSON header and in a log line, and so that no escaping decision ever
    /// stands between the `kid` on the wire and the `kid` in the ring.
    pub fn new(id: &str) -> Result<Self, AuthError> {
        let acceptable = !id.is_empty()
            && id.len() <= Self::MAX_LEN
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if acceptable {
            Ok(Self(id.to_string()))
        } else {
            Err(AuthError::PolicyMisconfigured)
        }
    }

    /// The identifier as it appears in a `kid` header.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for KeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A key that signs access tokens, permanently bound to one algorithm.
///
/// `algorithm` is private and is set from the key material's own type at construction. Nothing can
/// change it afterwards, so RFC 8725 §3.1's "one key, one algorithm" is a property of the value
/// rather than a rule a caller must remember.
pub struct SigningKey {
    id: KeyId,
    algorithm: TokenAlgorithm,
    inner: jsonwebtoken::EncodingKey,
}

impl SigningKey {
    /// The identifier stamped into the `kid` header of every token this key signs.
    #[must_use]
    pub const fn id(&self) -> &KeyId {
        &self.id
    }

    /// The algorithm this key is bound to.
    #[must_use]
    pub const fn algorithm(&self) -> TokenAlgorithm {
        self.algorithm
    }
}

impl fmt::Debug for SigningKey {
    /// Names the key and its algorithm. **The key material is never rendered** — a private signing
    /// key in a debug line is the whole issuing authority in a debug line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SigningKey({}, {}, {REDACTED})", self.id, self.algorithm)
    }
}

/// A key that verifies access tokens, permanently bound to one algorithm.
///
/// A public key is not a secret, but it is still not rendered: a `Debug` that printed key material
/// invites the habit of printing key material, and the habit is what leaks the private half.
#[derive(Clone)]
pub struct VerifyingKey {
    id: KeyId,
    algorithm: TokenAlgorithm,
    inner: jsonwebtoken::DecodingKey,
}

impl VerifyingKey {
    /// Builds a verifying key from a raw Ed25519 public key (32 bytes).
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when `public_key` is not 32 bytes.
    pub fn ed25519(id: KeyId, public_key: &[u8]) -> Result<Self, AuthError> {
        const ED25519_PUBLIC_LEN: usize = 32;
        if public_key.len() != ED25519_PUBLIC_LEN {
            return Err(AuthError::PolicyMisconfigured);
        }
        Ok(Self {
            id,
            algorithm: TokenAlgorithm::Ed25519,
            inner: jsonwebtoken::DecodingKey::from_ed_der(public_key),
        })
    }

    /// Builds a verifying key from an uncompressed P-256 public point (65 bytes, `0x04`-prefixed).
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when `public_key` is not a 65-byte uncompressed point.
    pub fn es256(id: KeyId, public_key: &[u8]) -> Result<Self, AuthError> {
        const P256_UNCOMPRESSED_LEN: usize = 65;
        const UNCOMPRESSED_TAG: u8 = 0x04;
        if public_key.len() != P256_UNCOMPRESSED_LEN || public_key[0] != UNCOMPRESSED_TAG {
            return Err(AuthError::PolicyMisconfigured);
        }
        Ok(Self {
            id,
            algorithm: TokenAlgorithm::Es256,
            inner: jsonwebtoken::DecodingKey::from_ec_der(public_key),
        })
    }

    /// The identifier a `kid` must match to select this key.
    #[must_use]
    pub const fn id(&self) -> &KeyId {
        &self.id
    }

    /// The algorithm this key is bound to.
    #[must_use]
    pub const fn algorithm(&self) -> TokenAlgorithm {
        self.algorithm
    }
}

impl fmt::Debug for VerifyingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "VerifyingKey({}, {}, {REDACTED})",
            self.id, self.algorithm
        )
    }
}

/// A freshly generated key pair.
///
/// # Why generation lives here rather than in a committed fixture
///
/// Committing a private key to a repository — even a "test only" one — puts key material in every
/// clone, every fork, and every search index, and makes a hard-coded cryptographic value the normal
/// way to obtain a key. Generating instead costs one already-present dependency: `aws-lc-rs` is
/// `jsonwebtoken`'s selected backend, so this adds **an edge and no package**.
#[derive(Debug)]
pub struct GeneratedKeyPair {
    /// The half that signs.
    pub signing: SigningKey,
    /// The half that verifies.
    pub verifying: VerifyingKey,
}

/// Generates an Ed25519 key pair bound to `id`, from **this crate's entropy port**.
///
/// # The randomness comes from the same single site as everything else
///
/// `aws-lc-rs` offers its own generator, and using it would have been shorter. It is not used,
/// because this crate's header states that [`EntropySource`] is the **only** source of randomness
/// here, and a signing key generated from a second source would quietly make that false. The seed
/// is drawn from the port and expanded by `Ed25519KeyPair::from_seed_unchecked`, which is the
/// documented construction for a caller-supplied seed.
///
/// # A limitation stated rather than implied
///
/// The 32-byte seed lives on the stack and is **not** zeroized on drop. That matches
/// [`crate::opaque::Opaque`], which holds 32 secret bytes under the same terms; this module does not
/// quietly introduce a stronger guarantee for one secret than the crate gives the others. It is
/// recorded here so the gap is a known one.
///
/// # Errors
///
/// [`AuthError::EntropyUnavailable`] when the platform CSPRNG fails, or when the backend refuses
/// the seed. There is **no fallback**: a key generated from a degraded source is worse than no key,
/// because it looks like a key.
pub fn generate_ed25519(
    id: KeyId,
    entropy: &dyn EntropySource,
) -> Result<GeneratedKeyPair, AuthError> {
    use aws_lc_rs::signature::KeyPair as _;

    const ED25519_SEED_LEN: usize = 32;
    let mut seed = [0_u8; ED25519_SEED_LEN];
    entropy
        .fill(&mut seed)
        .map_err(|_| AuthError::EntropyUnavailable)?;

    let pair = aws_lc_rs::signature::Ed25519KeyPair::from_seed_unchecked(&seed)
        .map_err(|_| AuthError::EntropyUnavailable)?;
    let public = pair.public_key().as_ref().to_vec();
    let document = pair.to_pkcs8().map_err(|_| AuthError::EntropyUnavailable)?;

    let generated = GeneratedKeyPair {
        signing: SigningKey {
            id: id.clone(),
            algorithm: TokenAlgorithm::Ed25519,
            inner: jsonwebtoken::EncodingKey::from_ed_der(document.as_ref()),
        },
        verifying: VerifyingKey::ed25519(id, &public)?,
    };
    seed.fill(0);
    Ok(generated)
}

impl SigningKey {
    /// Builds an Ed25519 signing key from operator-supplied PKCS#8 DER.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when the DER is not a usable Ed25519 private key. The
    /// key material is validated **here**, at configuration time, rather than at the first signing
    /// attempt — a misconfigured key should fail a deployment, not a request.
    pub fn ed25519_pkcs8(id: KeyId, pkcs8_der: &[u8]) -> Result<Self, AuthError> {
        aws_lc_rs::signature::Ed25519KeyPair::from_pkcs8(pkcs8_der)
            .map_err(|_| AuthError::PolicyMisconfigured)?;
        Ok(Self {
            id,
            algorithm: TokenAlgorithm::Ed25519,
            inner: jsonwebtoken::EncodingKey::from_ed_der(pkcs8_der),
        })
    }

    /// Builds a P-256 signing key from operator-supplied PKCS#8 DER.
    ///
    /// There is deliberately **no P-256 generator**. A P-256 private key is a scalar in a range
    /// that raw entropy does not satisfy without rejection sampling, and hand-rolling that on top
    /// of the entropy port would be writing key generation twice — once carefully, once not. An
    /// operator bringing a P-256 key from a KMS or from `openssl` is the realistic case, and it is
    /// the one this constructor serves.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when the DER is not a usable P-256 private key.
    pub fn es256_pkcs8(id: KeyId, pkcs8_der: &[u8]) -> Result<Self, AuthError> {
        aws_lc_rs::signature::EcdsaKeyPair::from_pkcs8(
            &aws_lc_rs::signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            pkcs8_der,
        )
        .map_err(|_| AuthError::PolicyMisconfigured)?;
        Ok(Self {
            id,
            algorithm: TokenAlgorithm::Es256,
            inner: jsonwebtoken::EncodingKey::from_ec_der(pkcs8_der),
        })
    }
}

/// The bounded, entirely local set of keys a verifier may select from.
///
/// # `kid` selects from here, or the token is rejected
///
/// There is no path that fetches a key. `jku` and `x5u` are URLs the token asks the verifier to
/// dereference, and `jwk` is a key the token asks it to trust; ASVS **V9.1.3 (L1)** covers all
/// three, where RFC 8725 §3.10 omits `jwk` and would leave the embedded-key path open. This module
/// **rejects** a token carrying any of them rather than ignoring them: ignoring an instruction to
/// fetch an attacker's key still accepts the token that carried it.
#[derive(Clone, Debug)]
pub struct KeyRing {
    keys: Vec<VerifyingKey>,
}

impl KeyRing {
    /// Builds a ring.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when `keys` is empty, holds more than [`MAX_RING_KEYS`],
    /// or contains two keys with the same [`KeyId`]. A duplicated identifier would make `kid`
    /// selection depend on insertion order, which is exactly the kind of ambiguity a key ring must
    /// not have.
    pub fn new(keys: Vec<VerifyingKey>) -> Result<Self, AuthError> {
        if keys.is_empty() || keys.len() > MAX_RING_KEYS {
            return Err(AuthError::PolicyMisconfigured);
        }
        let distinct: BTreeSet<&KeyId> = keys.iter().map(VerifyingKey::id).collect();
        if distinct.len() != keys.len() {
            return Err(AuthError::PolicyMisconfigured);
        }
        Ok(Self { keys })
    }

    /// The key whose identifier is exactly `kid`, if the ring holds one.
    #[must_use]
    pub fn get(&self, kid: &str) -> Option<&VerifyingKey> {
        self.keys.iter().find(|key| key.id.as_str() == kid)
    }

    /// How many keys the ring holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether the ring is empty. It never is — [`Self::new`] refuses one — but clippy asks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

// ---------------------------------------------------------------------------------------------
// Issuer, audience, scope, and the bounded configuration
// ---------------------------------------------------------------------------------------------

/// A validated printable-ASCII string used for an issuer, an audience, or a scope token.
fn printable_ascii(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
}

/// Who issued a token. Compared for **exact** equality; never parsed, never resolved.
///
/// RFC 8725 §3.8 is not a string comparison — it requires binding the **verifying key** to the
/// issuer. That binding is the [`KeyRing`]: one verifier holds one issuer and one ring, so a token
/// claiming this issuer can only ever be checked with this issuer's keys.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Issuer(String);

impl Issuer {
    /// The longest issuer accepted.
    pub const MAX_LEN: usize = 255;

    /// Builds an issuer.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when empty, over [`Self::MAX_LEN`], or not printable
    /// ASCII.
    pub fn new(value: &str) -> Result<Self, AuthError> {
        if printable_ascii(value, Self::MAX_LEN) {
            Ok(Self(value.to_string()))
        } else {
            Err(AuthError::PolicyMisconfigured)
        }
    }

    /// The issuer as it appears in the `iss` claim.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Issuer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Who a token is for. Checked against an **explicit configured set**, never against a wildcard.
///
/// RFC 9700 §2.3 makes issuing audience-restricted tokens a `SHOULD`, but **rejecting a
/// mis-audienced token a `MUST`**. The asymmetry is why [`AccessTokenVerifier`] cannot be built
/// without at least one audience.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Audience(String);

impl Audience {
    /// The longest audience accepted.
    pub const MAX_LEN: usize = 255;

    /// Builds an audience.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when empty, over [`Self::MAX_LEN`], or not printable
    /// ASCII.
    pub fn new(value: &str) -> Result<Self, AuthError> {
        if printable_ascii(value, Self::MAX_LEN) {
            Ok(Self(value.to_string()))
        } else {
            Err(AuthError::PolicyMisconfigured)
        }
    }

    /// The audience as it appears in the `aud` claim.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Audience {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One privilege a token carries.
///
/// The character set is RFC 6749 §3.3's `NQCHAR` — printable ASCII excluding the double quote,
/// the backslash, and the space that separates scope tokens. Validating on construction is what
/// makes [`ScopeSet::to_claim`] and [`ScopeSet::parse_claim`] exact inverses: nothing needs
/// escaping, so nothing can be smuggled through an escape.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Scope(String);

impl Scope {
    /// The longest single scope token accepted.
    pub const MAX_LEN: usize = 64;

    /// Builds a scope token.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when empty, over [`Self::MAX_LEN`], or containing a
    /// character outside RFC 6749 §3.3's `NQCHAR`.
    pub fn new(value: &str) -> Result<Self, AuthError> {
        let acceptable = !value.is_empty()
            && value.len() <= Self::MAX_LEN
            && value
                .bytes()
                .all(|b| matches!(b, 0x21 | 0x23..=0x5B | 0x5D..=0x7E));
        if acceptable {
            Ok(Self(value.to_string()))
        } else {
            Err(AuthError::PolicyMisconfigured)
        }
    }

    /// The scope token as it appears inside the `scope` claim.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The set of privileges a token carries.
///
/// A `BTreeSet` rather than a `Vec`: a scope set has no order and no duplicates, and encoding that
/// in the type means [`Self::to_claim`] produces the same string for the same privileges every
/// time — which is what makes a token's claim set comparable in a test without sorting it first.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ScopeSet(BTreeSet<Scope>);

impl ScopeSet {
    /// The largest number of scope tokens one token may carry.
    pub const MAX_SCOPES: usize = 32;

    /// Builds a scope set.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when more than [`Self::MAX_SCOPES`] distinct scopes are
    /// supplied.
    pub fn new(scopes: impl IntoIterator<Item = Scope>) -> Result<Self, AuthError> {
        let set: BTreeSet<Scope> = scopes.into_iter().collect();
        if set.len() > Self::MAX_SCOPES {
            return Err(AuthError::PolicyMisconfigured);
        }
        Ok(Self(set))
    }

    /// Whether this set grants `scope`.
    ///
    /// **This is the operation-level check FR-043 requires.** An edge that decided a request was
    /// "authenticated" has decided nothing about whether it may perform a particular operation, and
    /// the two questions are answered in two different places on purpose.
    #[must_use]
    pub fn grants(&self, scope: &Scope) -> bool {
        self.0.contains(scope)
    }

    /// Whether this set grants every scope in `required`.
    #[must_use]
    pub fn grants_all(&self, required: &Self) -> bool {
        required.0.is_subset(&self.0)
    }

    /// How many scopes the set holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The space-delimited `scope` claim value (RFC 6749 §3.3).
    #[must_use]
    pub fn to_claim(&self) -> String {
        self.0
            .iter()
            .map(Scope::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Parses a `scope` claim value.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when any token is not a valid [`Scope`] or the set is
    /// over-large. An empty claim yields an empty set, which grants nothing.
    pub fn parse_claim(claim: &str) -> Result<Self, AuthError> {
        let scopes = claim
            .split(' ')
            .filter(|token| !token.is_empty())
            .map(Scope::new)
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(scopes)
    }
}

/// A bounded, configurable tolerance for imperfectly synchronised clocks (FR-039).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Skew(Duration);

impl Skew {
    /// Builds a skew.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when negative or above [`MAX_SKEW`]. Refused, never
    /// clamped — see [`MAX_SKEW`].
    pub fn new(tolerance: Duration) -> Result<Self, AuthError> {
        if tolerance < Duration::zero() || tolerance > MAX_SKEW {
            return Err(AuthError::PolicyMisconfigured);
        }
        Ok(Self(tolerance))
    }

    /// The configured tolerance.
    #[must_use]
    pub const fn get(self) -> Duration {
        self.0
    }
}

impl Default for Skew {
    /// Thirty seconds: enough for machines whose clocks are merely imperfect, far short of enough
    /// to matter to a token that lives fifteen minutes.
    fn default() -> Self {
        Self(Duration::seconds(30))
    }
}

/// How long an issued access token stays valid (FR-036).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AccessLifetime(Duration);

impl AccessLifetime {
    /// Builds a lifetime.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when not positive or above [`MAX_ACCESS_LIFETIME`].
    pub fn new(lifetime: Duration) -> Result<Self, AuthError> {
        if lifetime <= Duration::zero() || lifetime > MAX_ACCESS_LIFETIME {
            return Err(AuthError::PolicyMisconfigured);
        }
        Ok(Self(lifetime))
    }

    /// The configured duration.
    #[must_use]
    pub const fn get(self) -> Duration {
        self.0
    }
}

impl Default for AccessLifetime {
    /// Fifteen minutes.
    fn default() -> Self {
        Self(Duration::minutes(15))
    }
}

// ---------------------------------------------------------------------------------------------
// The token itself
// ---------------------------------------------------------------------------------------------

/// A token identifier (`jti`), unique per issued token.
///
/// Not a secret — it exists so that a specific token can be named in a revocation list — but it is
/// generated from the same entropy port as every other random value in this crate, because a
/// predictable `jti` makes a replay-detection table guessable.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct JwtId(String);

impl JwtId {
    /// The number of random bytes behind an identifier.
    pub const BYTES: usize = 16;

    /// Generates an identifier.
    ///
    /// # Errors
    ///
    /// [`AuthError::EntropyUnavailable`] when the platform CSPRNG fails. No fallback.
    pub fn generate(source: &dyn EntropySource) -> Result<Self, AuthError> {
        let mut bytes = [0_u8; Self::BYTES];
        source
            .fill(&mut bytes)
            .map_err(|_| AuthError::EntropyUnavailable)?;
        let mut rendered = String::with_capacity(Self::BYTES * 2);
        for byte in bytes {
            use fmt::Write as _;
            let _ = write!(rendered, "{byte:02x}");
        }
        Ok(Self(rendered))
    }

    /// Rebuilds an identifier from its rendered form.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when `value` is not exactly `2 * BYTES` lowercase hex
    /// digits.
    pub fn from_wire(value: &str) -> Result<Self, AuthError> {
        let acceptable = value.len() == Self::BYTES * 2
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if acceptable {
            Ok(Self(value.to_string()))
        } else {
            Err(AuthError::PolicyMisconfigured)
        }
    }

    /// The identifier as it appears in the `jti` claim.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for JwtId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A signed access token, ready to be presented as a bearer credential.
///
/// # It renders redacted, like every other credential in this crate
///
/// A bearer token is a credential in the strongest sense: whoever holds the string is the subject
/// until it expires. `Debug` and `Display` therefore both print `[redacted]`, and [`Self::expose`]
/// is the single deliberate exit — the same shape [`crate::opaque::Opaque`] has.
#[derive(Clone)]
pub struct AccessToken(String);

impl AccessToken {
    /// The token, for the one place that must transmit it.
    ///
    /// Every call site is a place a credential can escape into a log. There is exactly one in this
    /// crate's non-test code, and there should be exactly one in yours.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AccessToken({REDACTED})")
    }
}

impl fmt::Display for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

/// The claim set, exactly as it appears on the wire.
///
/// Private on purpose: the claim set is an internal representation, and a client that decodes a
/// token and depends on this shape is depending on something Renvor may change. Every field is
/// **non-optional**, so a token missing any of them fails to deserialise rather than reaching a
/// check that might have been forgotten.
#[derive(serde::Serialize, serde::Deserialize)]
struct AccessClaims {
    iss: String,
    aud: Vec<String>,
    sub: String,
    exp: i64,
    iat: i64,
    jti: String,
    scope: String,
}

/// Why a presented token was not accepted.
///
/// # There is nowhere in this type to put a secret
///
/// Every variant is a bare marker. This is the shape `renvor-error`'s `ApiErrorCode` established
/// and FR-076 requires for audit events: the type *cannot* carry the token, the claim that failed,
/// or the key that did not match, so no `Debug`, log line, or telemetry span built from it can leak
/// one. The reason is deliberately coarse for the same argument — telling a caller precisely which
/// of eleven checks failed is telling an attacker precisely which one to fix next.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[non_exhaustive]
pub enum TokenRejection {
    /// Not three base64url segments, or the header did not parse.
    Malformed,
    /// The header named a token type this verifier does not issue.
    WrongTokenType,
    /// The header asked the verifier to fetch or trust key material: `jku`, `x5u`, or `jwk`.
    RemoteKeyRequested,
    /// The header carried `crit`, so it asked for an extension this verifier does not implement.
    UnsupportedCriticalHeader,
    /// The header carried no `kid`, or one the local ring does not hold.
    UnknownKey,
    /// The header's `alg` is not the algorithm the selected key is bound to.
    AlgorithmMismatch,
    /// The signature did not verify under the selected key.
    BadSignature,
    /// A required claim was absent or malformed.
    MalformedClaims,
    /// The `iss` claim is not this verifier's issuer.
    IssuerMismatch,
    /// No `aud` value is in this verifier's configured audience set.
    AudienceMismatch,
    /// The token has expired, allowing for the configured skew.
    Expired,
    /// The token's `iat` is further in the future than the configured skew allows.
    IssuedInTheFuture,
}

impl fmt::Display for TokenRejection {
    /// A **static** description per variant. Nothing derived from the presented token can reach a
    /// log line through this implementation, because there is nothing derived from it to reach one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Malformed => "the token is not a well-formed JWS",
            Self::WrongTokenType => "the token type is not one this verifier issues",
            Self::RemoteKeyRequested => "the header requested remote or embedded key material",
            Self::UnsupportedCriticalHeader => "the header requested an unimplemented extension",
            Self::UnknownKey => "no local key matches the key identifier",
            Self::AlgorithmMismatch => "the algorithm is not the one the key is bound to",
            Self::BadSignature => "the signature did not verify",
            Self::MalformedClaims => "a required claim is absent or malformed",
            Self::IssuerMismatch => "the issuer is not this verifier's issuer",
            Self::AudienceMismatch => "the audience is not a configured audience",
            Self::Expired => "the token has expired",
            Self::IssuedInTheFuture => "the token was issued too far in the future",
        })
    }
}

/// What a verified token establishes.
///
/// The subject is an [`AuthenticatedSubject`], which is the type whose construction *is* the
/// assertion that authentication happened — so a verified token and a live session produce the same
/// evidence, and an operation downstream cannot tell which one it came from and does not need to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VerifiedAccess {
    subject: AuthenticatedSubject,
    expires_at: DateTime<Utc>,
}

/// What a verified token establishes, together with its scopes and identifier.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VerifiedToken {
    access: VerifiedAccess,
    scopes: ScopeSet,
    id: JwtId,
}

impl VerifiedToken {
    /// The subject the token authenticates.
    #[must_use]
    pub const fn subject(&self) -> AuthenticatedSubject {
        self.access.subject
    }

    /// When the token stops being valid.
    #[must_use]
    pub const fn expires_at(&self) -> DateTime<Utc> {
        self.access.expires_at
    }

    /// The privileges the token carries.
    ///
    /// Ask this **inside the operation**, not at the edge (FR-043).
    #[must_use]
    pub const fn scopes(&self) -> &ScopeSet {
        &self.scopes
    }

    /// The token's identifier.
    #[must_use]
    pub const fn id(&self) -> &JwtId {
        &self.id
    }
}

// ---------------------------------------------------------------------------------------------
// Issuing
// ---------------------------------------------------------------------------------------------

/// Issues signed access tokens.
pub struct AccessTokenIssuer {
    issuer: Issuer,
    audience: Audience,
    key: SigningKey,
    lifetime: AccessLifetime,
}

impl AccessTokenIssuer {
    /// Builds an issuer.
    #[must_use]
    pub const fn new(
        issuer: Issuer,
        audience: Audience,
        key: SigningKey,
        lifetime: AccessLifetime,
    ) -> Self {
        Self {
            issuer,
            audience,
            key,
            lifetime,
        }
    }

    /// The identifier of the key this issuer signs with.
    #[must_use]
    pub const fn key_id(&self) -> &KeyId {
        self.key.id()
    }

    /// Issues a token for `subject` carrying `scopes`.
    ///
    /// The `alg` and `kid` headers are taken **from the key**, never from a parameter, so an
    /// issuer cannot be asked to sign with an algorithm the key is not bound to.
    ///
    /// # Errors
    ///
    /// [`AuthError::EntropyUnavailable`] when the `jti` cannot be generated;
    /// [`AuthError::PolicyMisconfigured`] when the claim set cannot be serialised or signed, which
    /// means the key material and the algorithm disagree — a configuration fault, not a request
    /// fault.
    pub fn issue(
        &self,
        subject: AuthenticatedSubject,
        scopes: &ScopeSet,
        clock: &dyn Clock,
        entropy: &dyn EntropySource,
    ) -> Result<AccessToken, AuthError> {
        let now = clock.now();
        let claims = AccessClaims {
            iss: self.issuer.as_str().to_string(),
            aud: vec![self.audience.as_str().to_string()],
            sub: subject.user_id().to_string(),
            exp: (now + self.lifetime.get()).timestamp(),
            iat: now.timestamp(),
            jti: JwtId::generate(entropy)?.as_str().to_string(),
            scope: scopes.to_claim(),
        };

        let mut header = jsonwebtoken::Header::new(self.key.algorithm.backend());
        header.typ = Some(ACCESS_TOKEN_TYP.to_string());
        header.kid = Some(self.key.id.as_str().to_string());

        jsonwebtoken::encode(&header, &claims, &self.key.inner)
            .map(AccessToken)
            .map_err(|_| AuthError::PolicyMisconfigured)
    }
}

impl fmt::Debug for AccessTokenIssuer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccessTokenIssuer")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("key", &self.key)
            .field("lifetime", &self.lifetime)
            .finish()
    }
}

// ---------------------------------------------------------------------------------------------
// Verifying
// ---------------------------------------------------------------------------------------------

/// Verifies presented access tokens against a fixed issuer and a bounded local key ring.
#[derive(Clone, Debug)]
pub struct AccessTokenVerifier {
    issuer: Issuer,
    audiences: BTreeSet<Audience>,
    ring: KeyRing,
    skew: Skew,
}

impl AccessTokenVerifier {
    /// Builds a verifier.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when `audiences` is empty. RFC 9700 §2.3 makes rejecting
    /// a mis-audienced token a `MUST`, and a verifier with no configured audience has no way to.
    pub fn new(
        issuer: Issuer,
        audiences: impl IntoIterator<Item = Audience>,
        ring: KeyRing,
        skew: Skew,
    ) -> Result<Self, AuthError> {
        let audiences: BTreeSet<Audience> = audiences.into_iter().collect();
        if audiences.is_empty() {
            return Err(AuthError::PolicyMisconfigured);
        }
        Ok(Self {
            issuer,
            audiences,
            ring,
            skew,
        })
    }

    /// Verifies `presented`.
    ///
    /// # The order of these checks is the security argument
    ///
    /// The header is inspected **before** any signature work: a token that asks for a remote key,
    /// carries a `crit` this verifier does not implement, or names an unknown `kid` is refused
    /// without a cryptographic operation being attempted at all. Only then is a key selected, and
    /// only then is the `alg` **compared** with that key's algorithm — a comparison, never a
    /// selection.
    ///
    /// # Errors
    ///
    /// A [`TokenRejection`], which carries no part of the presented token.
    pub fn verify(
        &self,
        presented: &str,
        clock: &dyn Clock,
    ) -> Result<VerifiedToken, TokenRejection> {
        let header =
            jsonwebtoken::decode_header(presented).map_err(|_| TokenRejection::Malformed)?;

        // 1. Explicit typing (RFC 8725 §3.11). A token issued for another purpose is not an access
        //    token, whatever it is signed with.
        if header.typ.as_deref() != Some(ACCESS_TOKEN_TYP) {
            return Err(TokenRejection::WrongTokenType);
        }

        // 2. ASVS V9.1.3 (L1) — all three, not RFC 8725 §3.10's two. REJECTED, not ignored.
        if header.jku.is_some() || header.x5u.is_some() || header.jwk.is_some() {
            return Err(TokenRejection::RemoteKeyRequested);
        }

        // 3. `crit` names extensions the verifier MUST understand. This one understands none, so
        //    any `crit` at all — including an empty list, which is itself malformed — is a refusal.
        if header.crit.is_some() {
            return Err(TokenRejection::UnsupportedCriticalHeader);
        }

        // 4. The key comes from the local ring or the token does not proceed.
        let kid = header.kid.as_deref().ok_or(TokenRejection::UnknownKey)?;
        let key = self.ring.get(kid).ok_or(TokenRejection::UnknownKey)?;

        // 5. THE COMPARISON. `header.alg` is checked against the algorithm the key is bound to. It
        //    does not widen a list, and it does not choose. RFC 8725 §3.1.
        if header.alg != key.algorithm.backend() {
            return Err(TokenRejection::AlgorithmMismatch);
        }

        let mut validation = jsonwebtoken::Validation::new(key.algorithm.backend());
        // Exactly one element, taken from the key. This is the invariant `Validation` cannot state.
        debug_assert_eq!(validation.algorithms.len(), 1);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(
            &self
                .audiences
                .iter()
                .map(Audience::as_str)
                .collect::<Vec<_>>(),
        );
        validation.required_spec_claims = ["iss", "aud", "sub", "exp", "iat", "jti"]
            .iter()
            .map(|c| (*c).to_string())
            .collect::<HashSet<String>>();
        // Time is ours. See the module header: the backend would use the system clock, which no
        // test can control and no operator can bound.
        validation.validate_exp = false;
        validation.validate_nbf = false;

        let decoded = jsonwebtoken::decode::<AccessClaims>(presented, &key.inner, &validation)
            .map_err(|error| match error.kind() {
                jsonwebtoken::errors::ErrorKind::InvalidIssuer => TokenRejection::IssuerMismatch,
                jsonwebtoken::errors::ErrorKind::InvalidAudience => {
                    TokenRejection::AudienceMismatch
                }
                jsonwebtoken::errors::ErrorKind::InvalidSignature => TokenRejection::BadSignature,
                _ => TokenRejection::MalformedClaims,
            })?;
        let claims = decoded.claims;

        // 6. Belt and braces on the issuer. The backend checked it; so does this, because a
        //    verifier that trusts one library call for its identity binding has one place to be
        //    wrong.
        if claims.iss != self.issuer.as_str() {
            return Err(TokenRejection::IssuerMismatch);
        }
        if !claims.aud.iter().any(|value| {
            self.audiences
                .iter()
                .any(|allowed| allowed.as_str() == value)
        }) {
            return Err(TokenRejection::AudienceMismatch);
        }

        // 7. Time, against the injected clock and the bounded skew.
        let now = clock.now();
        let expires_at =
            DateTime::from_timestamp(claims.exp, 0).ok_or(TokenRejection::MalformedClaims)?;
        let issued_at =
            DateTime::from_timestamp(claims.iat, 0).ok_or(TokenRejection::MalformedClaims)?;
        if now > expires_at + self.skew.get() {
            return Err(TokenRejection::Expired);
        }
        if issued_at > now + self.skew.get() {
            return Err(TokenRejection::IssuedInTheFuture);
        }

        // 8. The remaining claims.
        let user_id = parse_user_id(&claims.sub).ok_or(TokenRejection::MalformedClaims)?;
        let id = JwtId::from_wire(&claims.jti).map_err(|_| TokenRejection::MalformedClaims)?;
        let scopes =
            ScopeSet::parse_claim(&claims.scope).map_err(|_| TokenRejection::MalformedClaims)?;

        Ok(VerifiedToken {
            access: VerifiedAccess {
                subject: AuthenticatedSubject::new(user_id),
                expires_at,
            },
            scopes,
            id,
        })
    }
}

/// Parses the 32 lowercase hex digits a [`UserId`] renders as.
fn parse_user_id(rendered: &str) -> Option<UserId> {
    const HEX_LEN: usize = 32;
    if rendered.len() != HEX_LEN {
        return None;
    }
    let mut bytes = [0_u8; 16];
    for (index, slot) in bytes.iter_mut().enumerate() {
        let pair = rendered.get(index * 2..index * 2 + 2)?;
        if !pair
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return None;
        }
        *slot = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(UserId::from_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::{
        ACCESS_TOKEN_TYP, AccessLifetime, AccessToken, AccessTokenIssuer, AccessTokenVerifier,
        Audience, Issuer, JwtId, KeyId, KeyRing, MAX_ACCESS_LIFETIME, MAX_RING_KEYS, MAX_SKEW,
        REDACTED, Scope, ScopeSet, SigningKey, Skew, TokenAlgorithm, TokenRejection, VerifyingKey,
        parse_user_id,
    };
    use crate::clock::{Clock, FixedClock};
    use crate::error::AuthError;
    use crate::subject::{AuthenticatedSubject, UserId};
    use chrono::{DateTime, Duration, Utc};
    use renvor_core::observe::entropy::OsEntropy;

    // NO DIAGNOSTIC IN THIS MODULE INTERPOLATES A TOKEN, A CLAIM, A KEY, OR A SEED. A failure
    // message that printed the token would put a live bearer credential into the test log at
    // exactly the moment the defence around it failed — reporting the leak by leaking. Every
    // assertion is identified by a static description instead. This is the rule batch F's CodeQL
    // correction established, applied here from the first line rather than retrofitted.

    /// A fixed instant, so every expiry assertion is arithmetic rather than a race.
    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + seconds, 0).expect("a representable instant")
    }

    fn subject() -> AuthenticatedSubject {
        AuthenticatedSubject::new(UserId::from_bytes([7_u8; 16]))
    }

    fn key_id(name: &str) -> KeyId {
        KeyId::new(name).expect("a well-formed key identifier")
    }

    fn issuer() -> Issuer {
        Issuer::new("https://renvor.test/issuer").expect("a well-formed issuer")
    }

    fn audience() -> Audience {
        Audience::new("https://renvor.test/api").expect("a well-formed audience")
    }

    fn scopes(names: &[&str]) -> ScopeSet {
        ScopeSet::new(
            names
                .iter()
                .map(|name| Scope::new(name).expect("a well-formed scope")),
        )
        .expect("a bounded scope set")
    }

    /// An Ed25519 key pair whose **raw public bytes are also returned**, so a test can verify a
    /// token without going back through the code under test.
    struct RawPair {
        signing: SigningKey,
        verifying: VerifyingKey,
        public: Vec<u8>,
    }

    fn ed25519_pair(id: &str, seed_byte: u8) -> RawPair {
        use aws_lc_rs::signature::KeyPair as _;
        let seed = [seed_byte; 32];
        let pair = aws_lc_rs::signature::Ed25519KeyPair::from_seed_unchecked(&seed)
            .expect("a usable seed");
        let document = pair.to_pkcs8().expect("a serialisable key");
        RawPair {
            signing: SigningKey::ed25519_pkcs8(key_id(id), document.as_ref())
                .expect("a usable Ed25519 key"),
            verifying: VerifyingKey::ed25519(key_id(id), pair.public_key().as_ref())
                .expect("a usable Ed25519 public key"),
            public: pair.public_key().as_ref().to_vec(),
        }
    }

    fn es256_pair(id: &str) -> RawPair {
        use aws_lc_rs::signature::KeyPair as _;
        let algorithm = &aws_lc_rs::signature::ECDSA_P256_SHA256_FIXED_SIGNING;
        let document = aws_lc_rs::signature::EcdsaKeyPair::generate_pkcs8(
            algorithm,
            &aws_lc_rs::rand::SystemRandom::new(),
        )
        .expect("a generated P-256 key");
        let pair = aws_lc_rs::signature::EcdsaKeyPair::from_pkcs8(algorithm, document.as_ref())
            .expect("a usable P-256 key");
        RawPair {
            signing: SigningKey::es256_pkcs8(key_id(id), document.as_ref())
                .expect("a usable P-256 signing key"),
            verifying: VerifyingKey::es256(key_id(id), pair.public_key().as_ref())
                .expect("a usable P-256 public key"),
            public: pair.public_key().as_ref().to_vec(),
        }
    }

    fn verifier(keys: Vec<VerifyingKey>) -> AccessTokenVerifier {
        AccessTokenVerifier::new(
            issuer(),
            [audience()],
            KeyRing::new(keys).expect("a bounded ring"),
            Skew::default(),
        )
        .expect("a configured verifier")
    }

    fn issue(signing: SigningKey, clock: &dyn Clock) -> AccessToken {
        AccessTokenIssuer::new(issuer(), audience(), signing, AccessLifetime::default())
            .issue(
                subject(),
                &scopes(&["read", "write"]),
                clock,
                &OsEntropy::new(),
            )
            .expect("a signed token")
    }

    // -- base64url, written here on purpose ---------------------------------------------------
    //
    // The compatibility test below must not reuse the encoder the code under test uses, or it
    // would prove only that one implementation agrees with itself. These sixteen lines are
    // RFC 4648 §5 with padding omitted, which is what RFC 7515 §2 requires.

    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    fn b64url_encode(bytes: &[u8]) -> String {
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            let indices = [n >> 18 & 63, n >> 12 & 63, n >> 6 & 63, n & 63];
            for (position, index) in indices.iter().enumerate() {
                if position <= chunk.len() {
                    out.push(char::from(B64[*index as usize]));
                }
            }
        }
        out
    }

    fn b64url_decode(text: &str) -> Option<Vec<u8>> {
        let mut accumulator = 0_u32;
        let mut bits = 0_u32;
        let mut out = Vec::new();
        for byte in text.bytes() {
            let index = B64.iter().position(|candidate| *candidate == byte)? as u32;
            accumulator = (accumulator << 6) | index;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push(u8::try_from((accumulator >> bits) & 0xFF).ok()?);
            }
        }
        Some(out)
    }

    /// Builds a token with an arbitrary header and an arbitrary signature.
    ///
    /// Every header check runs **before** any cryptographic operation, so a deliberately invalid
    /// signature cannot mask a header rejection — if a header test ever started failing with
    /// `BadSignature`, that would itself be the finding.
    fn crafted(header_json: &str, payload_json: &str) -> String {
        format!(
            "{}.{}.{}",
            b64url_encode(header_json.as_bytes()),
            b64url_encode(payload_json.as_bytes()),
            b64url_encode(b"not-a-signature")
        )
    }

    // -- round trip ---------------------------------------------------------------------------

    #[test]
    fn a_round_trip_yields_the_subject_the_scopes_and_the_identifier() {
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));
        let token = issue(pair.signing, &clock);

        let verified = verifier(vec![pair.verifying])
            .verify(token.expose(), &clock)
            .expect("the token this test just issued");

        assert!(
            verified.subject() == subject(),
            "the subject did not survive the round trip"
        );
        assert!(
            verified.scopes() == &scopes(&["read", "write"]),
            "the scopes did not survive"
        );
        assert!(
            verified.expires_at() == at(0) + AccessLifetime::default().get(),
            "expiry is not issue time plus the configured lifetime"
        );
        assert!(
            JwtId::from_wire(verified.id().as_str()).is_ok(),
            "the token identifier is not the shape this crate generates"
        );
    }

    #[test]
    fn the_issued_header_names_the_type_the_key_and_the_algorithm() {
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));
        let token = issue(pair.signing, &clock);

        let header = jsonwebtoken::decode_header(token.expose()).expect("a parseable header");
        assert!(
            header.typ.as_deref() == Some(ACCESS_TOKEN_TYP),
            "the header names the wrong type"
        );
        assert!(
            header.kid.as_deref() == Some("k1"),
            "the header names the wrong key"
        );
        assert!(
            header.alg == jsonwebtoken::Algorithm::EdDSA,
            "the header names the wrong algorithm"
        );
        assert!(
            header.jku.is_none() && header.x5u.is_none() && header.jwk.is_none(),
            "this crate issued a header that asks for remote key material"
        );
        assert!(
            header.crit.is_none(),
            "this crate issued a header with a critical extension"
        );
    }

    // -- the algorithm and key binding --------------------------------------------------------

    #[test]
    fn a_token_signed_by_a_different_key_of_the_same_algorithm_is_rejected() {
        let signer = ed25519_pair("k1", 1);
        let other = ed25519_pair("k1", 2); // SAME kid, different key material.
        let clock = FixedClock::at(at(0));
        let token = issue(signer.signing, &clock);

        let outcome = verifier(vec![other.verifying]).verify(token.expose(), &clock);
        assert!(
            outcome == Err(TokenRejection::BadSignature),
            "a token signed by a key the ring does not hold was not refused as a bad signature"
        );
    }

    #[test]
    fn a_token_of_a_different_algorithm_is_rejected_even_when_the_ring_holds_its_identifier() {
        let ed = ed25519_pair("k1", 1);
        let ec = es256_pair("k1"); // SAME kid, different algorithm.
        let clock = FixedClock::at(at(0));
        let token = issue(ec.signing, &clock);

        // The ring is bound to Ed25519 under this identifier. The presented token says ES256.
        let outcome = verifier(vec![ed.verifying]).verify(token.expose(), &clock);
        assert!(
            outcome == Err(TokenRejection::AlgorithmMismatch),
            "the token's algorithm was allowed to differ from the key's"
        );
    }

    #[test]
    fn an_es256_token_verifies_under_an_es256_key() {
        // The mirror of the test above: the binding refuses a mismatch, and it must not refuse a
        // match. Without this, a verifier that rejected everything would pass the suite.
        let ec = es256_pair("k1");
        let clock = FixedClock::at(at(0));
        let token = issue(ec.signing, &clock);

        assert!(
            verifier(vec![ec.verifying])
                .verify(token.expose(), &clock)
                .is_ok(),
            "a correctly signed ES256 token was not accepted"
        );
    }

    #[test]
    fn a_header_that_names_another_algorithm_cannot_widen_the_verifier() {
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));
        let token = crafted(
            &format!(r#"{{"typ":"{ACCESS_TOKEN_TYP}","alg":"ES256","kid":"k1"}}"#),
            r#"{"iss":"x","aud":["y"],"sub":"z","exp":1,"iat":1,"jti":"j","scope":""}"#,
        );

        let outcome = verifier(vec![pair.verifying]).verify(&token, &clock);
        assert!(
            outcome == Err(TokenRejection::AlgorithmMismatch),
            "a token's own `alg` was able to select the verification algorithm"
        );
    }

    #[test]
    fn an_unsecured_token_is_rejected() {
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));
        let token = crafted(
            &format!(r#"{{"typ":"{ACCESS_TOKEN_TYP}","alg":"none","kid":"k1"}}"#),
            r#"{"iss":"x","aud":["y"],"sub":"z","exp":1,"iat":1,"jti":"j","scope":""}"#,
        );

        // `alg: none` is not a member of the algorithm enumeration at all, so the header does not
        // parse. The variant matters less than the guarantee: there is no accepting path.
        assert!(
            verifier(vec![pair.verifying])
                .verify(&token, &clock)
                .is_err(),
            "an unsecured token was accepted"
        );
    }

    #[test]
    fn an_unknown_key_identifier_is_rejected() {
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));
        let token = crafted(
            &format!(r#"{{"typ":"{ACCESS_TOKEN_TYP}","alg":"EdDSA","kid":"k9"}}"#),
            r#"{"iss":"x","aud":["y"],"sub":"z","exp":1,"iat":1,"jti":"j","scope":""}"#,
        );
        assert!(
            verifier(vec![pair.verifying]).verify(&token, &clock)
                == Err(TokenRejection::UnknownKey),
            "a key identifier outside the ring was not refused"
        );
    }

    #[test]
    fn a_header_without_a_key_identifier_is_rejected() {
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));
        let token = crafted(
            &format!(r#"{{"typ":"{ACCESS_TOKEN_TYP}","alg":"EdDSA"}}"#),
            r#"{"iss":"x","aud":["y"],"sub":"z","exp":1,"iat":1,"jti":"j","scope":""}"#,
        );
        assert!(
            verifier(vec![pair.verifying]).verify(&token, &clock)
                == Err(TokenRejection::UnknownKey),
            "a header with no key identifier was not refused"
        );
    }

    // -- remote key material, ASVS V9.1.3 -----------------------------------------------------

    #[test]
    fn every_remote_key_request_is_rejected_and_not_merely_ignored() {
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));
        let ring = verifier(vec![pair.verifying]);

        // `jwk` is the one RFC 8725 §3.10 omits and ASVS V9.1.3 covers. If only the RFC's list
        // were implemented, probe 2 would be the one that passed.
        let probes = [
            r#""jku":"https://attacker.test/keys""#,
            r#""x5u":"https://attacker.test/chain""#,
            r#""jwk":{"kty":"OKP","crv":"Ed25519","x":"AAAA"}"#,
        ];
        for (probe, injected) in probes.into_iter().enumerate() {
            let token = crafted(
                &format!(r#"{{"typ":"{ACCESS_TOKEN_TYP}","alg":"EdDSA","kid":"k1",{injected}}}"#),
                r#"{"iss":"x","aud":["y"],"sub":"z","exp":1,"iat":1,"jti":"j","scope":""}"#,
            );
            assert!(
                ring.verify(&token, &clock) == Err(TokenRejection::RemoteKeyRequested),
                "a header asking for remote or embedded key material was not refused by probe {probe}"
            );
        }
    }

    #[test]
    fn a_critical_header_extension_is_rejected() {
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));
        let token = crafted(
            &format!(r#"{{"typ":"{ACCESS_TOKEN_TYP}","alg":"EdDSA","kid":"k1","crit":["exp"]}}"#),
            r#"{"iss":"x","aud":["y"],"sub":"z","exp":1,"iat":1,"jti":"j","scope":""}"#,
        );
        assert!(
            verifier(vec![pair.verifying]).verify(&token, &clock)
                == Err(TokenRejection::UnsupportedCriticalHeader),
            "a critical extension this verifier does not implement was not refused"
        );
    }

    // -- token kind confusion, RFC 8725 §3.11 / §3.12 -----------------------------------------

    #[test]
    fn a_token_of_another_kind_is_rejected_even_when_correctly_signed() {
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));

        // Signed by the RIGHT key, with the RIGHT algorithm, for the RIGHT audience — and refused,
        // because it is not an access token. This is the mutually-exclusive validation §3.12 asks
        // for: a refresh or identity token must not be spendable as an access token.
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::EdDSA);
        header.typ = Some("renvor-something-else+jwt".to_string());
        header.kid = Some("k1".to_string());
        let claims = serde_json::json!({
            "iss": issuer().as_str(), "aud": [audience().as_str()], "sub": subject().user_id().to_string(),
            "exp": at(600).timestamp(), "iat": at(0).timestamp(),
            "jti": "0123456789abcdef0123456789abcdef", "scope": "read",
        });
        let signer = ed25519_pair("k1", 1);
        let encoded = jsonwebtoken::encode(&header, &claims, encoding_of(&signer.signing))
            .expect("a signed token");

        assert!(
            verifier(vec![pair.verifying]).verify(&encoded, &clock)
                == Err(TokenRejection::WrongTokenType),
            "a correctly signed token of another kind was accepted as an access token"
        );
    }

    /// Reaches the private encoding key, for the one test that must sign a *malformed* token.
    fn encoding_of(key: &SigningKey) -> &jsonwebtoken::EncodingKey {
        &key.inner
    }

    // -- issuer and audience -------------------------------------------------------------------

    #[test]
    fn a_token_from_another_issuer_is_rejected() {
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));
        let other = Issuer::new("https://elsewhere.test/issuer").expect("a well-formed issuer");
        let token =
            AccessTokenIssuer::new(other, audience(), pair.signing, AccessLifetime::default())
                .issue(subject(), &scopes(&["read"]), &clock, &OsEntropy::new())
                .expect("a signed token");

        assert!(
            verifier(vec![pair.verifying]).verify(token.expose(), &clock)
                == Err(TokenRejection::IssuerMismatch),
            "a token from another issuer was accepted"
        );
    }

    #[test]
    fn a_token_for_another_audience_is_rejected() {
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));
        let other = Audience::new("https://elsewhere.test/api").expect("a well-formed audience");
        let token =
            AccessTokenIssuer::new(issuer(), other, pair.signing, AccessLifetime::default())
                .issue(subject(), &scopes(&["read"]), &clock, &OsEntropy::new())
                .expect("a signed token");

        assert!(
            verifier(vec![pair.verifying]).verify(token.expose(), &clock)
                == Err(TokenRejection::AudienceMismatch),
            "a token for another audience was accepted"
        );
    }

    #[test]
    fn one_configured_audience_among_several_is_enough() {
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));
        let token = issue(pair.signing, &clock);

        let wide = AccessTokenVerifier::new(
            issuer(),
            [
                Audience::new("https://other.test/api").expect("a well-formed audience"),
                audience(),
            ],
            KeyRing::new(vec![pair.verifying]).expect("a bounded ring"),
            Skew::default(),
        )
        .expect("a configured verifier");

        assert!(
            wide.verify(token.expose(), &clock).is_ok(),
            "a token whose audience is one of several configured audiences was refused"
        );
    }

    // -- time, against the injected clock ------------------------------------------------------

    #[test]
    fn expiry_is_forgiven_within_the_skew_and_refused_beyond_it() {
        let pair = ed25519_pair("k1", 1);
        let issued = FixedClock::at(at(0));
        let token = issue(pair.signing, &issued);
        let ring = verifier(vec![pair.verifying]);

        let lifetime = AccessLifetime::default().get().num_seconds();
        let skew = Skew::default().get().num_seconds();

        // Inside the lifetime.
        assert!(
            ring.verify(token.expose(), &FixedClock::at(at(lifetime - 1)))
                .is_ok(),
            "a token was refused before it expired"
        );
        // Expired, but inside the skew.
        assert!(
            ring.verify(token.expose(), &FixedClock::at(at(lifetime + skew)))
                .is_ok(),
            "a token was refused inside the configured skew"
        );
        // One second past the skew. THE BOUNDARY IS THE TEST.
        assert!(
            ring.verify(token.expose(), &FixedClock::at(at(lifetime + skew + 1)))
                == Err(TokenRejection::Expired),
            "an expired token was accepted beyond the configured skew"
        );
    }

    #[test]
    fn a_token_issued_further_ahead_than_the_skew_allows_is_rejected() {
        let pair = ed25519_pair("k1", 1);
        let skew = Skew::default().get().num_seconds();
        let token = issue(pair.signing, &FixedClock::at(at(skew + 60)));
        let ring = verifier(vec![pair.verifying]);

        assert!(
            ring.verify(token.expose(), &FixedClock::at(at(0)))
                == Err(TokenRejection::IssuedInTheFuture),
            "a token issued beyond the skew in the future was accepted"
        );
        // And a token issued only slightly ahead is fine, or the check would be a blanket refusal.
        let near = issue(ed25519_pair("k1", 1).signing, &FixedClock::at(at(skew)));
        assert!(
            ring.verify(near.expose(), &FixedClock::at(at(0))).is_ok(),
            "a token issued inside the skew was refused"
        );
    }

    // -- integrity ------------------------------------------------------------------------------

    #[test]
    fn a_tampered_payload_is_rejected() {
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));
        let token = issue(pair.signing, &clock);

        let parts: Vec<&str> = token.expose().split('.').collect();
        let mut claims: serde_json::Value =
            serde_json::from_slice(&b64url_decode(parts[1]).expect("a decodable payload"))
                .expect("parseable claims");
        claims["scope"] = serde_json::Value::String("read write admin".to_string());
        let forged = format!(
            "{}.{}.{}",
            parts[0],
            b64url_encode(
                serde_json::to_string(&claims)
                    .expect("serialisable claims")
                    .as_bytes()
            ),
            parts[2]
        );

        assert!(
            verifier(vec![pair.verifying]).verify(&forged, &clock)
                == Err(TokenRejection::BadSignature),
            "a payload edited after signing was accepted"
        );
    }

    // -- independent verification, RFC 7515 §5.2 -----------------------------------------------

    #[test]
    fn an_issued_token_verifies_under_an_independent_implementation() {
        // WHAT THIS PROVES, precisely: that what this crate emits is a conformant JWS which an
        // implementation that shares none of `jsonwebtoken`'s parsing or validation code accepts.
        // The signing input is reconstructed from the raw compact serialisation, the base64url
        // codec is the one written in this module, and the signature is checked with the raw
        // Ed25519 primitive rather than through any JWT library.
        //
        // WHAT IT DOES NOT PROVE: it shares the crypto backend, so it is not evidence about
        // `aws-lc-rs` itself. It is evidence about the *encoding* and the *structure*, which is
        // where interoperability actually lives.
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));
        let token = issue(pair.signing, &clock);

        let compact = token.expose();
        let parts: Vec<&str> = compact.split('.').collect();
        assert!(
            parts.len() == 3,
            "the compact serialisation is not three segments"
        );
        assert!(
            !compact.contains('='),
            "the serialisation carries base64 padding"
        );

        let header: serde_json::Value =
            serde_json::from_slice(&b64url_decode(parts[0]).expect("a decodable header"))
                .expect("a parseable header");
        assert!(header["alg"] == "EdDSA", "the header does not name EdDSA");
        assert!(
            header["typ"] == ACCESS_TOKEN_TYP,
            "the header does not name this token type"
        );

        let claims: serde_json::Value =
            serde_json::from_slice(&b64url_decode(parts[1]).expect("a decodable payload"))
                .expect("parseable claims");
        for required in ["iss", "aud", "sub", "exp", "iat", "jti", "scope"] {
            assert!(
                !claims[required].is_null(),
                "a required claim is absent from the payload"
            );
        }

        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let signature = b64url_decode(parts[2]).expect("a decodable signature");
        let independent = aws_lc_rs::signature::UnparsedPublicKey::new(
            &aws_lc_rs::signature::ED25519,
            &pair.public,
        );
        assert!(
            independent
                .verify(signing_input.as_bytes(), &signature)
                .is_ok(),
            "an independent RFC 7515 verification of this crate's own token failed"
        );
    }

    // -- scope, enforced at the operation ------------------------------------------------------

    #[test]
    fn scopes_are_answered_by_the_set_rather_than_by_the_edge() {
        let held = scopes(&["orders.read", "orders.write"]);
        let read = Scope::new("orders.read").expect("a well-formed scope");
        let admin = Scope::new("orders.admin").expect("a well-formed scope");

        assert!(held.grants(&read), "a held scope was not granted");
        assert!(!held.grants(&admin), "an unheld scope was granted");
        assert!(
            held.grants_all(&scopes(&["orders.read"])),
            "a held subset was not granted"
        );
        assert!(
            !held.grants_all(&scopes(&["orders.read", "orders.admin"])),
            "a set containing an unheld scope was granted"
        );
        assert!(
            !ScopeSet::default().grants(&read),
            "an empty scope set granted a privilege"
        );
    }

    #[test]
    fn the_scope_claim_round_trips_exactly() {
        let set = scopes(&["b", "a", "c"]);
        assert!(
            set.to_claim() == "a b c",
            "the scope claim is not canonically ordered"
        );
        assert!(
            ScopeSet::parse_claim("a b c").expect("a parseable claim") == set,
            "a scope claim did not round trip"
        );
        assert!(
            ScopeSet::parse_claim("  a   b  ").expect("a parseable claim") == scopes(&["a", "b"]),
            "repeated separators were not absorbed"
        );
        assert!(
            ScopeSet::parse_claim("")
                .expect("an empty claim")
                .is_empty(),
            "an empty claim did not yield an empty set"
        );
    }

    #[test]
    fn a_malformed_scope_claim_is_rejected() {
        // A quote and a backslash are outside RFC 6749 §3.3's NQCHAR; accepting either is how a
        // scope string becomes a place to hide a second value.
        for malformed in ["read\"write", "read\\write", "read\u{7f}"] {
            assert!(
                ScopeSet::parse_claim(malformed).is_err(),
                "a scope outside RFC 6749 NQCHAR was accepted"
            );
        }
        assert!(Scope::new("").is_err(), "an empty scope was accepted");
        assert!(
            Scope::new(&"x".repeat(Scope::MAX_LEN + 1)).is_err(),
            "an over-long scope was accepted"
        );
        assert!(
            ScopeSet::new(
                (0..=ScopeSet::MAX_SCOPES)
                    .map(|n| Scope::new(&format!("s{n}")).expect("a well-formed scope"))
            )
            .is_err(),
            "an over-large scope set was accepted"
        );
    }

    #[test]
    fn a_token_whose_scope_claim_is_malformed_is_rejected() {
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::EdDSA);
        header.typ = Some(ACCESS_TOKEN_TYP.to_string());
        header.kid = Some("k1".to_string());
        let claims = serde_json::json!({
            "iss": issuer().as_str(), "aud": [audience().as_str()],
            "sub": subject().user_id().to_string(),
            "exp": at(600).timestamp(), "iat": at(0).timestamp(),
            "jti": "0123456789abcdef0123456789abcdef", "scope": "read\"write",
        });
        let signer = ed25519_pair("k1", 1);
        let encoded = jsonwebtoken::encode(&header, &claims, encoding_of(&signer.signing))
            .expect("a signed token");

        assert!(
            verifier(vec![pair.verifying]).verify(&encoded, &clock)
                == Err(TokenRejection::MalformedClaims),
            "a signed token carrying a malformed scope claim was accepted"
        );
    }

    // -- configuration is refused, never clamped -----------------------------------------------

    #[test]
    fn an_out_of_range_configuration_is_refused_rather_than_clamped() {
        assert!(
            Skew::new(MAX_SKEW + Duration::seconds(1)).is_err(),
            "an over-large skew was accepted"
        );
        assert!(
            Skew::new(Duration::seconds(-1)).is_err(),
            "a negative skew was accepted"
        );
        assert!(Skew::new(MAX_SKEW).is_ok(), "the ceiling skew was refused");
        assert!(
            Skew::new(Duration::zero()).is_ok(),
            "a zero skew was refused"
        );

        assert!(
            AccessLifetime::new(MAX_ACCESS_LIFETIME + Duration::seconds(1)).is_err(),
            "an over-long access lifetime was accepted"
        );
        assert!(
            AccessLifetime::new(Duration::zero()).is_err(),
            "a zero lifetime was accepted"
        );
        assert!(
            AccessLifetime::new(MAX_ACCESS_LIFETIME).is_ok(),
            "the ceiling lifetime was refused"
        );
    }

    #[test]
    fn a_ring_must_be_non_empty_bounded_and_unambiguous() {
        assert!(
            KeyRing::new(Vec::new()).is_err(),
            "an empty ring was accepted"
        );

        let many: Vec<VerifyingKey> = (0..=MAX_RING_KEYS)
            .map(|n| {
                ed25519_pair(
                    &format!("k{n}"),
                    u8::try_from(n).expect("a small index") + 1,
                )
                .verifying
            })
            .collect();
        assert!(
            KeyRing::new(many).is_err(),
            "an over-large ring was accepted"
        );

        let duplicated = vec![
            ed25519_pair("k1", 1).verifying,
            ed25519_pair("k1", 2).verifying,
        ];
        assert!(
            KeyRing::new(duplicated).is_err(),
            "a ring holding two keys under one identifier was accepted"
        );
    }

    #[test]
    fn a_verifier_without_an_audience_is_refused() {
        let ring = KeyRing::new(vec![ed25519_pair("k1", 1).verifying]).expect("a bounded ring");
        assert!(
            AccessTokenVerifier::new(issuer(), [], ring, Skew::default()).is_err(),
            "a verifier with no configured audience was built"
        );
    }

    #[test]
    fn identifiers_issuers_audiences_and_key_ids_are_validated() {
        assert!(
            KeyId::new("").is_err(),
            "an empty key identifier was accepted"
        );
        assert!(
            KeyId::new("has space").is_err(),
            "a key identifier with a space was accepted"
        );
        assert!(
            KeyId::new(&"k".repeat(KeyId::MAX_LEN + 1)).is_err(),
            "an over-long key id was accepted"
        );
        assert!(
            KeyId::new("k-1_2.3").is_ok(),
            "the documented charset was refused"
        );

        assert!(Issuer::new("").is_err(), "an empty issuer was accepted");
        assert!(Audience::new("").is_err(), "an empty audience was accepted");
        assert!(
            Issuer::new("héllo").is_err(),
            "a non-ASCII issuer was accepted"
        );

        assert!(
            JwtId::from_wire("0123456789abcdef0123456789abcdef").is_ok(),
            "a valid jti was refused"
        );
        assert!(
            JwtId::from_wire("0123").is_err(),
            "a short jti was accepted"
        );
        assert!(
            JwtId::from_wire("0123456789ABCDEF0123456789abcdef").is_err(),
            "an upper-case jti was accepted, so two renderings would name one token"
        );
        assert!(
            JwtId::generate(&OsEntropy::new()).is_ok(),
            "the entropy port could not produce a jti"
        );
    }

    #[test]
    fn a_verifying_key_refuses_material_of_the_wrong_shape() {
        assert!(
            VerifyingKey::ed25519(key_id("k1"), &[0_u8; 31]).is_err(),
            "a short Ed25519 key was accepted"
        );
        assert!(
            VerifyingKey::es256(key_id("k1"), &[0_u8; 65]).is_err(),
            "a point without the uncompressed tag was accepted"
        );
        assert!(
            VerifyingKey::es256(key_id("k1"), &[4_u8; 64]).is_err(),
            "a short P-256 point was accepted"
        );
        assert!(
            SigningKey::ed25519_pkcs8(key_id("k1"), b"not a key").is_err(),
            "arbitrary bytes were accepted as a signing key"
        );
        assert!(
            SigningKey::es256_pkcs8(key_id("k1"), b"not a key").is_err(),
            "arbitrary bytes were accepted as a P-256 signing key"
        );
    }

    // -- redaction ------------------------------------------------------------------------------

    #[test]
    fn a_token_never_renders_itself() {
        let pair = ed25519_pair("k1", 1);
        let clock = FixedClock::at(at(0));
        let token = issue(pair.signing, &clock);

        let debug = format!("{token:?}");
        let display = format!("{token}");
        assert!(
            debug.contains(REDACTED),
            "Debug omitted the redaction placeholder"
        );
        assert!(
            !debug.contains(token.expose()),
            "Debug rendered the bearer token"
        );
        assert!(
            display == REDACTED,
            "Display was not exactly the redaction placeholder"
        );
    }

    #[test]
    fn a_key_never_renders_its_material() {
        let pair = ed25519_pair("k1", 1);
        let signing = format!("{:?}", pair.signing);
        let verifying = format!("{:?}", pair.verifying);
        assert!(
            signing.contains(REDACTED),
            "a signing key omitted the redaction placeholder"
        );
        assert!(
            signing.contains("k1") && signing.contains("EdDSA"),
            "a signing key did not name itself"
        );
        assert!(
            verifying.contains(REDACTED),
            "a verifying key omitted the redaction placeholder"
        );
    }

    #[test]
    fn a_rejection_has_nowhere_to_put_a_credential() {
        // The type-level guarantee: every variant is a bare marker, so a `Debug` or a log line
        // built from one cannot carry the token, the claim, or the key that failed. This test is
        // the compile-time shape made observable — `TokenRejection` is `Copy`, which a variant
        // holding a `String` could not be.
        fn assert_copy<T: Copy>() {}
        assert_copy::<TokenRejection>();

        let rendered = format!(
            "{:?} {}",
            TokenRejection::BadSignature,
            TokenRejection::Expired
        );
        assert!(!rendered.is_empty(), "a rejection rendered nothing at all");
        assert!(
            rendered.is_ascii(),
            "a rejection rendered something other than its static description"
        );
    }

    // -- helpers ---------------------------------------------------------------------------------

    #[test]
    fn a_subject_identifier_round_trips_through_the_claim() {
        let id = UserId::from_bytes([0xAB; 16]);
        assert!(
            parse_user_id(&id.to_string()) == Some(id),
            "a subject identifier did not round trip"
        );
        assert!(
            parse_user_id("short").is_none(),
            "a short subject was accepted"
        );
        assert!(
            parse_user_id(&"g".repeat(32)).is_none(),
            "a non-hex subject was accepted"
        );
        assert!(
            parse_user_id(&"AB".repeat(16)).is_none(),
            "an upper-case subject was accepted, so two renderings would name one user"
        );
    }

    #[test]
    fn the_algorithms_name_themselves_on_the_wire() {
        assert!(
            TokenAlgorithm::Ed25519.wire_name() == "EdDSA",
            "Ed25519 is misnamed on the wire"
        );
        assert!(
            TokenAlgorithm::Es256.wire_name() == "ES256",
            "ES256 is misnamed on the wire"
        );
    }

    #[test]
    fn the_base64url_codec_in_this_module_round_trips() {
        // The compatibility test is only evidence if its own codec is correct. Without this, a
        // broken decoder could make that test pass by accident.
        for case in [
            &b""[..],
            b"f",
            b"fo",
            b"foo",
            b"foob",
            b"fooba",
            b"foobar",
            &[0xFF_u8; 33],
        ] {
            let encoded = b64url_encode(case);
            assert!(!encoded.contains('='), "the codec emitted padding");
            assert!(
                b64url_decode(&encoded).as_deref() == Some(case),
                "the base64url codec in this module does not round trip"
            );
        }
    }

    #[test]
    fn an_unconfigured_error_is_a_policy_fault_and_not_a_credential_fault() {
        // Batch F added `PolicyMisconfigured` precisely so configuration refusals stop borrowing
        // `PasswordRejected`. Every constructor in this module uses the new variant.
        assert!(
            KeyId::new("").unwrap_err() == AuthError::PolicyMisconfigured,
            "a key id refusal used the wrong variant"
        );
        assert!(
            Issuer::new("").unwrap_err() == AuthError::PolicyMisconfigured,
            "an issuer refusal used the wrong variant"
        );
        assert!(
            Scope::new("").unwrap_err() == AuthError::PolicyMisconfigured,
            "a scope refusal used the wrong variant"
        );
        assert!(
            Skew::new(-Duration::seconds(1)).unwrap_err() == AuthError::PolicyMisconfigured,
            "a skew refusal used the wrong variant"
        );
    }
}
