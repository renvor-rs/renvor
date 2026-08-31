//! Password policy, Argon2id hashing, verification, and the rehash-on-login upgrade.
//!
//! # Every number here is cited, not chosen
//!
//! NIST SP 800-63B-4 §3.1.1.2, quoted where each is used:
//!
//! | Rule | Strength | Where |
//! |---|---|---|
//! | minimum **15**, single-factor | SHALL | [`PasswordPolicy::DEFAULT_MINIMUM`] |
//! | maximum at least **64** | SHOULD | [`PasswordPolicy::DEFAULT_MAXIMUM`] |
//! | **no** composition rules | SHALL NOT | there is no character-class check in this file |
//! | **no** periodic rotation | SHALL NOT | there is no expiry on a password hash |
//! | verify the **entire** password | SHALL | [`PasswordPolicy`] refuses, it never truncates |
//! | **NFC** normalization | SHOULD | [`normalise`] |
//! | blocklist the **entire** password | SHALL | [`PasswordBlocklist`] takes the whole candidate |
//!
//! Argon2id parameters come from RFC 9106 §4's **second** recommended option. The first specifies
//! `m=2^21` — **2 GiB per concurrent hash** — which is a deployment's choice and not a framework
//! default: ten simultaneous logins would want 20 GiB. The RFC calls the second *"a uniformly safe
//! option"*.
//!
//! # Two bugs this module was written with, on purpose, and then fixed
//!
//! Both are recorded because both are the *default* thing to write, and neither is visible without
//! a test that looks for it:
//!
//! 1. **Length counted in UTF-8 bytes.** `"éééééééé"` is 8 characters and 16 bytes, so a
//!    byte-counted 15-minimum admits an **8-character** password. The policy believes it bought 15
//!    characters of search space and bought 8.
//! 2. **No normalization.** `"é"` as `U+00E9` and as `e` + `U+0301` are different byte sequences.
//!    A user whose keyboard or password manager emits the decomposed form is locked out of the
//!    account they just created, and the failure looks like a wrong password.

use argon2::password_hash::{
    PasswordHash as PhcHash, PasswordHasher as _, PasswordVerifier as _, SaltString,
};
use argon2::{Algorithm, Argon2, Params, Version};
use renvor_core::observe::entropy::EntropySource;
use unicode_normalization::UnicodeNormalization as _;

use crate::error::AuthError;

/// Applies the normalization NIST §3.1.1.2 recommends.
///
/// **NFC, not NFKC.** NFC composes canonically equivalent sequences, so `e` + `U+0301` becomes
/// `U+00E9` and the two spellings of one character hash identically. NFKC additionally folds
/// *compatibility* characters — it would turn `ﬁ` into `fi` and a full-width digit into an ASCII
/// one — which silently changes what a password-manager-generated secret hashes to, and shrinks
/// the effective alphabet nobody agreed to shrink.
#[must_use]
pub fn normalise(password: &str) -> String {
    password.nfc().collect()
}

/// How long a password must be, and how long it may be.
///
/// **Length is the only rule.** There is deliberately no character-class check anywhere in this
/// file: §3.1.1.2 says verifiers *"SHALL NOT impose other composition rules"*, so the absence is
/// the requirement being met, not an omission.
#[derive(Clone, Copy, Debug)]
pub struct PasswordPolicy {
    minimum: usize,
    maximum: usize,
}

impl PasswordPolicy {
    /// §3.1.1.2: *"SHALL require passwords… used as a single-factor authentication mechanism to be
    /// a minimum of 15 characters in length."*
    pub const DEFAULT_MINIMUM: usize = 15;

    /// §3.1.1.2: *"SHOULD permit a maximum password length of at least 64 characters."*
    ///
    /// 256 rather than exactly 64 — the standard sets a floor, not a ceiling, and a longer
    /// passphrase should not be refused. It is **bounded** rather than unlimited because Argon2id
    /// hashing cost grows with input, so an unbounded field is a way to spend a server's memory
    /// from an unauthenticated endpoint.
    pub const DEFAULT_MAXIMUM: usize = 256;

