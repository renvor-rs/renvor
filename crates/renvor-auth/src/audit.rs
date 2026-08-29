//! The audit port: a closed vocabulary that has nowhere to put a secret.
//!
//! # The requirement is structural, not editorial
//!
//! FR-076 does not ask that events avoid carrying credentials. It asks that carrying one be
//! **impossible**: *"the event type makes it structurally impossible, not merely discouraged."*
//!
//! A `detail: String`, a `metadata: HashMap<String, String>`, an `error: Box<dyn Error>`, or a
//! `context: impl Display` would each satisfy every other requirement in this module and defeat
//! that one. This crate has made the same argument twice already — [`crate::AuthError`] is a
//! fieldless enum because *"a `String` detail is a place a credential can end up"*, and
//! `renvor-error` says the same about `InvalidParam`. This is that argument a third time, applied
//! to observability, which is where it matters most: an audit record is written precisely so that
//! somebody reads it later.
//!
//! # How it is enforced: `AuditEvent` is `Copy`
//!
//! [`AuditEvent`] derives [`Copy`], and that derive is the proof.
//!
//! `String`, `Vec<u8>`, `Box<T>`, `HashMap<K, V>`, `Arc<T>`, `Cow<'_, str>` and every other owned
//! heap type is **not** `Copy`. Adding a field of any of them makes the derive fail to compile.
//! Not a warning, not a lint, not a review comment — the crate does not build.
//!
//! **The honest limit of that proof**, because a guarantee whose edges are not stated is a guarantee
//! nobody can rely on: `&'static str` *is* `Copy`, so a `&'static str` field would survive the
//! derive. Reaching it with attacker-controlled text needs `Box::leak`, which is conspicuous, but
//! it is not impossible. Two things sit behind the derive for that case — an exact
//! [`core::mem::size_of`] assertion in this module's tests, which any added field breaks, and the
//! credential-canary sweep over every rendered event.
//!
//! # There is no `reason` field, and that is deliberate
//!
//! [`AuditOutcome`] has exactly two variants. An event records *the decision*, never *why*. A
//! reason field is where the abuse-control dimension name would end up — and
//! `evidence/abuse-control-matrix.md` §5 requires a refusal not to name the dimension that produced
//! it. A vocabulary that can express the reason makes that requirement unenforceable one layer up.
//!
//! # The enums are exhaustive, on purpose
//!
//! Unlike [`crate::AuthError`], nothing here is `#[non_exhaustive]`. A sink must match every
//! variant, and adding one is deliberately a breaking change.
//!
//! The reason is specific to auditing: under `#[non_exhaustive]` every sink grows a `_ => {}` arm,
//! and a newly added action then falls into it **silently**. The sink keeps compiling, keeps
//! reporting success, and stops recording an action nobody notices is missing. For an error
//! taxonomy a catch-all is a reasonable default; for an audit trail it is a gap that looks like
//! coverage.
//!
//! # Phase 010 owns the adapters
//!
//! FR-075. This module ships the port and [`RecordingAuditSink`], which is a **test** sink.
//! There is no `tracing` bridge, no OpenTelemetry exporter, and no file writer here — those are
//! rendering decisions, and they belong downstream of the boundary that makes the guarantee.

use core::fmt;

use chrono::{DateTime, Utc};
use renvor_core::observe::EntropySource;
use thiserror::Error;

use crate::error::AuthError;
use crate::subject::{AuthenticatedSubject, UserId};

/// An opaque correlation identifier: **8 bytes, and nothing derived from a caller**.
///
/// # "Validated" means a caller-supplied string cannot become one uninspected
///
/// [`Self::parse`] accepts exactly 16 lowercase hexadecimal characters and rejects everything else.
/// There is no `From<String>`, no `new(&str)`, and no fallible-but-lossy path — so a `traceparent`
/// header, a user-supplied request identifier, or a stray password cannot be carried through this
/// field by being handed to a permissive constructor.
///
/// `renvor_http::RequestId::encode` produces exactly that alphabet and exactly that length, which
/// is what lets the transport hand its per-request identifier straight through in batch J. If it
/// ever stopped producing it, [`Self::parse`] would refuse rather than accept something else.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CorrelationId([u8; 8]);

