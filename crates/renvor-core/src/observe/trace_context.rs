//! W3C Trace Context: a total, fail-closed parser for `traceparent` and `tracestate`.
//!
//! # These headers are attacker-controlled input, and the parser is written that way
//!
//! Anyone who can reach an HTTP endpoint can send a `traceparent`. The W3C Recommendation says
//! what a receiver must do with a bad one — **ignore it** — and names why: restarting a trace at a
//! front gate *"eliminates a potential denial-of-service attack surface"* (§3.4). So this parser
//! has exactly two outcomes, a context or a closed rejection, and no input can make it allocate
//! beyond the fixed bounds, panic, or carry bytes it did not validate.
//!
//! The rules, each with the section it comes from, are asserted one by one in the tests below and
//! fuzzed in `tests/trace_context.rs`:
//!
//! | Rule | Section |
//! |---|---|
//! | `version "-" trace-id "-" parent-id "-" trace-flags`, lowercase hex only | §3.2.2, §3.2.2.2 |
//! | version `ff` is invalid; this implementation accepts version `00` | §3.2.2.1 |
//! | a 32-hex trace-id of all zeroes is invalid → ignore the header | §3.2.2.3 |
//! | a 16-hex parent-id of all zeroes is invalid → ignore the header | §3.2.2.4 |
//! | `trace-flags` is a bit field: mask, never compare | §3.2.2.5 |
//! | outbound `trace-flags` carry only the bit this version defines; every other bit is set to zero | §3.2.2.5.2, §4.3 |
//! | a `traceparent` that fails to parse means `tracestate` is not parsed | §3.3 |
//! | a `tracestate` that fails to parse does **not** affect `traceparent` | §3.3 |
//! | at most 32 list-members; keys and values have closed grammars; ≤ 512 bytes propagated | §3.3.1.1–§3.3.1.5 |
//! | a simple key and a `system-id` begin with a lowercase letter; only a `tenant-id` may begin with a digit | §3.3.1.3.1 |
//! | a key appears at most once in a `tracestate` | §3.3.1.4 |
//!
//! # What this module does not do
//!
//! It does not touch the request identifier. `RequestId` keeps its single entropy-only
//! constructor (ADR-0012 Finding 1); an inbound trace context is *recorded beside* it, never
//! adopted as it. And it does not propagate `tracestate` onward: Renvor's only outbound HTTP is
//! the OTLP exporter (C-O15), which carries no request trace context, so a validated `tracestate`
//! reaches a span field's member count and the handler through `Request::trace_context`, and
//! §4.3 permits a receiver to drop it.

use core::fmt;

/// The exact length of a version-00 `traceparent` value: `00-` + 32 + `-` + 16 + `-` + 2.
pub const TRACEPARENT_LEN: usize = 55;

/// The most bytes of a combined `tracestate` value this implementation accepts (§3.3.1.5's
/// *"SHOULD propagate at least 512"* taken as the ceiling, so the bound is also the guarantee).
pub const MAX_TRACESTATE_BYTES: usize = 512;

/// The most list-members a `tracestate` may carry (§3.3.1.1).
pub const MAX_TRACESTATE_MEMBERS: usize = 32;

/// The most bytes in one `tracestate` key or value (§3.3.1.3).
const MAX_MEMBER_PART: usize = 256;

/// The `sampled` bit of `trace-flags` (§3.2.2.5.1).
const FLAG_SAMPLED: u8 = 0b0000_0001;

/// Why a `traceparent` was ignored.
///
/// **Fieldless.** A rejection that carried the offending bytes would be a way for a caller to
/// smuggle text into a log through the one field a diagnostic is likely to print. The reason is
/// enough for a metric and an operator; the bytes are never wanted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TraceContextRejection {
    /// Not exactly 55 bytes.
    Length,
    /// A delimiter was not `-` where §3.2.2 puts one.
    Delimiter,
    /// The version was not `00` (which includes the forbidden `ff`).
    Version,
    /// The trace-id was not 32 lowercase hex characters.
    TraceIdNotHex,
    /// The trace-id was all zeroes.
    TraceIdZero,
    /// The parent-id was not 16 lowercase hex characters.
    ParentIdNotHex,
    /// The parent-id was all zeroes.
    ParentIdZero,
    /// The trace-flags were not 2 lowercase hex characters.
    FlagsNotHex,
}