    /// Builds a policy.
    ///
    /// # Errors
    ///
    /// [`AuthError::PasswordRejected`] if the bounds are weaker than the standard permits — a
    /// misconfiguration that silently lowered the floor would be worse than a refusal to start.
    pub const fn new(minimum: usize, maximum: usize) -> Result<Self, AuthError> {
        if minimum < Self::DEFAULT_MINIMUM || maximum < 64 || maximum < minimum {
            return Err(AuthError::PasswordRejected);
        }
        Ok(Self { minimum, maximum })
    }

    /// The configured minimum, in code points.
    #[must_use]
    pub const fn minimum(&self) -> usize {
        self.minimum
    }

    /// The configured maximum, in code points.
    #[must_use]
    pub const fn maximum(&self) -> usize {
        self.maximum
    }

    /// Checks a candidate's length, **in Unicode code points, after NFC**.
    ///
    /// **The unit is a `SHALL`, not a choice.** §3.1.1.2: *"Each Unicode code point SHALL be
    /// counted as a single character when evaluating password length."*
    ///
    /// Both ways of getting it wrong are conformance failures, in opposite directions:
    ///
    /// | Counted as | Effect |
    /// |---|---|
    /// | UTF-8 bytes | over-counts — wrongly **accepts** `"éééééééé"`, 8 code points, under a 15 floor |
    /// | grapheme clusters | under-counts — wrongly **rejects** a conforming password |
    ///
    /// **Normalising first is also load-bearing**, not tidiness: NFC composition can turn 15 code
    /// points into 14, so measuring before normalising can admit a password the `SHALL` forbids.
    ///
    /// # Errors
    ///
    /// [`AuthError::PasswordRejected`]. The error is **fieldless**, so the rejected password cannot
    /// travel with the refusal.
    pub fn admit(&self, candidate: &str) -> Result<(), AuthError> {
        let length = normalise(candidate).chars().count();
        if length < self.minimum || length > self.maximum {
            return Err(AuthError::PasswordRejected);
        }
        Ok(())
    }
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            minimum: Self::DEFAULT_MINIMUM,
            maximum: Self::DEFAULT_MAXIMUM,
        }
    }
}

/// A source of known-compromised and commonly-used passwords.
///
/// §3.1.1.2: *"The entire password SHALL be subject to comparison, not substrings or words that
/// might be contained therein."* The trait therefore takes the **whole candidate** and returns a
/// verdict — there is no API here that could accept a substring, so the requirement is met by the
/// shape rather than by the implementations remembering it.
pub trait PasswordBlocklist: Send + Sync + core::fmt::Debug {
    /// Whether the **complete** candidate is known-compromised or commonly used.
    fn contains(&self, candidate: &str) -> bool;
}

/// A blocklist backed by an in-memory list.
///
/// Offline by construction: there is no network call on the authentication path, so an outage in
/// somebody else's service cannot become an outage in registration.
#[derive(Clone, Debug, Default)]
pub struct StaticBlocklist {
    entries: Vec<String>,
}

impl StaticBlocklist {
    /// Builds a blocklist from `entries`, each normalised the same way a candidate will be.
    #[must_use]
    pub fn new(entries: impl IntoIterator<Item = String>) -> Self {
        Self {
            entries: entries.into_iter().map(|entry| normalise(&entry)).collect(),
        }
    }

    /// How many entries this list holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the list is empty — which would make every check vacuous.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl PasswordBlocklist for StaticBlocklist {
    fn contains(&self, candidate: &str) -> bool {
        let candidate = normalise(candidate);
        // `Vec::contains` is WHOLE-ELEMENT equality, not substring search — the method name is
        // exactly the word that invites the wrong reading, so it is spelled out here. §3.1.1.2
        // names substring matching as the thing not to do, and
        // `the_blocklist_compares_the_entire_password_and_not_substrings` fails if this ever
        // becomes `str::contains`.
        self.entries.contains(&candidate)
    }
}