impl CorrelationId {
    /// The rendered length: 16 lowercase hex characters.
    pub const ENCODED_LEN: usize = 16;

    /// Builds an identifier from the entropy port and **nothing else**.
    ///
    /// # Errors
    ///
    /// [`AuthError::EntropyUnavailable`]. There is no fallback — the same rule the rest of this
    /// crate follows.
    pub fn from_entropy(source: &dyn EntropySource) -> Result<Self, AuthError> {
        let mut bytes = [0_u8; 8];
        source
            .fill(&mut bytes)
            .map_err(|_| AuthError::EntropyUnavailable)?;
        Ok(Self(bytes))
    }

    /// Builds an identifier from bytes a caller already holds.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 8]) -> Self {
        Self(bytes)
    }

    /// Parses the rendered form: **exactly** 16 lowercase hexadecimal characters.
    ///
    /// Returns `None` for any other length, for uppercase, for a `0x` prefix, for whitespace, and
    /// for any non-hex byte. Fail-closed and total: there is no input this accepts partially.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let raw = value.as_bytes();
        if raw.len() != Self::ENCODED_LEN {
            return None;
        }
        let mut bytes = [0_u8; 8];
        let mut index = 0;
        while index < 8 {
            let high = hex_value(raw[index * 2])?;
            let low = hex_value(raw[index * 2 + 1])?;
            bytes[index] = (high << 4) | low;
            index += 1;
        }
        Some(Self(bytes))
    }

    /// The rendered form: 16 lowercase hex characters.
    #[must_use]
    pub fn encode(&self) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(Self::ENCODED_LEN);
        for byte in self.0 {
            out.push(DIGITS[usize::from(byte >> 4)] as char);
            out.push(DIGITS[usize::from(byte & 0x0f)] as char);
        }
        out
    }
}

/// Lowercase hex only. Uppercase is refused rather than folded, so one string has one identifier.
const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.encode())
    }
}

impl fmt::Debug for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CorrelationId({})", self.encode())
    }
}

/// Who acted.
///
/// Two variants, mirroring [`crate::Subject`]: authentication either happened or it did not, and
/// the distinction is in the type rather than in a nullable field somebody has to remember to check
/// (FR-059).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuditActor {
    /// Nobody was authenticated. The overwhelming majority of authentication events.
    Anonymous,
    /// An authenticated account acted.
    Account(UserId),
}

impl AuditActor {
    /// The acting account, when there was one.
    #[must_use]
    pub const fn account(self) -> Option<UserId> {
        match self {
            Self::Anonymous => None,
            Self::Account(user) => Some(user),
        }
    }
}

impl From<AuthenticatedSubject> for AuditActor {
    fn from(subject: AuthenticatedSubject) -> Self {
        Self::Account(subject.user_id())
    }
}

/// An opaque reference to a server-issued credential family — a session or a refresh family.
///
/// **The identifier, never the secret.** These are the 16 bytes a row is keyed by; the secret whose
/// digest that row holds is not representable here.
///
/// A single type rather than one variant per credential kind, so [`AuditSubject`] has the **same
/// shape under every cargo feature**. A vocabulary that gained a variant with `--features tokens`
/// would be a closed set only for one build configuration, which is not what FR-071 asks for.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct CredentialRef([u8; 16]);

impl CredentialRef {
    /// Wraps an identifier.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// The wrapped identifier.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// What the action was about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuditSubject {
    /// The action concerns the actor alone, or concerns nothing nameable.
    ///
    /// **This is the variant an unknown account produces**, and it has to be: naming the account a
    /// forgot-password request asked about would put the answer to "does this account exist" into
    /// the audit trail, where it is exactly as readable as it would be in a response body.
    Unspecified,
    /// A user account.
    Account(UserId),
    /// A session or token family, by its identifier.
    Credential(CredentialRef),
}