impl TraceContextRejection {
    /// A stable label for a metric or a structured field.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Length => "length",
            Self::Delimiter => "delimiter",
            Self::Version => "version",
            Self::TraceIdNotHex => "trace_id_not_hex",
            Self::TraceIdZero => "trace_id_zero",
            Self::ParentIdNotHex => "parent_id_not_hex",
            Self::ParentIdZero => "parent_id_zero",
            Self::FlagsNotHex => "flags_not_hex",
        }
    }
}

impl fmt::Display for TraceContextRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A 16-byte trace identifier. Never all zeroes (§3.2.2.3).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraceId([u8; 16]);

impl TraceId {
    /// Builds a trace identifier from bytes a caller already holds.
    ///
    /// Returns `None` for all zeroes, so the one invalid value cannot be constructed.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 16]) -> Option<Self> {
        (bytes != [0; 16]).then_some(Self(bytes))
    }

    /// The raw bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// The 32 lowercase hex characters.
    #[must_use]
    pub fn encode(&self) -> String {
        hex(&self.0)
    }
}

impl fmt::Display for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.encode())
    }
}

impl fmt::Debug for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TraceId({})", self.encode())
    }
}

/// An 8-byte span identifier. Never all zeroes (§3.2.2.4).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpanId([u8; 8]);

impl SpanId {
    /// Builds a span identifier from bytes a caller already holds; `None` for all zeroes.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 8]) -> Option<Self> {
        (bytes != [0; 8]).then_some(Self(bytes))
    }

    /// The raw bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }

    /// The 16 lowercase hex characters.
    #[must_use]
    pub fn encode(&self) -> String {
        hex(&self.0)
    }
}

impl fmt::Display for SpanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.encode())
    }
}

impl fmt::Debug for SpanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SpanId({})", self.encode())
    }
}

/// The 8-bit `trace-flags` field.
///
/// Held as the byte, because §3.2.2.5 is explicit that *"you cannot interpret flags by decoding
/// the hex value and looking at the resulting number"* — only masked bits mean anything, and this
/// version of the specification defines one. Inbound, the byte is kept whole
/// ([`TraceFlags::as_byte`]) so the record says what was received; outbound, only the defined bit
/// leaves ([`TraceFlags::supported`]; §3.2.2.5.2 and §4.3).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraceFlags(u8);

impl TraceFlags {
    /// The flags with only `sampled` set.
    pub const SAMPLED: Self = Self(FLAG_SAMPLED);
    /// No flags set.
    pub const NONE: Self = Self(0);
    /// The bits this version of the specification defines (§3.2.2.5.1): only `sampled`. What
    /// [`TraceFlags::supported`] keeps and [`TraceContext::render_traceparent`] emits.
    pub const SUPPORTED: Self = Self(FLAG_SAMPLED);

    /// Builds flags from the raw byte. Every bit pattern is representable; only `sampled` is
    /// interpreted.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Self {
        Self(byte)
    }

    /// The raw byte **as received**, undefined bits included: the inbound record. Not what
    /// leaves — [`TraceContext::render_traceparent`] renders [`TraceFlags::supported`].
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        self.0
    }

    /// Whether the caller recorded its side of the trace (§3.2.2.5.1) — **masked**, never compared.
    #[must_use]
    pub const fn sampled(self) -> bool {
        self.0 & FLAG_SAMPLED != 0
    }

    /// The flags with every bit this version of the specification does not define set to zero:
    /// what leaves Renvor.
    ///
    /// §3.2.2.5.2: *"The behavior of other flags … is not defined and is reserved for future use.
    /// Vendors MUST set those to zero."* §4.3: *"Vendors will set all unparsed / unknown
    /// trace-flags to 0 on outgoing requests."* [`TraceContext::render_traceparent`] renders this;
    /// [`TraceFlags::as_byte`] keeps the received byte for the inbound record.
    #[must_use]
    pub const fn supported(self) -> Self {
        Self(self.0 & Self::SUPPORTED.0)
    }

    /// The 2 lowercase hex characters.
    #[must_use]
    pub fn encode(self) -> String {
        hex(&[self.0])
    }
}