/// A stored password hash, in PHC string format.
///
/// PHC because the string **carries its own parameters**, which is what makes FR-011's
/// rehash-on-login detectable per record: an application that raises its cost factor can tell,
/// from the stored value alone, which rows were written under the old one.
///
/// `Debug` is derived deliberately. A PHC string is a salt and a digest, not a credential — it is
/// what the database already holds, and hiding it would make a migration undiagnosable.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PasswordHash(String);

impl PasswordHash {
    /// The PHC string, for storage.
    #[must_use]
    pub fn as_phc(&self) -> &str {
        &self.0
    }

    /// Rebuilds a hash read from the database.
    #[must_use]
    pub fn from_phc(phc: impl Into<String>) -> Self {
        Self(phc.into())
    }
}

/// Argon2id parameters, recorded rather than implied.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Argon2idParameters {
    /// Memory cost, in kibibytes.
    pub memory_kib: u32,
    /// Time cost, in iterations.
    pub iterations: u32,
    /// Degree of parallelism, in lanes.
    pub lanes: u32,
}

impl Argon2idParameters {
    /// RFC 9106 §4's **second** recommended option: *"Argon2id with t=3 iterations, p=4 lanes,
    /// m=2^(16) (64 MiB of RAM), 128-bit salt, and 256-bit tag size."*
    ///
    /// # Measured, not copied
    ///
    /// FR-008 requires the parameters be benchmarked for a named deployment class rather than
    /// taken on the RFC's authority. Measured on **aarch64 macOS 26.3, release build**, by
    /// `benchmark::benchmark_the_recommended_parameter_sets`:
    ///
    /// | Option | Memory | Hash | Verify |
    /// |---|---|---|---|
    /// | **this one** | 64 MiB | **71.9 ms** | 67.8 ms |
    /// | [`Self::RFC_9106_FIRST`] | 2 GiB | **1.50 s** | 861 ms |
    ///
    /// That is the whole argument for the default. 1.5 seconds and 2 GiB **per concurrent login**
    /// is not something a framework can impose on every deployment; 72 ms is a login. Re-run the
    /// benchmark on the target hardware before trusting these numbers for it — they describe one
    /// machine, which is why the tool ships rather than only the result.
    pub const RFC_9106_SECOND: Self = Self {
        memory_kib: 65_536,
        iterations: 3,
        lanes: 4,
    };

    /// RFC 9106 §4's **first** recommended option: `m=2^21`, **2 GiB per concurrent hash**.
    ///
    /// Provided so a deployment that has measured its memory can choose it. **Not the default**:
    /// measured at **1.50 s per hash** and 2 GiB of resident memory on the machine described in
    /// [`Self::RFC_9106_SECOND`], so ten simultaneous logins would want 20 GiB and 15 seconds.
    pub const RFC_9106_FIRST: Self = Self {
        memory_kib: 2_097_152,
        iterations: 1,
        lanes: 4,
    };
}

/// Hashes and verifies passwords with Argon2id.
#[derive(Clone, Debug)]
pub struct PasswordService {
    parameters: Argon2idParameters,
}

impl Default for PasswordService {
    fn default() -> Self {
        Self::new(Argon2idParameters::RFC_9106_SECOND)
    }
}

impl PasswordService {
    /// Builds a service with `parameters`.
    #[must_use]
    pub const fn new(parameters: Argon2idParameters) -> Self {
        Self { parameters }
    }

    /// The parameters in force, so a deployment can record what it actually ran.
    #[must_use]
    pub const fn parameters(&self) -> Argon2idParameters {
        self.parameters
    }