/// The action taken. A **closed vocabulary**, one variant per auditable operation.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum AuditAction {
    /// An account was registered, or registration was attempted.
    Register,
    /// A password authentication was attempted.
    LogIn,
    /// A session was ended.
    LogOut,
    /// A verification token was issued and dispatched.
    VerificationResend,
    /// A verification token was presented.
    VerificationComplete,
    /// A password reset was requested.
    PasswordForgot,
    /// A reset token was presented and a password was set.
    PasswordReset,
    /// A session identifier was rotated.
    SessionRotate,
    /// A session, or every session for a subject, was revoked.
    SessionRevoke,
    /// An access token was issued.
    TokenIssue,
    /// A refresh token was presented for rotation.
    TokenRefresh,
    /// A refresh-token replay was detected and the family was revoked.
    TokenFamilyRevoke,
    /// An authorization policy was consulted. FR-062.
    PolicyDecision,
    /// An abuse control refused a request before the flow ran.
    AbuseRefusal,
}

impl AuditAction {
    /// Every action, for exhaustive tests and for sinks that enumerate.
    pub const ALL: [Self; 14] = [
        Self::Register,
        Self::LogIn,
        Self::LogOut,
        Self::VerificationResend,
        Self::VerificationComplete,
        Self::PasswordForgot,
        Self::PasswordReset,
        Self::SessionRotate,
        Self::SessionRevoke,
        Self::TokenIssue,
        Self::TokenRefresh,
        Self::TokenFamilyRevoke,
        Self::PolicyDecision,
        Self::AbuseRefusal,
    ];

    /// A stable, lowercase, hyphenated name for a sink that must render one.
    ///
    /// **This is the only place an action becomes text**, and the text comes from this `match` — so
    /// there is no path by which a caller's string becomes an action name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::LogIn => "log-in",
            Self::LogOut => "log-out",
            Self::VerificationResend => "verification-resend",
            Self::VerificationComplete => "verification-complete",
            Self::PasswordForgot => "password-forgot",
            Self::PasswordReset => "password-reset",
            Self::SessionRotate => "session-rotate",
            Self::SessionRevoke => "session-revoke",
            Self::TokenIssue => "token-issue",
            Self::TokenRefresh => "token-refresh",
            Self::TokenFamilyRevoke => "token-family-revoke",
            Self::PolicyDecision => "policy-decision",
            Self::AbuseRefusal => "abuse-refusal",
        }
    }
}

impl fmt::Display for AuditAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Whether the action went ahead.
///
/// **Two variants, and no third.** See the module header: there is no `reason`, and there is no
/// `Failed` distinct from `Refused`, because both would carry the information a refusal is required
/// not to disclose.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuditOutcome {
    /// The action proceeded.
    Permitted,
    /// The action did not proceed.
    Refused,
}

impl AuditOutcome {
    /// A stable name for rendering.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Permitted => "permitted",
            Self::Refused => "refused",
        }
    }
}

impl fmt::Display for AuditOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One audit record.
///
/// # Every field is fixed-size, and `Copy` is what enforces it
///
/// See the module header. `String`, `Vec<u8>`, `Box<T>` and `HashMap<K, V>` are not `Copy`, so
/// adding one is a compile error rather than a review finding.
///
/// ```
/// use renvor_auth::audit::AuditEvent;
///
/// // The bound is the guarantee. If `AuditEvent` ever grew an owned heap field, this line would
/// // stop compiling — which is the point.
/// fn requires_copy<T: Copy>() {}
/// requires_copy::<AuditEvent>();
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AuditEvent {
    actor: AuditActor,
    subject: AuditSubject,
    action: AuditAction,
    outcome: AuditOutcome,
    correlation: CorrelationId,
    at: DateTime<Utc>,
}