impl fmt::Debug for TraceFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TraceFlags({})", self.encode())
    }
}

/// A validated, bounded `tracestate` value.
///
/// Kept as the canonical combined string (members joined by `,` with no optional whitespace), at
/// most [`MAX_TRACESTATE_BYTES`] long. An invalid or oversized header does not produce one; it is
/// dropped, which §3.3 and §4.3 permit, and never affects the `traceparent` verdict.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TraceState {
    canonical: String,
    members: usize,
}

impl TraceState {
    /// Parses a combined header value. Returns `None` for anything that fails §3.3.1, a repeated
    /// key included (§3.3.1.4).
    ///
    /// Multiple header fields must be combined by the caller with `,` before this is called
    /// (§3.3.1.1); this function sees one value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        if value.len() > MAX_TRACESTATE_BYTES {
            return None;
        }
        // Both vectors are bounded by the member cap, checked before every push, so the duplicate
        // scan below is at most 32 × 32 comparisons and no input grows either.
        let mut members: Vec<&str> = Vec::new();
        let mut keys: Vec<&str> = Vec::new();
        for raw in value.split(',') {
            // OWS around a list-member is ignored (§3.3.1.1); empty members are allowed.
            let member = raw.trim_matches([' ', '\t']);
            if member.is_empty() {
                continue;
            }
            let (key, member_value) = member.split_once('=')?;
            if !valid_key(key) || !valid_value(member_value) {
                return None;
            }
            // §3.3.1.4: "Only one entry per key is allowed". Exact byte match; keys are lowercase
            // by grammar, so there is no other equality. Empty members were skipped above and
            // cannot hide a repeat (Phase 010 correction round, Finding 4).
            if keys.contains(&key) {
                return None;
            }
            if members.len() == MAX_TRACESTATE_MEMBERS {
                return None;
            }
            keys.push(key);
            members.push(member);
        }
        Some(Self {
            canonical: members.join(","),
            members: members.len(),
        })
    }

    /// The canonical combined value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// How many list-members the value carries.
    #[must_use]
    pub const fn members(&self) -> usize {
        self.members
    }
}

/// Renders the **member count**, never the members: a `tracestate` value is opaque vendor text
/// a caller chose, and a `Debug` that printed it would be the smuggling channel this module
/// exists to close.
impl fmt::Debug for TraceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TraceState")
            .field("members", &self.members)
            .finish_non_exhaustive()
    }
}

/// A validated inbound trace context: what a `traceparent` (and optionally a `tracestate`) said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceContext {
    trace_id: TraceId,
    parent_id: SpanId,
    flags: TraceFlags,
    state: Option<TraceState>,
}

impl TraceContext {
    /// Builds a context from parts a caller already validated.
    #[must_use]
    pub const fn new(trace_id: TraceId, parent_id: SpanId, flags: TraceFlags) -> Self {
        Self {
            trace_id,
            parent_id,
            flags,
            state: None,
        }
    }

    /// Parses a `traceparent` value, and a `tracestate` value if one was sent.
    ///
    /// `tracestate` is looked at only when `traceparent` parsed (§3.3), and a `tracestate` that
    /// fails is dropped without affecting the result (§3.3). The `traceparent` rules are total:
    /// every byte is checked and the header is ignored — never partially accepted — on the first
    /// rule it fails.
    ///
    /// # Errors
    ///
    /// A [`TraceContextRejection`] naming the first rule the value failed.
    pub fn parse(
        traceparent: &str,
        tracestate: Option<&str>,
    ) -> Result<Self, TraceContextRejection> {
        let mut parsed = parse_traceparent(traceparent)?;
        parsed.state = tracestate.and_then(TraceState::parse);
        Ok(parsed)
    }

    /// The trace this request belongs to.
    #[must_use]
    pub const fn trace_id(&self) -> TraceId {
        self.trace_id
    }