    fn argon2(&self) -> Result<Argon2<'_>, AuthError> {
        let params = Params::new(
            self.parameters.memory_kib,
            self.parameters.iterations,
            self.parameters.lanes,
            None,
        )
        .map_err(|_| AuthError::PasswordRejected)?;
        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
    }

    /// Hashes `password`, normalising it first.
    ///
    /// The salt comes from the **entropy port**, not from `argon2`'s own `rand` feature, so this
    /// crate keeps one randomness site rather than two.
    ///
    /// # Errors
    ///
    /// [`AuthError::EntropyUnavailable`] if a salt cannot be generated — there is no fallback —
    /// or [`AuthError::PasswordRejected`] if hashing fails.
    pub fn hash(
        &self,
        password: &str,
        source: &dyn EntropySource,
    ) -> Result<PasswordHash, AuthError> {
        // RFC 9106 §4 specifies a 128-bit salt. NIST §3.1.1.2 requires at least 32 bits, so this
        // is four times the floor.
        let mut salt_bytes = [0_u8; 16];
        source
            .fill(&mut salt_bytes)
            .map_err(|_| AuthError::EntropyUnavailable)?;
        let salt = SaltString::encode_b64(&salt_bytes).map_err(|_| AuthError::PasswordRejected)?;

        let normalised = normalise(password);
        let hash = self
            .argon2()?
            .hash_password(normalised.as_bytes(), &salt)
            .map_err(|_| AuthError::PasswordRejected)?;
        Ok(PasswordHash(hash.to_string()))
    }

    /// Verifies `password` against `stored`, normalising it the same way [`Self::hash`] did.
    ///
    /// Returns `false` for a malformed stored hash rather than erroring: a corrupt row is an
    /// authentication failure, and distinguishing it for the caller would be an oracle.
    #[must_use]
    pub fn verify(&self, password: &str, stored: &PasswordHash) -> bool {
        let Ok(parsed) = PhcHash::new(stored.as_phc()) else {
            return false;
        };
        let Ok(argon2) = self.argon2() else {
            return false;
        };
        let normalised = normalise(password);
        argon2
            .verify_password(normalised.as_bytes(), &parsed)
            .is_ok()
    }

    /// Whether `stored` was written under parameters weaker than the ones now in force.
    ///
    /// Read from the **PHC string itself**, which is why the format matters: the answer is a fact
    /// about the row, not about when the application was deployed.
    #[must_use]
    pub fn needs_rehash(&self, stored: &PasswordHash) -> bool {
        let Ok(parsed) = PhcHash::new(stored.as_phc()) else {
            // Unparseable means it cannot be shown to meet the current parameters, so it is
            // upgraded on the next successful login. Fail toward the stronger outcome.
            return true;
        };
        let Ok(params) = Params::try_from(&parsed) else {
            return true;
        };
        params.m_cost() < self.parameters.memory_kib
            || params.t_cost() < self.parameters.iterations
            || params.p_cost() < self.parameters.lanes
    }

    /// Verifies against the stored hash when the account exists, and against a **dummy hash** when
    /// it does not.
    ///
    /// # Why this is one function and not two
    ///
    /// FR-012 wants an unknown account to cost what a known one costs. The obvious shape —
    /// `if let Some(hash) = stored { verify(hash) } else { verify(dummy); false }` — has two arms,
    /// and two arms is what a later refactor collapses into an early return. Selecting the hash
    /// **before** the single call site leaves no arm to remove: there is one `verify` here, and it
    /// always runs.
    ///
    /// Measured on this machine (see [`Argon2idParameters::RFC_9106_SECOND`]), a skipped
    /// verification is **68 ms** cheaper. That is not a difference an attacker needs statistics to
    /// see, which is why the structure carries the guarantee rather than a comment asking callers
    /// to be careful.
    ///
    /// `&` rather than `&&`, deliberately: the non-short-circuiting operator cannot skip its right
    /// operand, so the result does not depend on evaluation order.
    ///
    /// # This is not proven by a timing test
    ///
    /// It could only be, and a timing-equality assertion is flaky by construction — it would fail
    /// on a loaded machine and teach a team to re-run gates rather than trust them. The guarantee
    /// is **structural**, and mutation `B-M8` is recorded as a **surviving** mutation for exactly
    /// that reason rather than paired with a test that would eventually lie.
    #[must_use]
    pub fn verify_against_stored_or_dummy(
        &self,
        password: &str,
        stored: Option<&PasswordHash>,
        dummy: &PasswordHash,
    ) -> bool {
        let present = stored.is_some();
        let hash = stored.unwrap_or(dummy);
        let verified = self.verify(password, hash);
        // An account that does not exist never authenticates, WHATEVER the dummy holds. Without
        // this, anyone who learned the dummy's password could log in as any nonexistent user.
        verified & present
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Argon2idParameters, PasswordBlocklist, PasswordHash, PasswordPolicy, PasswordService,
        StaticBlocklist, normalise,
    };
    use renvor_core::observe::entropy::FixedEntropy;

    fn entropy() -> FixedEntropy {
        FixedEntropy::new(vec![0x11, 0x22, 0x33, 0x44])
    }

    /// A deliberately cheap parameter set, so the suite does not spend 64 MiB per hash.
    ///
    /// The parameters under test are asserted separately by
    /// [`the_default_parameters_are_rfc_9106s_second_option`], so speeding these up does not
    /// weaken the claim about what ships.
    fn fast() -> PasswordService {
        PasswordService::new(Argon2idParameters {
            memory_kib: 8,
            iterations: 1,
            lanes: 1,
        })
    }

    #[test]
    fn length_is_counted_in_code_points_not_bytes() {
        // THE BUG THIS TEST EXISTS FOR, AND IT FAILED HERE FIRST. NIST §3.1.1.2 requires a
        // 15-character minimum; counting UTF-8 BYTES admits a password of EIGHT characters,
        // because each two-byte character counts double.
        let policy = PasswordPolicy::default();

        let eight_two_byte_characters = "éééééééé";
        assert_eq!(eight_two_byte_characters.chars().count(), 8);
        assert_eq!(eight_two_byte_characters.len(), 16);
        assert!(
            policy.admit(eight_two_byte_characters).is_err(),
            "an 8-character password was admitted because its BYTE length reached 15"
        );

        // POSITIVE CONTROL: 15 code points IS admitted, so the refusal is about the count rather
        // than about a policy that refuses everything.
        assert!(policy.admit("correct horse b").is_ok());
    }

    #[test]
    fn a_decomposed_password_verifies_against_its_composed_form() {
        // ALSO FAILED HERE FIRST. Without NFC, a user whose keyboard or password manager emits the
        // decomposed form is locked out of the account they just created, and it looks to them
        // like a wrong password.
        let service = fast();
        let composed = "caf\u{e9} correct horse";
        let decomposed = "cafe\u{301} correct horse";
        assert_ne!(
            composed, decomposed,
            "the forms must differ as bytes, or this proves nothing"
        );

        let stored = service
            .hash(composed, &entropy())
            .expect("hashing succeeds");
        assert!(service.verify(decomposed, &stored));
    }

    #[test]
    fn a_password_registered_in_decomposed_form_verifies_in_composed_form() {
        // THE OTHER DIRECTION, AND A MUTATION FOUND IT MISSING.
        //
        // `a_decomposed_password_verifies_against_its_composed_form` hashes the COMPOSED form,
        // where normalising is a no-op — so deleting the normalisation from `hash` left it green.
        // Mutation B-M2 survived on exactly that.
        //
        // This is the case that actually strands a user: they REGISTER with the decomposed form,
        // the stored hash is of decomposed bytes, and every later login normalises to composed and
        // fails. The account is unreachable and the error says "wrong password".
        let service = fast();
        let composed = "caf\u{e9} correct horse";
        let decomposed = "cafe\u{301} correct horse";

        let stored = service
            .hash(decomposed, &entropy())
            .expect("hashing succeeds");
        assert!(
            service.verify(composed, &stored),
            "a password registered in decomposed form must verify in composed form"
        );
        assert!(
            service.verify(decomposed, &stored),
            "...and in the form it was registered with"
        );
    }

    #[test]
    fn normalisation_is_nfc_and_not_nfkc() {
        // NFKC would fold compatibility characters, changing what a password-manager-generated
        // secret hashes to and shrinking an alphabet nobody agreed to shrink.
        assert_eq!(normalise("cafe\u{301}"), "caf\u{e9}", "NFC must compose");
        assert_eq!(
            normalise("\u{fb01}"),
            "\u{fb01}",
            "NFC must NOT fold the ﬁ ligature; NFKC would"
        );
        assert_eq!(
            normalise("\u{ff11}"),
            "\u{ff11}",
            "NFC must NOT fold a full-width digit"
        );
    }

    #[test]
    fn a_wrong_password_does_not_verify() {
        // POSITIVE CONTROL: verification is not simply returning true.
        let service = fast();
        let stored = service
            .hash("correct horse battery", &entropy())
            .expect("hashed");
        assert!(!service.verify("incorrect horse battery", &stored));
    }

    #[test]
    fn no_composition_rule_is_imposed() {
        // §3.1.1.2: "SHALL NOT impose other composition rules". A long all-lowercase passphrase
        // with no digit and no symbol is exactly what the standard wants accepted.
        let policy = PasswordPolicy::default();
        assert!(policy.admit("correct horse battery staple").is_ok());
        assert!(
            policy.admit("aaaaaaaaaaaaaaaa").is_ok(),
            "repetition is not a composition rule"
        );
    }

    #[test]
    fn spaces_are_accepted_anywhere_including_the_ends() {
        // §3.1.1.2 requires the space character to be accepted. Trimming input is the common way
        // this breaks: a password manager may emit a trailing space and the user cannot see it.
        let policy = PasswordPolicy::default();
        let padded = "  correct horse battery  ";
        assert!(policy.admit(padded).is_ok());

        let service = fast();
        let stored = service.hash(padded, &entropy()).expect("hashed");
        assert!(
            service.verify(padded, &stored),
            "the padded form must verify"
        );
        assert!(
            !service.verify(padded.trim(), &stored),
            "trimming must NOT happen silently — the trimmed form is a different password"
        );
    }

    #[test]
    fn the_entire_password_is_verified_and_never_truncated() {
        // §3.1.1.2: "SHALL verify the entire submitted password (e.g., not truncate it)."
        // Two passwords that agree for 72 bytes and differ after it. 72 is not arbitrary: it is
        // bcrypt's truncation point, and the classic version of this bug.
        let service = fast();
        let base = "a".repeat(72);
        let first = format!("{base}first");
        let second = format!("{base}second");

        let stored = service.hash(&first, &entropy()).expect("hashed");
        assert!(service.verify(&first, &stored));
        assert!(
            !service.verify(&second, &stored),
            "two passwords differing only past byte 72 verified against the same hash"
        );
    }

    #[test]
    fn the_maximum_is_enforced_as_a_refusal_and_not_a_truncation() {
        let policy = PasswordPolicy::default();
        let too_long = "a".repeat(PasswordPolicy::DEFAULT_MAXIMUM + 1);
        assert!(policy.admit(&too_long).is_err());
        // POSITIVE CONTROL: exactly the maximum is admitted.
        assert!(
            policy
                .admit(&"a".repeat(PasswordPolicy::DEFAULT_MAXIMUM))
                .is_ok()
        );
    }

    #[test]
    fn a_policy_weaker_than_the_standard_is_refused_at_construction() {
        // A misconfiguration that silently lowered the floor would be worse than a refusal to
        // start, and there is no silent fallback in this project.
        assert!(PasswordPolicy::new(8, 256).is_err(), "below the 15 SHALL");
        assert!(PasswordPolicy::new(15, 32).is_err(), "below the 64 SHOULD");
        assert!(PasswordPolicy::new(100, 50).is_err(), "inverted bounds");
        // POSITIVE CONTROL.
        assert!(PasswordPolicy::new(15, 64).is_ok());
        assert!(PasswordPolicy::new(20, 1024).is_ok());
    }

    #[test]
    fn the_blocklist_compares_the_entire_password_and_not_substrings() {
        // §3.1.1.2: "The entire password SHALL be subject to comparison, not substrings or words
        // that might be contained therein."
        let list = StaticBlocklist::new(["password".to_owned(), "correct horse".to_owned()]);
        assert!(list.contains("password"), "an exact entry must match");
        assert!(
            !list.contains("password battery staple horse"),
            "a candidate CONTAINING a listed word must NOT be refused — that is substring matching"
        );
        assert!(!list.contains("passwor"), "a prefix must not match");
        assert!(
            !list.is_empty(),
            "an empty list would make every check vacuous"
        );
    }

    #[test]
    fn the_blocklist_normalises_both_sides() {
        // Otherwise a listed entry could be evaded by typing its decomposed form.
        let list = StaticBlocklist::new(["caf\u{e9} password".to_owned()]);
        assert!(list.contains("cafe\u{301} password"));
    }

    #[test]
    fn an_obsolete_stored_hash_is_detected_for_rehash() {
        // FR-011. The answer comes from the PHC string itself, so it is a fact about the ROW.
        let weak = fast();
        let stored = weak
            .hash("correct horse battery", &entropy())
            .expect("hashed");

        let strong = PasswordService::new(Argon2idParameters {
            memory_kib: 64,
            iterations: 3,
            lanes: 1,
        });
        assert!(
            strong.needs_rehash(&stored),
            "a weaker stored hash must be flagged"
        );
        assert!(
            !weak.needs_rehash(&stored),
            "a hash at current parameters must NOT be flagged"
        );
    }

    #[test]
    fn an_unparseable_stored_hash_is_flagged_for_rehash() {
        // Fail toward the stronger outcome: what cannot be shown to meet current parameters is
        // upgraded on the next successful login.
        assert!(fast().needs_rehash(&PasswordHash::from_phc("not a phc string")));
    }

    #[test]
    fn an_absent_account_never_authenticates_even_with_the_dummys_own_password() {
        // THE DANGEROUS HALF, and it IS behaviourally testable. If the result were the raw
        // verification outcome, anyone who learned the dummy's password could authenticate as any
        // user that does not exist — a login that succeeds for accounts nobody created.
        let service = fast();
        let dummy = service
            .hash("a dummy password value", &entropy())
            .expect("hashed");

        assert!(
            !service.verify_against_stored_or_dummy("anything at all", None, &dummy),
            "an absent account must not authenticate"
        );
        assert!(
            !service.verify_against_stored_or_dummy("a dummy password value", None, &dummy),
            "THE DUMMY'S OWN PASSWORD must not authenticate an absent account"
        );
    }

    #[test]
    fn a_present_account_still_authenticates_through_the_same_path() {
        // POSITIVE CONTROL. Without it, a function returning `false` unconditionally would satisfy
        // the test above while breaking every login.
        let service = fast();
        let dummy = service
            .hash("a dummy password value", &entropy())
            .expect("hashed");
        let stored = service
            .hash("correct horse battery", &entropy())
            .expect("hashed");

        assert!(service.verify_against_stored_or_dummy(
            "correct horse battery",
            Some(&stored),
            &dummy
        ));
        assert!(!service.verify_against_stored_or_dummy(
            "wrong horse battery",
            Some(&stored),
            &dummy
        ));
    }

    #[test]
    fn the_stored_form_is_a_phc_string_carrying_its_parameters() {
        let service = fast();
        let phc = service
            .hash("correct horse battery", &entropy())
            .expect("hashed");
        let text = phc.as_phc();
        assert!(
            text.starts_with("$argon2id$"),
            "must be Argon2id in PHC form: {text}"
        );
        assert!(text.contains("$v=19$"), "must record the version: {text}");
        assert!(
            text.contains("m=8,t=1,p=1"),
            "must record the parameters: {text}"
        );
    }

    #[test]
    fn a_different_salt_yields_a_different_hash_for_the_same_password() {
        // The salt comes from the entropy port, so two registrations of the same password do not
        // collide in the database — which is what makes a stolen table non-sortable.
        let service = fast();
        let first = service
            .hash("correct horse battery", &entropy())
            .expect("hashed");
        let second = service
            .hash(
                "correct horse battery",
                &FixedEntropy::new(vec![0x99, 0x88]),
            )
            .expect("hashed");
        assert_ne!(
            first, second,
            "different salts must yield different stored values"
        );
        assert!(service.verify("correct horse battery", &first));
        assert!(service.verify("correct horse battery", &second));
    }

    #[test]
    fn the_default_parameters_are_rfc_9106s_second_option() {
        // The claim the other tests' cheap parameters must not be allowed to weaken.
        let parameters = PasswordService::default().parameters();
        assert_eq!(parameters, Argon2idParameters::RFC_9106_SECOND);
        assert_eq!(parameters.memory_kib, 65_536, "m=2^16, 64 MiB");
        assert_eq!(parameters.iterations, 3, "t=3");
        assert_eq!(parameters.lanes, 4, "p=4");
        // ...and the first option is available but is NOT the default: 2 GiB per concurrent hash.
        assert_eq!(Argon2idParameters::RFC_9106_FIRST.memory_kib, 2_097_152);
        assert_ne!(parameters, Argon2idParameters::RFC_9106_FIRST);
    }
}