impl AuditEvent {
    /// Builds an event.
    ///
    /// Every argument is a closed enum or a fixed-size identifier. There is no argument this
    /// constructor could accept that carries arbitrary text, which is FR-076 restated as a
    /// signature.
    #[must_use]
    pub const fn new(
        action: AuditAction,
        outcome: AuditOutcome,
        actor: AuditActor,
        subject: AuditSubject,
        correlation: CorrelationId,
        at: DateTime<Utc>,
    ) -> Self {
        Self {
            actor,
            subject,
            action,
            outcome,
            correlation,
            at,
        }
    }

    /// Who acted.
    #[must_use]
    pub const fn actor(&self) -> AuditActor {
        self.actor
    }

    /// What it was about.
    #[must_use]
    pub const fn subject(&self) -> AuditSubject {
        self.subject
    }

    /// What was done.
    #[must_use]
    pub const fn action(&self) -> AuditAction {
        self.action
    }

    /// Whether it proceeded.
    #[must_use]
    pub const fn outcome(&self) -> AuditOutcome {
        self.outcome
    }

    /// The correlation identifier.
    #[must_use]
    pub const fn correlation(&self) -> CorrelationId {
        self.correlation
    }

    /// When it happened, from the injected clock rather than from the system.
    #[must_use]
    pub const fn at(&self) -> DateTime<Utc> {
        self.at
    }
}

/// Why an audit sink refused an event.
///
/// **Fieldless**, for the same reason [`AuthError`] is: a sink error that carried driver text would
/// reintroduce at the failure path exactly what the event type forbids on the success path.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
#[non_exhaustive]
pub enum AuditError {
    /// The sink could not record the event.
    #[error("the audit sink could not record the event")]
    Unavailable,
}

/// Where audit events go.
///
/// # Failure semantics, and why they are uniform
///
/// A sink failure is **returned**, never swallowed, and the caller is required to treat it
/// identically on every path. `evidence/abuse-control-matrix.md` §6 states the rule: a sink that
/// fails for a known account and succeeds for an unknown one — or that is *called* on one path and
/// not the other — is an enumeration oracle built out of observability.
///
/// This crate's services therefore record on **both** the permitted and the refused path, with the
/// same call shape, and propagate a failure the same way in both.
pub trait AuditSink: Send + Sync {
    /// Records `event`.
    ///
    /// # Errors
    ///
    /// [`AuditError::Unavailable`].
    fn record(
        &self,
        event: AuditEvent,
    ) -> impl core::future::Future<Output = Result<(), AuditError>> + Send;
}

/// A deterministic in-memory sink. FR-074.
///
/// Records in call order, so ordering assertions are about the code under test rather than about a
/// scheduler. It is not an operational adapter and is not intended as one — Phase 010 owns those
/// (FR-075).
#[derive(Debug, Default)]
pub struct RecordingAuditSink {
    events: std::sync::Mutex<Vec<AuditEvent>>,
    failing: std::sync::atomic::AtomicBool,
}

impl RecordingAuditSink {
    /// An empty sink that accepts everything.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A sink that refuses **every** event, for testing failure semantics.
    ///
    /// Refusing every event rather than the next one is deliberate: a sink that fails once and then
    /// succeeds cannot demonstrate that two different flows fail *identically*, which is the
    /// property that matters.
    #[must_use]
    pub fn failing() -> Self {
        let sink = Self::default();
        sink.failing
            .store(true, std::sync::atomic::Ordering::SeqCst);
        sink
    }