    /// The caller's span, as it identified it.
    #[must_use]
    pub const fn parent_id(&self) -> SpanId {
        self.parent_id
    }

    /// The caller's flags.
    #[must_use]
    pub const fn flags(&self) -> TraceFlags {
        self.flags
    }

    /// The validated `tracestate`, if one was sent and was well-formed.
    #[must_use]
    pub const fn state(&self) -> Option<&TraceState> {
        self.state.as_ref()
    }

    /// Renders a version-00 `traceparent` from the **validated** fields.
    ///
    /// This is the only way a `traceparent` leaves Renvor, so what leaves is what was checked:
    /// exactly 55 bytes, lowercase, and the flags masked to [`TraceFlags::SUPPORTED`] — the bit
    /// this version defines keeps its received value and every other bit is zero. §3.2.2.5.2:
    /// *"Vendors MUST set those to zero"*; §4.3: *"Vendors will set all unparsed / unknown
    /// trace-flags to 0 on outgoing requests."* The first version re-emitted the received byte and
    /// claimed the specification asked for it (Phase 010 correction round, Finding 4). The
    /// received byte stays available through [`TraceFlags::as_byte`] on [`Self::flags`].
    #[must_use]
    pub fn render_traceparent(&self) -> String {
        let mut out = String::with_capacity(TRACEPARENT_LEN);
        out.push_str("00-");
        out.push_str(&self.trace_id.encode());
        out.push('-');
        out.push_str(&self.parent_id.encode());
        out.push('-');
        out.push_str(&self.flags.supported().encode());
        out
    }
}

/// Parses a version-00 `traceparent` value and nothing else.
///
/// # Errors
///
/// The first rule the value failed, in the order §3.2.2 lists the fields.
pub fn parse_traceparent(value: &str) -> Result<TraceContext, TraceContextRejection> {
    let bytes = value.as_bytes();
    if bytes.len() != TRACEPARENT_LEN {
        return Err(TraceContextRejection::Length);
    }
    if bytes[2] != b'-' || bytes[35] != b'-' || bytes[52] != b'-' {
        return Err(TraceContextRejection::Delimiter);
    }
    // Version `ff` is forbidden and every other non-`00` version is one this implementation does
    // not speak. §3.2.4 permits a receiver to parse a higher version's first four fields, and a
    // receiver that does not is still conforming; the strict choice is the one an attacker cannot
    // steer.
    if &bytes[0..2] != b"00" {
        return Err(TraceContextRejection::Version);
    }
    let trace_id = decode::<16>(&bytes[3..35]).ok_or(TraceContextRejection::TraceIdNotHex)?;
    let trace_id = TraceId::from_bytes(trace_id).ok_or(TraceContextRejection::TraceIdZero)?;
    let parent_id = decode::<8>(&bytes[36..52]).ok_or(TraceContextRejection::ParentIdNotHex)?;
    let parent_id = SpanId::from_bytes(parent_id).ok_or(TraceContextRejection::ParentIdZero)?;
    let flags = decode::<1>(&bytes[53..55]).ok_or(TraceContextRejection::FlagsNotHex)?;
    Ok(TraceContext::new(
        trace_id,
        parent_id,
        TraceFlags::from_byte(flags[0]),
    ))
}

/// Decodes exactly `2 * N` **lowercase** hex bytes. Uppercase is refused: `HEXDIGLC` (§3.2.2).
fn decode<const N: usize>(text: &[u8]) -> Option<[u8; N]> {
    if text.len() != 2 * N {
        return None;
    }
    let mut out = [0_u8; N];
    for (index, pair) in text.chunks_exact(2).enumerate() {
        out[index] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Some(out)
}

const fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    out
}