/// The parameter benchmark FR-008 requires.
///
/// **Ignored by default, and it is a measurement rather than an assertion.** A test that asserted
/// "hashing takes under N milliseconds" would fail on a loaded machine and teach a team to re-run
/// gates instead of trusting them — the failure mode this project has already recorded once, in
/// finding F-3. So this measures and prints; a human reads the number and records it for their
/// deployment class.
///
/// Run it, in **release** — a debug build's Argon2id is several times slower and its number means
/// nothing:
///
/// ```text
/// cargo test -p renvor-auth --release -- --ignored --nocapture benchmark
/// ```
#[cfg(test)]
mod benchmark {
    use super::{Argon2idParameters, PasswordService};
    use renvor_core::observe::entropy::OsEntropy;

    #[test]
    #[ignore = "a measurement, not an assertion; run in release with --ignored --nocapture"]
    fn benchmark_the_recommended_parameter_sets() {
        let password = "correct horse battery staple";
        let source = OsEntropy::new();

        for (label, parameters) in [
            (
                "RFC 9106 SECOND (default)",
                Argon2idParameters::RFC_9106_SECOND,
            ),
            ("RFC 9106 FIRST (2 GiB)", Argon2idParameters::RFC_9106_FIRST),
        ] {
            let service = PasswordService::new(parameters);
            let started = std::time::Instant::now();
            let Ok(stored) = service.hash(password, &source) else {
                println!("{label}: COULD NOT HASH — parameters rejected on this machine");
                continue;
            };
            let hashed = started.elapsed();

            let started = std::time::Instant::now();
            let verified = service.verify(password, &stored);
            let verify_time = started.elapsed();

            println!(
                "{label}: m={} KiB t={} p={} | hash {hashed:?} | verify {verify_time:?} | ok={verified}",
                parameters.memory_kib, parameters.iterations, parameters.lanes
            );
        }
    }
}