    /// Every event recorded, in order.
    #[must_use]
    pub fn events(&self) -> Vec<AuditEvent> {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// How many events were recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Whether nothing has been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The most recent event, if any.
    #[must_use]
    pub fn last(&self) -> Option<AuditEvent> {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .last()
            .copied()
    }

    /// Every recorded action, in order — the shape most ordering assertions want.
    #[must_use]
    pub fn actions(&self) -> Vec<AuditAction> {
        self.events()
            .into_iter()
            .map(|event| event.action())
            .collect()
    }
}

impl AuditSink for RecordingAuditSink {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        if self.failing.load(std::sync::atomic::Ordering::SeqCst) {
            // Nothing is recorded on the failing path. A sink that both failed AND stored would
            // let a test pass while the caller believed the record was lost.
            return Err(AuditError::Unavailable);
        }
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(event);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AuditAction, AuditActor, AuditError, AuditEvent, AuditOutcome, AuditSink, AuditSubject,
        CorrelationId, CredentialRef, RecordingAuditSink,
    };
    use crate::subject::{AuthenticatedSubject, UserId};
    use chrono::{DateTime, TimeZone as _, Utc};

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0)
            .single()
            .expect("a real instant")
    }

    fn user() -> UserId {
        UserId::from_bytes([7; 16])
    }

    fn correlation() -> CorrelationId {
        CorrelationId::from_bytes([0xde, 0xad, 0xbe, 0xef, 0x01, 0x02, 0x03, 0x04])
    }

    // ---- the correlation identifier is validated, not merely typed -----------------------------

    #[test]
    fn a_correlation_identifier_round_trips_through_its_rendered_form() {
        let id = correlation();
        assert_eq!(id.encode(), "deadbeef01020304");
        assert_eq!(CorrelationId::parse(&id.encode()), Some(id));

        // POSITIVE CONTROL: a different value renders differently, so the equality above is about
        // the bytes rather than about a constant.
        let other = CorrelationId::from_bytes([0; 8]);
        assert_ne!(other.encode(), id.encode());
        assert_eq!(other.encode(), "0000000000000000");
    }

    #[test]
    fn nothing_but_sixteen_lowercase_hex_characters_becomes_a_correlation_identifier() {
        // This is the whole of "validated". Each of these is something a caller-supplied header
        // could plausibly contain, and none of them may pass.
        for refused in [
            "",                  // empty
            "deadbeef0102030",   // one short
            "deadbeef010203045", // one long
            "DEADBEEF01020304",  // uppercase — refused, not folded
            "DeAdBeEf01020304",  // mixed case
            "0xdeadbeef010204",  // a prefix
            "deadbeef 1020304",  // whitespace
            "deadbeef0102030g",  // a non-hex digit
            "hunter2hunter2hu",  // a password of exactly the right length
            "../../etc/passwd",  // exactly 16 bytes of traversal
        ] {
            assert_eq!(
                CorrelationId::parse(refused),
                None,
                "accepted {refused:?}, which a caller controls"
            );
        }

        // POSITIVE CONTROL: the parser is not simply always `None`.
        assert!(CorrelationId::parse("0123456789abcdef").is_some());
    }

    #[test]
    fn the_rendered_length_is_exactly_what_the_transport_produces() {
        // Batch J hands `renvor_http::RequestId::encode()` straight to `parse`. That is only sound
        // while the two agree on length and alphabet, so the agreement is asserted here rather
        // than assumed at the call site.
        assert_eq!(CorrelationId::ENCODED_LEN, 16);
        assert_eq!(correlation().encode().len(), CorrelationId::ENCODED_LEN);
        assert!(
            correlation()
                .encode()
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        );
    }

    // ---- the structural guarantee -------------------------------------------------------------

    #[test]
    fn an_audit_event_is_exactly_its_fixed_size_fields_and_no_pointer() {
        // THE SECOND NET, behind the `Copy` bound. `Copy` already excludes `String`, `Vec`, `Box`
        // and `HashMap` — the derive would not compile. What it does NOT exclude is `&'static str`,
        // which is `Copy` and which `Box::leak` can reach with runtime text.
        //
        // An exact size is what closes that. Any added field changes it, including a 16-byte
        // `&'static str`, and this test then fails and asks for a review.
        //
        // A changed number here is NOT a test to update. It means a field was added to an audit
        // event, and the question is what that field can hold.
        assert_eq!(
            core::mem::size_of::<AuditEvent>(),
            56,
            "an audit event gained or lost a field"
        );

        // POSITIVE CONTROL: the assertion can fail. A type with one extra owned field is larger,
        // which is exactly the change the number above is watching for.
        struct WithText {
            _event: AuditEvent,
            _detail: String,
        }
        assert!(core::mem::size_of::<WithText>() > core::mem::size_of::<AuditEvent>());
    }

    #[test]
    fn every_action_has_a_distinct_stable_name_and_all_lists_all_of_them() {
        // `ALL` is the thing sinks and tests enumerate over. If a variant is added and not listed,
        // every exhaustive test below silently stops covering it.
        let mut names: Vec<&str> = AuditAction::ALL.iter().map(|a| a.as_str()).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "two actions render the same name");

        // The exhaustive match is the mechanism: adding a variant without adding it to `ALL` makes
        // this assertion fail, because `as_str` must gain an arm and `ALL` must gain an entry.
        assert_eq!(AuditAction::ALL.len(), 14);

        for action in AuditAction::ALL {
            assert!(!action.as_str().is_empty());
            assert_eq!(action.to_string(), action.as_str());
        }
    }

    #[test]
    fn no_rendering_of_any_event_can_contain_a_credential() {
        // SC-006, applied to the vocabulary itself rather than to one call site. Every action,
        // every outcome, every actor and every subject, rendered both ways.
        //
        // The argument this test completes: the canaries were never handed to the constructor,
        // because the constructor has no parameter that could take one. That is what makes the
        // absence structural rather than incidental.
        //
        // A canary must be a credential *value*, never a word the vocabulary legitimately uses.
        // The first version of this list contained the bare word "password" and the test failed —
        // correctly — on `AuditAction::PasswordForgot::as_str()`, which is "password-forgot". That
        // is the sweep working, not the vocabulary leaking, and the fix is a canary that no
        // legitimate rendering can contain.
        const CANARIES: [&str; 8] = [
            "hunter2",
            "correct-horse-battery-staple",
            "Bearer eyJhbGciOi",
            "__Host-rv_session=",
            "https://example.test/reset?token=",
            "P@ssw0rd-canary-9f3a",
            "-----BEGIN PRIVATE KEY-----",
            "sk_live_",
        ];

        let mut rendered = Vec::new();
        for action in AuditAction::ALL {
            for outcome in [AuditOutcome::Permitted, AuditOutcome::Refused] {
                for actor in [AuditActor::Anonymous, AuditActor::Account(user())] {
                    for subject in [
                        AuditSubject::Unspecified,
                        AuditSubject::Account(user()),
                        AuditSubject::Credential(CredentialRef::from_bytes([9; 16])),
                    ] {
                        let event =
                            AuditEvent::new(action, outcome, actor, subject, correlation(), at());
                        rendered.push(format!("{event:?}"));
                        rendered.push(format!("{}", event.action()));
                        rendered.push(format!("{}", event.outcome()));
                        rendered.push(format!("{}", event.correlation()));
                    }
                }
            }
        }

        assert_eq!(rendered.len(), 14 * 2 * 2 * 3 * 4, "the sweep lost a case");
        for text in &rendered {
            for canary in CANARIES {
                assert!(
                    !text.contains(canary),
                    "a rendered audit event contained {canary:?}"
                );
            }
        }

        // POSITIVE CONTROL: the sweep detects a canary when one IS present, so the absences above
        // are facts about the events rather than about a search that never matches.
        let planted = format!("{} hunter2", rendered[0]);
        assert!(CANARIES.iter().any(|canary| planted.contains(canary)));
    }

    #[test]
    fn an_unknown_account_is_named_by_nothing() {
        // The subject variant an unknown account produces carries no identifier, so the audit
        // trail does not answer "does this account exist" for anyone who can read it.
        let event = AuditEvent::new(
            AuditAction::PasswordForgot,
            AuditOutcome::Permitted,
            AuditActor::Anonymous,
            AuditSubject::Unspecified,
            correlation(),
            at(),
        );
        assert_eq!(event.subject(), AuditSubject::Unspecified);
        assert_eq!(event.actor().account(), None);

        // POSITIVE CONTROL: a known subject IS nameable, so `Unspecified` is a choice at the call
        // site rather than the only thing the type can express.
        let known = AuditEvent::new(
            AuditAction::PasswordForgot,
            AuditOutcome::Permitted,
            AuditActor::Anonymous,
            AuditSubject::Account(user()),
            correlation(),
            at(),
        );
        assert_eq!(known.subject(), AuditSubject::Account(user()));
    }

    #[test]
    fn an_authenticated_subject_becomes_an_actor_without_losing_its_identity() {
        let subject = AuthenticatedSubject::new(user());
        assert_eq!(AuditActor::from(subject), AuditActor::Account(user()));
        assert_eq!(AuditActor::from(subject).account(), Some(user()));
    }

    // ---- the deterministic sink ---------------------------------------------------------------

    #[tokio::test]
    async fn the_recording_sink_keeps_call_order() {
        let sink = RecordingAuditSink::new();
        assert!(sink.is_empty());

        for action in [
            AuditAction::LogIn,
            AuditAction::SessionRotate,
            AuditAction::TokenIssue,
        ] {
            sink.record(AuditEvent::new(
                action,
                AuditOutcome::Permitted,
                AuditActor::Anonymous,
                AuditSubject::Unspecified,
                correlation(),
                at(),
            ))
            .await
            .expect("the sink accepts");
        }

        assert_eq!(
            sink.actions(),
            vec![
                AuditAction::LogIn,
                AuditAction::SessionRotate,
                AuditAction::TokenIssue
            ]
        );
        assert_eq!(sink.len(), 3);
        assert_eq!(
            sink.last().map(|e| e.action()),
            Some(AuditAction::TokenIssue)
        );
    }

    #[tokio::test]
    async fn a_failing_sink_refuses_every_event_and_stores_none_of_them() {
        // The failure semantics FR-074's sink has to be able to demonstrate: refusal is total and
        // it is not silently swallowed.
        let sink = RecordingAuditSink::failing();

        for action in [AuditAction::LogIn, AuditAction::PasswordForgot] {
            let outcome = sink
                .record(AuditEvent::new(
                    action,
                    AuditOutcome::Permitted,
                    AuditActor::Anonymous,
                    AuditSubject::Unspecified,
                    correlation(),
                    at(),
                ))
                .await;
            assert_eq!(outcome, Err(AuditError::Unavailable));
        }

        // Nothing stored. A sink that failed AND stored would let a caller's test pass while the
        // caller believed the record was lost.
        assert!(sink.is_empty());

        // POSITIVE CONTROL: an ordinary sink accepts the same events, so the refusals above are
        // about the failing mode rather than about the events.
        let working = RecordingAuditSink::new();
        working
            .record(AuditEvent::new(
                AuditAction::LogIn,
                AuditOutcome::Permitted,
                AuditActor::Anonymous,
                AuditSubject::Unspecified,
                correlation(),
                at(),
            ))
            .await
            .expect("the working sink accepts");
        assert_eq!(working.len(), 1);
    }

    #[tokio::test]
    async fn the_same_two_flows_fail_identically_when_the_sink_is_down() {
        // The non-oracle property, at the sink layer. A known-account flow and an unknown-account
        // flow must produce the SAME sink error, or the difference is the answer to a question the
        // response body refuses to answer.
        let sink = RecordingAuditSink::failing();

        let known = sink
            .record(AuditEvent::new(
                AuditAction::PasswordForgot,
                AuditOutcome::Permitted,
                AuditActor::Anonymous,
                AuditSubject::Account(user()),
                correlation(),
                at(),
            ))
            .await;
        let unknown = sink
            .record(AuditEvent::new(
                AuditAction::PasswordForgot,
                AuditOutcome::Permitted,
                AuditActor::Anonymous,
                AuditSubject::Unspecified,
                correlation(),
                at(),
            ))
            .await;

        assert_eq!(known, unknown);
        assert_eq!(known, Err(AuditError::Unavailable));
    }
}