/// §3.3.1.3.1: a simple key or a multi-tenant `tenant@system` key, each part lowercase.
///
/// The Level 1 ABNF, verbatim: `simple-key = lcalpha 0*255( lcalpha / DIGIT / "_" / "-"/ "*" /
/// "/" )`, `tenant-id = ( lcalpha / DIGIT ) 0*240( … )`, `system-id = lcalpha 0*13( … )`. So a
/// simple key and a system-id begin with a lowercase letter, and only a tenant-id may begin with
/// a digit. The first version let every part begin with a digit and accepted `9rojo` (Phase 010
/// correction round, Finding 4).
fn valid_key(key: &str) -> bool {
    if key.is_empty() || key.len() > MAX_MEMBER_PART {
        return false;
    }
    match key.split_once('@') {
        None => valid_key_part(key, 256, Lead::Letter),
        // `tenant` is 1–241 characters and `system` 1–14, per the ABNF.
        Some((tenant, system)) => {
            valid_key_part(tenant, 241, Lead::LetterOrDigit)
                && valid_key_part(system, 14, Lead::Letter)
        }
    }
}

/// What the first character of a key part may be (§3.3.1.3.1).
#[derive(Clone, Copy)]
enum Lead {
    /// `lcalpha`: a simple key or a system-id.
    Letter,
    /// `( lcalpha / DIGIT )`: a tenant-id.
    LetterOrDigit,
}

fn valid_key_part(part: &str, max: usize, lead: Lead) -> bool {
    let bytes = part.as_bytes();
    if bytes.is_empty() || bytes.len() > max {
        return false;
    }
    let first_ok = match lead {
        Lead::Letter => bytes[0].is_ascii_lowercase(),
        Lead::LetterOrDigit => bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit(),
    };
    first_ok
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'_' | b'-' | b'*' | b'/')
        })
}

/// §3.3.1.3.2: up to 256 printable ASCII characters except `,` and `=`, not ending in a space.
fn valid_value(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_MEMBER_PART {
        return false;
    }
    if bytes[bytes.len() - 1] == b' ' {
        return false;
    }
    bytes
        .iter()
        .all(|byte| (0x20..=0x7e).contains(byte) && !matches!(byte, b',' | b'='))
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_TRACESTATE_BYTES, MAX_TRACESTATE_MEMBERS, TRACEPARENT_LEN, TraceContext,
        TraceContextRejection, TraceFlags, TraceId, TraceState, parse_traceparent,
    };

    const VALID: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

    #[test]
    fn the_specifications_own_example_parses_and_re_renders_byte_identically() {
        // §3.2.3's example. Round-tripping is the property that makes "never echo anything
        // unvalidated" checkable: the only bytes that can leave are the ones that were decoded.
        let context = parse_traceparent(VALID).expect("the example is valid");
        assert_eq!(
            context.trace_id().encode(),
            "4bf92f3577b34da6a3ce929d0e0e4736"
        );
        assert_eq!(context.parent_id().encode(), "00f067aa0ba902b7");
        assert!(context.flags().sampled());
        assert_eq!(context.render_traceparent(), VALID);
        assert_eq!(VALID.len(), TRACEPARENT_LEN);
    }

    #[test]
    fn each_rule_is_refused_by_name_in_field_order() {
        // One structured mutant per rule, each named by the rejection §3.2.2 implies.
        let cases: [(&str, TraceContextRejection); 9] = [
            ("", TraceContextRejection::Length),
            (
                "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-",
                TraceContextRejection::Length,
            ),
            (
                "00_4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                TraceContextRejection::Delimiter,
            ),
            (
                "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                TraceContextRejection::Version,
            ),
            (
                "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                TraceContextRejection::Version,
            ),
            (
                "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
                TraceContextRejection::TraceIdNotHex,
            ),
            (
                "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
                TraceContextRejection::TraceIdZero,
            ),
            (
                "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01",
                TraceContextRejection::ParentIdZero,
            ),
            (
                "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-0g",
                TraceContextRejection::FlagsNotHex,
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(
                parse_traceparent(input).expect_err("must be refused"),
                expected,
                "input {input:?}"
            );
        }
        // Uppercase in the parent-id is refused too — §3.2.2.4's own example of an invalid value.
        assert_eq!(
            parse_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00F067AA0BA902B7-01")
                .expect_err("uppercase parent"),
            TraceContextRejection::ParentIdNotHex
        );
    }

    #[test]
    fn flags_are_masked_not_compared() {
        // §3.2.2.5: `09` carries the sampled bit beside a bit this version does not define. The
        // sampled bit is read by mask; the received byte stays available as the inbound record;
        // and what leaves carries only the defined bit. §3.2.2.5.2: "Vendors MUST set those to
        // zero"; §4.3: "Vendors will set all unparsed / unknown trace-flags to 0 on outgoing
        // requests." The first version re-emitted `09` and asserted `ends_with("-09")` (Phase
        // 010 correction round, Finding 4).
        let context = parse_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-09")
            .expect("valid");
        assert!(context.flags().sampled());
        assert_eq!(
            context.flags().as_byte(),
            0x09,
            "the inbound record is the received byte"
        );
        assert!(
            context.render_traceparent().ends_with("-01"),
            "an undefined flag bit left Renvor"
        );
        assert!(!TraceFlags::from_byte(0x08).sampled());
        assert_eq!(TraceFlags::from_byte(0x09).supported(), TraceFlags::SAMPLED);
        assert_eq!(TraceFlags::from_byte(0x08).supported(), TraceFlags::NONE);
        assert_eq!(TraceFlags::from_byte(0xff).supported(), TraceFlags::SAMPLED);
        assert_eq!(TraceFlags::SUPPORTED, TraceFlags::SAMPLED);
        // Every undefined bit, without and with the sampled bit.
        let undefined_only =
            parse_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-08")
                .expect("valid");
        assert!(undefined_only.render_traceparent().ends_with("-00"));
        let all_bits = parse_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-ff")
            .expect("valid");
        assert!(all_bits.render_traceparent().ends_with("-01"));
        assert_eq!(all_bits.flags().as_byte(), 0xff);
    }

    #[test]
    fn the_invalid_identifiers_cannot_be_constructed() {
        assert!(TraceId::from_bytes([0; 16]).is_none());
        assert!(super::SpanId::from_bytes([0; 8]).is_none());
        // POSITIVE CONTROL: any non-zero value is fine.
        assert!(TraceId::from_bytes([1; 16]).is_some());
    }

    #[test]
    fn a_bad_tracestate_is_dropped_without_touching_the_traceparent_verdict() {
        // §3.3: "failure to parse tracestate MUST NOT affect the parsing of traceparent".
        let context = TraceContext::parse(VALID, Some("this is not = a list,,,=")).expect("valid");
        assert!(context.state().is_none(), "a malformed tracestate was kept");
        assert_eq!(context.render_traceparent(), VALID);

        // And the reverse (§3.3): a bad traceparent means tracestate is never parsed.
        assert!(TraceContext::parse("nope", Some("rojo=00f067aa0ba902b7")).is_err());
    }

    #[test]
    fn tracestate_grammar_is_enforced_member_by_member() {
        // §3.3.2's examples, with whitespace the spec permits around members.
        let state =
            TraceState::parse("rojo=00f067aa0ba902b7 , congo=t61rcWkgMzE").expect("valid list");
        assert_eq!(state.as_str(), "rojo=00f067aa0ba902b7,congo=t61rcWkgMzE");
        assert_eq!(state.members(), 2);

        // Empty members are allowed and dropped from the canonical form.
        assert_eq!(TraceState::parse("a=b,,c=d").unwrap().members(), 2);
        assert_eq!(TraceState::parse("").unwrap().members(), 0);

        // Multi-tenant keys.
        assert!(TraceState::parse("tenant@vendor=1").is_some());
        assert!(TraceState::parse("@vendor=1").is_none());
        assert!(TraceState::parse("tenant@verylongsystemname=1").is_none());

        // §3.3.1.3.1, verbatim: `simple-key = lcalpha 0*255( lcalpha / DIGIT / "_" / "-"/ "*" /
        // "/" )`, `tenant-id = ( lcalpha / DIGIT ) 0*240( … )`, `system-id = lcalpha 0*13( … )`.
        // A simple key and a system-id begin with a lowercase letter; only a tenant-id may begin
        // with a digit. The first version let every part begin with a digit and pinned `9rojo=1`
        // as valid (Phase 010 correction round, Finding 4).
        assert!(TraceState::parse("Rojo=1").is_none());
        assert!(TraceState::parse("-rojo=1").is_none());
        assert!(TraceState::parse("rojo=1").is_some());
        assert!(
            TraceState::parse("9rojo=1").is_none(),
            "a simple key began with a digit"
        );
        assert!(
            TraceState::parse("9tenant@vendor=1").is_some(),
            "a tenant-id may begin with a digit"
        );
        assert!(
            TraceState::parse("tenant@9vendor=1").is_none(),
            "a system-id began with a digit"
        );

        // Values: printable ASCII except `,` and `=`, no control characters.
        assert!(TraceState::parse("a=b=c").is_none());
        assert!(TraceState::parse("a=b\tc").is_none());
        assert!(TraceState::parse("a=").is_none());
        // A trailing space at a member boundary is OWS (§3.3.1.1) and is trimmed before the
        // value rule is applied, so `a=b ` canonicalises rather than failing — the value rule's
        // "not a trailing blank" clause is reached only by a blank that survives trimming, which
        // none can. Asserted so the canonical form is pinned, not assumed.
        assert_eq!(TraceState::parse("a=b ").unwrap().as_str(), "a=b");
        assert_eq!(TraceState::parse("a=b c").unwrap().as_str(), "a=b c");
        assert!(TraceState::parse("a=\u{e9}").is_none());
    }

    #[test]
    fn a_tracestate_key_appears_at_most_once() {
        // §3.3.1.4: "Only one entry per key is allowed because the entry represents that last
        // position in the trace." A repeated key is a malformed list; the whole value is dropped,
        // which §4.3 permits for a tracestate that cannot be parsed. Exact byte match: keys are
        // lowercase by grammar, so there is no other equality (Phase 010 correction round,
        // Finding 4).
        assert!(
            TraceState::parse("a=1,a=2").is_none(),
            "a duplicate key was accepted"
        );
        assert!(
            TraceState::parse("a=1, a=2").is_none(),
            "optional whitespace hid a duplicate key"
        );
        assert!(
            TraceState::parse("a=1,,a=2").is_none(),
            "an empty member hid a duplicate key"
        );
        // POSITIVE CONTROLS: distinct keys are kept in order, and a multi-tenant key is not the
        // simple key it begins with.
        let state = TraceState::parse("a=1,b=2").expect("distinct keys");
        assert_eq!(state.members(), 2);
        assert_eq!(state.as_str(), "a=1,b=2");
        assert_eq!(TraceState::parse("a@v=1,a=2").unwrap().members(), 2);
    }

    #[test]
    fn tracestate_bounds_are_enforced() {
        let too_many = (0..=MAX_TRACESTATE_MEMBERS)
            .map(|index| format!("k{index}=v"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(
            TraceState::parse(&too_many).is_none(),
            "33 members accepted"
        );
        let at_limit = (0..MAX_TRACESTATE_MEMBERS)
            .map(|index| format!("k{index}=v"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(TraceState::parse(&at_limit).is_some(), "32 members refused");

        let oversized = format!("a={}", "x".repeat(MAX_TRACESTATE_BYTES));
        assert!(TraceState::parse(&oversized).is_none());
        let long_value = format!("a={}", "x".repeat(257));
        assert!(
            TraceState::parse(&long_value).is_none(),
            "a 257-byte value accepted"
        );
        let max_value = format!("a={}", "x".repeat(256));
        assert!(
            TraceState::parse(&max_value).is_some(),
            "a 256-byte value refused"
        );
    }

    #[test]
    fn debug_never_prints_the_tracestate_members() {
        // A tracestate is caller-chosen opaque text. The one field a diagnostic prints must not
        // carry it.
        let state = TraceState::parse("vendor=secretlookingvalue").expect("valid");
        let rendered = format!("{state:?}");
        assert!(!rendered.contains("secretlookingvalue"), "{rendered}");
        // POSITIVE CONTROL: the count is shown, so the redaction is targeted.
        assert!(rendered.contains("members: 1"), "{rendered}");
    }
}
