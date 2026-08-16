//! Constrained construction of configuration errors: T080, FR-021, C-E3.
//!
//! # This module exists because a message can carry a value even when a field cannot
//!
//! [`crate::error::KernelError::Configuration`] has no field that can hold a configuration value —
//! that was the point of its shape. It was not enough, and the gap was found by measurement rather
//! than review:
//!
//! ```text
//! serde/toml, deserializing `port = "hunter2-do-not-print"` into a u16, reports:
//!     invalid type: string "hunter2-do-not-print", expected u16
//! ```
//!
//! **The decoder's own message quotes the offending value.** Any adapter that forwards it into
//! `constraint` — the obvious thing to do, and what Renvor's first configuration adapter did —
//! puts a possibly-secret value into an error message, in **every** output form. C-E3 allows 0.
//!
//! # How this is closed
//!
//! Two mechanisms, because either alone is a rule rather than a guarantee:
//!
//! 1. **[`Constraint`] cannot carry a value.** Every variant holds shapes, bounds, or a
//!    `&'static str` — none of which a runtime value can become. The one variant holding a
//!    `String` is reachable **only** through [`Constraint::from_decoder`], which strips the value.
//! 2. **The `Configuration` variant is `#[non_exhaustive]`**, so no crate outside `renvor-core`
//!    can build it with a struct literal. `renvor-config` and every future adapter must come
//!    through [`configuration`], which takes a `Constraint`.
//!
//! Mechanism 2 is what makes mechanism 1 unavoidable. Without it, an adapter could keep formatting
//! its own string and the type would have no say.

use crate::error::KernelError;

/// The most of an identifier that is reproduced in a diagnostic, in bytes.
///
/// **Renvor's number, not the specification's.** C-E3 requires the value to be absent; nothing
/// requires a *length* bound, and that omission is what this constant closes.
///
/// # An identifier is safe to name and unsafe to name without limit
///
/// A configuration key is not a secret — C-E3 permits emitting it, and an error that will not say
/// *which* key was wrong is not actionable. Its **volume** is a separate question, and it was
/// unbounded until T146. Measured on the real stack before the fix:
///
/// ```text
/// a 9,999,999-byte TOML key      -> a 10,000,269-byte error message
/// a non-Unicode variable name    -> 3.00x amplification through the lossy rendering
/// ```
///
/// Both numbers are attacker-chosen. A configuration file and an environment variable are the two
/// inputs an operator most often lets someone else supply, and an error that scales with them
/// turns one bad key into a log line nothing will read, a terminal nothing can scroll, and — where
/// the diagnostic is retained — a stored object sized by whoever wrote the key.
///
/// 128 bytes is far longer than any hand-written dotted key (`server.tls.certificate_chain_path`
/// is 33) and short enough that a full page of them still fits in one screen.
pub const MAX_IDENTIFIER_BYTES: usize = 128;

/// The most of a constraint **message** that is reproduced in a diagnostic, in bytes.
///
/// # Why this is separate from, and larger than, [`MAX_IDENTIFIER_BYTES`]
///
/// A constraint is a sentence, not a name. `"found string, expected a sequence of (u8, u8)"` is a
/// legitimate message, and a schema with twenty fields produces a legitimate
/// ``unknown field `x`, expected one of `a`, `b`, …`` that is longer still. Truncating those at 128
/// bytes would damage messages that were never a problem, so the ceiling is stated once, here, at a
/// size that fits a real decoder message and still cannot be set by an attacker.
///
/// # This constant exists because the first T146 fix was incomplete
///
/// Found by the round-four requirements delta review (R4-1, CRITICAL) and **reproduced through the
/// public stack**: a 1,000,000-byte key produced a **1,000,363-byte** message *after* T146.
///
/// `configuration` bounded `key` and `layer` and left `constraint` alone, and `constraint` is where
/// the key actually travelled. [`Constraint::from_decoder`]'s first rule forwards a decoder message
/// verbatim when it starts with `missing field`, `unknown field`, or `duplicate key` — and two of
/// those three **quote the key**. A `serde` schema using `deny_unknown_fields`, which is the
/// fail-closed shape the constitution pushes an author toward, therefore wrote the whole key into
/// the message through a field nobody had bounded.
///
/// The lesson is in the *test* that missed it: it counted `bounded_identifier` calls, so it
/// asserted that the fix was present rather than that the property held. It has been replaced with
/// an end-to-end reproduction.
pub const MAX_CONSTRAINT_BYTES: usize = 512;

/// The largest a **rendered** identifier can be, in bytes.
///
/// Equal to [`MAX_IDENTIFIER_BYTES`], because the truncation notice is counted **inside** the
/// ceiling rather than added to it. That is what makes bounding idempotent — see `bounded_to`, and
/// finding S4-3 for the misreported length that made idempotence necessary rather than tidy.
///
/// It remains a separate constant so call sites assert against a name rather than recomputing the
/// relationship, which is the defect [`Constraint::TooDeep`] was added to prevent one layer up.
pub const MAX_RENDERED_IDENTIFIER_BYTES: usize = MAX_IDENTIFIER_BYTES;

/// The same, for a constraint message.
pub const MAX_RENDERED_CONSTRAINT_BYTES: usize = MAX_CONSTRAINT_BYTES;

/// Renders an identifier into a diagnostic, bounded by [`MAX_IDENTIFIER_BYTES`].
///
/// Returns `raw` unchanged when it already fits — the overwhelmingly common case, and one that
/// must not gain a suffix, because an error reading ``key `port` (truncated; 4 bytes in full)``
/// would be worse than the problem this solves.
///
/// # The original length is reported, and that is deliberate
///
/// Truncating silently would make two different 10 MB keys produce byte-identical errors, so an
/// operator could not tell "my key is too long" from "my key is wrong". The **length** is a shape
/// fact about the identifier, not a fragment of it, and this file's whole argument is that shapes
/// are safe to report where values are not.
///
/// # Truncation lands on a character boundary
///
/// `&raw[..n]` panics when `n` splits a multi-byte character, and a diagnostic path that panics on
/// hostile input is worse than the unbounded message it replaced — SC-004 allows **0** panics. The
/// cut walks back to a boundary, so a key made entirely of 4-byte characters truncates at 128 and
/// a key whose 128th byte is mid-character truncates a little earlier.
#[must_use]
pub fn bounded_identifier(raw: &str) -> String {
    bounded_to(raw, MAX_IDENTIFIER_BYTES)
}

/// Renders a constraint message into a diagnostic, bounded by [`MAX_CONSTRAINT_BYTES`].
///
/// Separate from [`bounded_identifier`] only in its ceiling. See [`MAX_CONSTRAINT_BYTES`] for why a
/// message gets a larger one than a name, and for the CRITICAL finding that showed this function
/// had to exist.
#[must_use]
pub fn bounded_constraint(raw: &str) -> String {
    bounded_to(raw, MAX_CONSTRAINT_BYTES)
}

/// The shared implementation: truncate at `ceiling` bytes, on a character boundary, and say so.
///
/// One function rather than two near-copies, because two copies of a truncation rule are two places
/// for the character-boundary walk to be got wrong.
fn bounded_to(raw: &str, ceiling: usize) -> String {
    if raw.len() <= ceiling {
        return raw.to_owned();
    }

    // The notice is counted INSIDE the ceiling, which is what makes this function idempotent:
    // its output is always `<= ceiling`, so bounding an already-bounded string returns it
    // unchanged.
    //
    // # Why idempotence is required rather than merely tidy (S4-3, R4-9)
    //
    // An identifier can pass through two bounds on one journey: `SourceLayer::file` bounds a path
    // at construction, and `Display`/`Debug` bound it again on the way out. When the notice sat
    // *outside* the ceiling, the first pass produced a ~180-byte string, the second pass saw
    // something over the 128-byte ceiling and truncated it AGAIN — and reported "180 bytes in
    // full", which is the length of the first truncation rather than of the original path.
    //
    // A length that is not the length of anything the reader has is worse than no length at all:
    // it is a number that looks like evidence. Reserving room for the notice removes the second
    // truncation entirely.
    let notice = format!("… (truncated; {} bytes in full)", raw.len());
    let room = ceiling.saturating_sub(notice.len());

    let mut cut = room.min(raw.len());
    while cut > 0 && !raw.is_char_boundary(cut) {
        cut -= 1;
    }

    format!("{}{notice}", &raw[..cut])
}

/// What was wrong with a configuration value, stated **without** the value.
///
/// Every variant describes a shape, a bound, or a fixed rule. None can hold the offending value,
/// which is what makes redaction a property of the type rather than of the caller's discipline.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Constraint {
    /// No layer supplied a value and there is no default.
    Missing,
    /// The value had the wrong shape. Both sides are shapes; neither is a value.
    WrongShape {
        /// What the source supplied, e.g. `string`.
        found: &'static str,
        /// What the schema declared, e.g. `u16`.
        expected: &'static str,
    },
    /// A number outside its inclusive range.
    OutOfRange {
        /// The smallest accepted value.
        minimum: i128,
        /// The largest accepted value.
        maximum: i128,
    },
    /// Shorter than the declared minimum. Carries the **length**, never the content.
    TooShort {
        /// The minimum accepted length.
        minimum: usize,
    },
    /// Longer than the declared maximum. Carries the **length**, never the content.
    TooLong {
        /// The maximum accepted length.
        maximum: usize,
    },
    /// Larger than a declared byte ceiling.
    ///
    /// Distinct from [`Self::TooLong`] because bytes are not characters, and a size ceiling
    /// reported in characters is a number that does not mean what it says.
    TooLarge {
        /// The largest accepted size, in bytes.
        maximum_bytes: u64,
    },
    /// Nested deeper than a declared ceiling.
    ///
    /// Distinct from [`Self::TooLong`] and [`Self::TooLarge`] because a nesting depth is neither a
    /// character count nor a byte count, and because the reason for the ceiling is different: a
    /// structure past it cannot be decoded **or dropped** without recursing far enough to overflow
    /// the stack, which no Rust guard can catch.
    ///
    /// Added at the W-005 security delta review (S4-1). The two depth refusals previously wrote
    /// the number into a `Rule`'s `&'static str`, so the ceiling and the message that named it
    /// were independent — setting the constant to 8 left both messages saying 32, and a test
    /// asserting the literal still passed. Carrying the number makes that impossible.
    TooDeep {
        /// The deepest accepted nesting.
        maximum_depth: usize,
    },
    /// A fixed, author-written explanation.
    ///
    /// `&'static str` on purpose: a runtime value cannot become one, so this variant cannot be
    /// used to smuggle a value in behind a rule's name.
    Rule(&'static str),
    /// A decoder's own report, **already stripped** of any value it named.
    ///
    /// The only variant carrying a `String`, and its only constructor is
    /// [`Constraint::from_decoder`].
    Decoder(String),
}

/// Message prefixes that name **keys**, which C-E3 permits emitting, and never values.
///
/// Kept as a short explicit list rather than a heuristic: anything not on it is rewritten, so a
/// decoder message shape nobody anticipated fails closed instead of being forwarded.
const KEY_ONLY_PREFIXES: [&str; 3] = ["missing field", "unknown field", "duplicate key"];

impl Constraint {
    /// Builds a constraint from a decoder's message, **removing the value it may quote**.
    ///
    /// `found` is the shape the source actually supplied, which the caller knows from the value it
    /// holds — so the resulting message can still say *what kind of thing* was there without
    /// saying what it was.
    ///
    /// The rules, in order:
    ///
    /// 1. A message naming only a **key** (`missing field`, `unknown field`, `duplicate key`) is
    ///    kept as-is. Keys are already carried by the error and are permitted by C-E3.
    /// 2. Otherwise the fragment after the **last** `", expected "` is kept — that is the declared
    ///    type, and everything before it is where decoders quote the input.
    /// 3. That fragment is then checked against a type-description whitelist. Anything that does
    ///    not look like a bare type name is discarded.
    /// 4. A message matching neither shape is discarded entirely and replaced with the shapes.
    ///    **Failing closed** matters more here than a richer message.
    ///
    /// # Rule 2 said `split_once` until 2026-08-16, and that was a fail-open
    ///
    /// Found by the W-005 security review (finding 2.1), **measured through the real stack** on
    /// both the file and environment layers, at top level and nested. `split_once` splits at the
    /// **first** occurrence, and the rule assumed the first `", expected "` was the decoder's own
    /// separator. A value containing that literal put the first occurrence *inside itself*, so
    /// everything after it — the rest of the value included — was written into the error:
    ///
    /// ```text
    /// value:  s3cr3t-token-abc123", expected u16
    /// before: found a string, expected s3cr3t-token-abc123", expected u16      ← leaked
    /// after:  found a string, expected the declared type                       ← discarded
    /// ```
    ///
    /// The doc comment claimed the unmatched case failed closed. It did — but that case is the one
    /// needing no protection. The branch reached by attacker-chosen text was the forwarding one.
    ///
    /// `rsplit_once` fixes it because the decoder appends its separator **last**, so the final
    /// occurrence is always the decoder's own. Rule 3 exists because relying on that alone would be
    /// one library rewording away from being wrong again.
    #[must_use]
    pub fn from_decoder(message: &str, found: &'static str) -> Self {
        if KEY_ONLY_PREFIXES
            .iter()
            .any(|prefix| message.starts_with(prefix))
        {
            // BOUNDED (R4-1). Two of the three permitted prefixes — `unknown field` and
            // `duplicate key` — quote the KEY, and a key is chosen by whoever wrote the
            // configuration. Forwarding this branch verbatim is how a 1 MB key produced a 1 MB
            // error message after T146 had supposedly bounded diagnostics: the `key` field was
            // bounded and the key travelled in `constraint` instead.
            return Self::Decoder(bounded_constraint(message));
        }

        let stripped = Self::WrongShape {
            found,
            expected: "the declared type",
        };

        message
            .rsplit_once(", expected ")
            .filter(|(_, expected)| Self::is_type_description(expected))
            .map_or(stripped, |(_discarded, expected)| {
                Self::Decoder(format!("found {found}, expected {expected}"))
            })
    }

    /// Whether a fragment looks like a bare type description rather than smuggled input.
    ///
    /// A whitelist, deliberately. The alternative — looking for characters that indicate a value —
    /// is a blocklist, and a blocklist of ways to encode a credential is a list somebody will get
    /// wrong. A serde or toml type name is short and made of identifier characters, spaces, and a
    /// little punctuation for generics and tuples; anything else is discarded rather than judged.
    fn is_type_description(fragment: &str) -> bool {
        !fragment.is_empty()
            && fragment.len() <= 64
            && fragment.chars().all(|character| {
                character.is_ascii_alphanumeric()
                    || matches!(
                        character,
                        ' ' | '_' | '-' | ':' | '<' | '>' | ',' | '(' | ')'
                    )
            })
    }

    /// How this constraint reads inside an error message.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Missing => "no layer supplied a value and there is no default".to_owned(),
            Self::WrongShape { found, expected } => format!("found {found}, expected {expected}"),
            Self::OutOfRange { minimum, maximum } => {
                format!("must be between {minimum} and {maximum} inclusive")
            }
            Self::TooShort { minimum } => format!("must be at least {minimum} characters"),
            Self::TooLong { maximum } => format!("must be at most {maximum} characters"),
            Self::TooLarge { maximum_bytes } => {
                format!("exceeds the {maximum_bytes} byte ceiling")
            }
            Self::TooDeep { maximum_depth } => format!(
                "nests deeper than the {maximum_depth}-level ceiling; decoding and then dropping a \
                 structure that deep recurses far enough to overflow the stack, which no Rust \
                 guard can catch"
            ),
            Self::Rule(rule) => (*rule).to_owned(),
            Self::Decoder(report) => report.clone(),
        }
    }
}

/// Builds a [`KernelError::Configuration`] from parts that cannot carry a value.
///
/// The **only** way for a crate outside `renvor-core` to construct that variant, because the
/// variant is `#[non_exhaustive]`. That chokepoint is what makes the bound below unavoidable
/// rather than a convention every adapter has to remember.
///
/// Both identifiers are passed through [`bounded_identifier`], so a key or a layer name chosen by
/// whoever wrote the configuration cannot set the size of the resulting message.
#[must_use]
pub fn configuration(
    key: impl Into<String>,
    layer: impl Into<String>,
    expected_type: &'static str,
    constraint: &Constraint,
) -> KernelError {
    KernelError::Configuration {
        key: bounded_identifier(&key.into()),
        // Bounded HERE as well as inside `Constraint::from_decoder`, and the redundancy is the
        // point (R4-1). Bounding only at the constructor of the one variant that can carry a long
        // string makes the guarantee depend on nobody adding a second such variant later; bounding
        // at the chokepoint makes it a property of every `Configuration` error there will ever be.
        constraint: bounded_constraint(&constraint.describe()),
        layer: bounded_identifier(&layer.into()),
        expected_type,
    }
}

/// Builds a [`KernelError::ConfigurationConflict`] with every identifier bounded.
///
/// The **only** way for a crate outside `renvor-core` to construct that variant, for the same
/// reason [`configuration`] is: the variant is `#[non_exhaustive]`.
///
/// # This variant was unmediated until T146
///
/// `Configuration` has been `#[non_exhaustive]` since T080, so every adapter had to come through
/// [`configuration`]. `ConfigurationConflict` was not, and `renvor-config` built it with a struct
/// literal — so it carried three attacker-influenced identifiers (`key`, and **both** layer names)
/// with no constructor in a position to bound any of them. Two variants describing the same
/// subject should not have had two different construction rules; they now have one.
///
/// The two shapes are `&'static str` for the same reason a [`Constraint`] mostly is: a shape is
/// what may be reported, and a static cannot be built from a runtime value.
#[must_use]
pub fn conflict(
    key: impl Into<String>,
    first_layer: impl Into<String>,
    first_shape: &'static str,
    second_layer: impl Into<String>,
    second_shape: &'static str,
) -> KernelError {
    KernelError::ConfigurationConflict {
        key: bounded_identifier(&key.into()),
        first_layer: bounded_identifier(&first_layer.into()),
        first_shape,
        second_layer: bounded_identifier(&second_layer.into()),
        second_shape,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Constraint, MAX_IDENTIFIER_BYTES, MAX_RENDERED_CONSTRAINT_BYTES,
        MAX_RENDERED_IDENTIFIER_BYTES, bounded_identifier, configuration, conflict,
    };
    use crate::error::{ErrorCategory, KernelError};

    const CREDENTIAL: &str = "hunter2-do-not-print";

    #[test]
    fn an_ordinary_identifier_is_returned_untouched() {
        // POSITIVE CONTROL for every bound below. A ceiling that rewrote ordinary keys would pass
        // the size assertions perfectly and make every real diagnostic worse — which is the
        // failure mode that matters, since almost every key is ordinary.
        for key in [
            "port",
            "server.tls.certificate_chain_path",
            "database.pool.max_connections",
            "ключ.с.юникодом",
        ] {
            assert_eq!(
                bounded_identifier(key),
                key,
                "an identifier well inside the ceiling was rewritten"
            );
            assert!(
                !bounded_identifier(key).contains("truncated"),
                "an ordinary identifier gained a truncation notice"
            );
        }
    }

    #[test]
    fn the_rendered_size_does_not_grow_with_the_input() {
        // T146, the property the whole constant exists for. Two inputs three orders of magnitude
        // apart must render to the *same* size, or the bound is a suggestion.
        let short = "k".repeat(MAX_IDENTIFIER_BYTES * 10);
        let long = "k".repeat(MAX_IDENTIFIER_BYTES * 10_000);

        let rendered_short = bounded_identifier(&short);
        let rendered_long = bounded_identifier(&long);

        assert!(rendered_short.len() <= MAX_RENDERED_IDENTIFIER_BYTES);
        assert!(rendered_long.len() <= MAX_RENDERED_IDENTIFIER_BYTES);

        // The only difference permitted is the decimal length of the reported byte count.
        //
        // A fixed message, not an interpolated one. This file handles credentials, and
        // `tests/diagnostics.rs` refuses any assertion diagnostic that interpolates — the byte
        // count would be safe here, but a per-case exemption is how the class of defect that gate
        // exists to prevent gets reintroduced. It fired on this very line and was right to.
        assert!(
            rendered_long.len().abs_diff(rendered_short.len()) <= 4,
            "the rendering grew when the input grew 1000-fold, so it is not bounded"
        );

        // POSITIVE CONTROL: the inputs really were enormous and really did differ.
        assert!(long.len() > 1_000_000 && long.len() > short.len() * 100);
    }

    #[test]
    fn bounding_is_idempotent_and_reports_the_original_length() {
        // S4-3 / R4-9. An identifier can meet two bounds on one journey: `SourceLayer::file` bounds
        // a path at construction, and `Display`/`Debug` bound it again on the way out. When the
        // notice sat outside the ceiling, the second pass truncated the FIRST truncation and
        // reported its length — a number that was the length of nothing the reader had.
        let path = "x".repeat(200_000);

        let once = bounded_identifier(&path);
        let twice = bounded_identifier(&once);

        assert_eq!(
            once, twice,
            "bounding twice is not the same as bounding once"
        );
        assert!(once.len() <= MAX_RENDERED_IDENTIFIER_BYTES);
        assert!(
            once.contains("200000"),
            "the reported length must be the ORIGINAL length, not the first truncation's"
        );
        assert!(
            !twice.contains(&format!("{} bytes", once.len())),
            "the second pass reported its own input's length"
        );
    }

    #[test]
    fn a_truncated_identifier_still_says_what_it_was() {
        let key = format!("{}.tail", "a".repeat(10_000));
        let rendered = bounded_identifier(&key);

        assert!(
            rendered.starts_with("aaaa"),
            "the beginning of the key was discarded, so it names nothing"
        );
        assert!(
            rendered.contains("10005"),
            "the original length is missing, so two different long keys look identical"
        );
    }

    #[test]
    fn truncation_never_splits_a_character() {
        // `&raw[..n]` panics when `n` lands mid-character, and SC-004 allows 0 panics on hostile
        // input. Every offset in the boundary region is exercised by shifting an ASCII run against
        // a multi-byte tail, so some iteration must land the cut inside a character.
        for pad in 0..8 {
            let key = format!(
                "{}{}",
                "a".repeat(MAX_IDENTIFIER_BYTES - pad),
                "é€𝄞".repeat(64)
            );
            let rendered = bounded_identifier(&key);
            assert!(rendered.len() <= MAX_RENDERED_IDENTIFIER_BYTES);
        }

        // And a key made entirely of 4-byte characters, where every boundary but one is invalid.
        let key = "𝄞".repeat(1_000);
        assert!(bounded_identifier(&key).len() <= MAX_RENDERED_IDENTIFIER_BYTES);
    }

    #[test]
    fn a_configuration_error_is_bounded_in_both_identifiers() {
        // The chokepoint, exercised as an adapter would reach it: a hostile key AND a hostile
        // layer name, since bounding one and forgetting the other is the obvious partial fix.
        let error = configuration(
            "k".repeat(9_999_999),
            "l".repeat(9_999_999),
            "u16",
            &Constraint::Missing,
        );

        let rendered = error.to_string();
        assert!(
            rendered.len() < 1_024,
            "a 10 MB key produced an over-long message"
        );
        assert_eq!(error.category(), ErrorCategory::Configuration);
    }

    #[test]
    fn a_conflict_error_is_bounded_in_all_three_identifiers() {
        // The variant that had no constructor until T146, so nothing was in a position to bound
        // it. All three identifiers are hostile here: bounding the key and forwarding the layers
        // would still let an attacker set the message size.
        let error = conflict(
            "k".repeat(9_999_999),
            "first".repeat(2_000_000),
            "table",
            "second".repeat(2_000_000),
            "string",
        );

        let rendered = error.to_string();
        assert!(
            rendered.len() < 1_024,
            "a conflict over 10 MB identifiers produced an over-long message"
        );
        assert_eq!(error.category(), ErrorCategory::ConfigurationConflict);
    }

    #[test]
    fn every_string_field_of_a_configuration_error_is_bounded() {
        // # This test replaces one that counted calls, and the difference is the whole lesson
        //
        // The previous version asserted `production.matches("bounded_identifier(&").count() == 5`.
        // It passed. It was also useless, because it asserted that **the fix I wrote was present**
        // rather than that **the property held** — and the property did not hold. `configuration`
        // sets three `String` fields and the old test only ever knew about the two I had bounded.
        // `constraint` carried the key straight through (R4-1, CRITICAL, reproduced end to end at
        // 1,000,363 bytes).
        //
        // So this version drives every field from hostile input and measures the result. A field
        // added later is covered the moment someone passes it something long, and a bound that is
        // removed fails here regardless of how the source is spelled.
        let giant = "k".repeat(1_000_000);
        let decoder_message = format!("unknown field `{giant}`, expected one of `port`");

        let error = configuration(
            giant.clone(),
            giant.clone(),
            "u16",
            &Constraint::from_decoder(&decoder_message, "table"),
        );

        let KernelError::Configuration {
            key,
            constraint,
            layer,
            ..
        } = &error
        else {
            panic!("the constructor stopped producing a Configuration error");
        };

        assert!(key.len() <= MAX_RENDERED_IDENTIFIER_BYTES, "key unbounded");
        assert!(
            layer.len() <= MAX_RENDERED_IDENTIFIER_BYTES,
            "layer unbounded"
        );
        assert!(
            constraint.len() <= MAX_RENDERED_CONSTRAINT_BYTES,
            "constraint unbounded — this is exactly R4-1"
        );
        assert!(
            error.to_string().len() < 2_048,
            "the rendered message is unbounded"
        );

        // POSITIVE CONTROL: the input really was enormous, so the bounds above removed something.
        assert!(decoder_message.len() > 1_000_000);
    }

    #[test]
    fn a_decoder_message_quoting_a_giant_key_is_bounded() {
        // R4-1 at the unit level, on the branch that caused it. `unknown field` and `duplicate
        // key` are on the permitted-prefix list *because* keys may be named — and both quote the
        // key, so "permitted to name" had to stop meaning "permitted at any length".
        // Identified by `index`, not by interpolating the prefix or the rendering. This file
        // handles credentials, so `tests/diagnostics.rs` refuses any assertion diagnostic that
        // interpolates — and it fired on the first draft of this very test, which is the gate
        // doing exactly its job.
        for (index, prefix) in ["unknown field", "duplicate key", "missing field"]
            .into_iter()
            .enumerate()
        {
            let giant = "k".repeat(500_000);
            let raw = format!("{prefix} `{giant}`");
            let described = Constraint::from_decoder(&raw, "table").describe();

            assert!(
                described.len() <= MAX_RENDERED_CONSTRAINT_BYTES,
                "permitted-prefix branch {index} forwarded an over-long message"
            );
            // The prefix survives: the message still says WHICH kind of problem it was.
            assert!(
                described.starts_with(prefix),
                "permitted-prefix branch {index} lost its prefix"
            );
        }
    }

    #[test]
    fn an_ordinary_decoder_message_is_not_truncated() {
        // POSITIVE CONTROL for the two tests above. A ceiling that mangled real decoder messages
        // would satisfy every size assertion and make every genuine schema error worse. A
        // twenty-field `deny_unknown_fields` rejection is long, legitimate, and must survive.
        let fields = (0..20)
            .map(|n| format!("`field_{n}`"))
            .collect::<Vec<_>>()
            .join(", ");
        let raw = format!("unknown field `typo`, expected one of {fields}");
        assert!(
            raw.len() > 200,
            "the fixture is not long enough to be a test"
        );

        let described = Constraint::from_decoder(&raw, "table").describe();
        assert_eq!(described, raw, "a legitimate decoder message was truncated");
    }

    #[test]
    fn a_decoder_message_quoting_a_value_is_stripped_of_it() {
        // The exact message `toml` produces, measured rather than guessed.
        let raw = format!("invalid type: string \"{CREDENTIAL}\", expected u16");
        let constraint = Constraint::from_decoder(&raw, "string");

        let described = constraint.describe();
        assert!(
            !described.contains(CREDENTIAL),
            "the value survived stripping"
        );
        assert!(described.contains("u16"), "the expectation was discarded");
        assert!(described.contains("string"), "the shape was discarded");

        // POSITIVE CONTROL: the raw message really does contain the value, so the stripping above
        // removed something rather than acting on a message that never had it.
        assert!(raw.contains(CREDENTIAL));
    }

    #[test]
    fn a_value_that_contains_the_separator_does_not_smuggle_itself_through() {
        // W-005 security finding 2.1, as a regression test. `split_once` took the FIRST
        // `", expected "`; a value containing that literal put the first occurrence inside itself,
        // so its tail was copied into the error. Measured through the real stack on both layers
        // before the fix, on values an attacker controls: an environment variable and a TOML file.
        //
        // Each payload below is a *value*. The message is what the decoder produces around it.
        for payload in [
            "s3cr3t-token-abc123\", expected u16",
            "hunter2, expected LEAKED-TAIL",
            "a, expected b, expected c",
            ", expected ",
        ] {
            let raw = format!("invalid type: string \"{payload}\", expected u16");
            let described = Constraint::from_decoder(&raw, "string").describe();

            assert!(
                !described.contains("s3cr3t-token-abc123")
                    && !described.contains("LEAKED-TAIL")
                    && !described.contains("hunter2"),
                "a value containing the separator reached the error"
            );
            // And what survives is a description of shapes, never a fragment of the input.
            assert!(
                described.starts_with("found string, expected "),
                "the rewritten message has an unexpected shape"
            );
        }
    }

    #[test]
    fn an_ordinary_type_name_still_survives() {
        // POSITIVE CONTROL for the test above and for `is_type_description`. Tightening the rule
        // must not make every message collapse to "the declared type" — a constraint that never
        // says what was expected is safe and useless, and would pass the leak assertions perfectly.
        for (raw, expected) in [
            ("invalid type: string \"x\", expected u16", "u16"),
            ("invalid type: integer 1, expected a string", "a string"),
            (
                "invalid type: string \"x\", expected Vec<String>",
                "Vec<String>",
            ),
            (
                "invalid type: string \"x\", expected a sequence of (u8, u8)",
                "a sequence of (u8, u8)",
            ),
        ] {
            let described = Constraint::from_decoder(raw, "string").describe();
            assert!(
                described.ends_with(expected),
                "the declared type was discarded"
            );
        }
    }

    #[test]
    fn a_fragment_that_is_not_a_type_name_is_discarded() {
        // Rule 3 on its own: even after `rsplit_once`, anything that does not look like a bare
        // type name is thrown away rather than judged. This is what stops a future library
        // rewording from re-opening finding 2.1.
        for fragment in [
            "a value containing \"quotes\"",
            "a\nnewline",
            "a really long fragment that goes well past the sixty-four character ceiling set here",
        ] {
            let raw = format!("invalid type: string \"v\", expected {fragment}");
            let described = Constraint::from_decoder(&raw, "string").describe();
            assert_eq!(
                described, "found string, expected the declared type",
                "a non-type fragment was forwarded"
            );
        }
    }

    #[test]
    fn a_message_of_an_unrecognised_shape_is_discarded_entirely() {
        // Failing closed: a decoder message nobody anticipated is replaced, not forwarded.
        let raw = format!("something unexpected happened near \"{CREDENTIAL}\"");
        let described = Constraint::from_decoder(&raw, "string").describe();

        assert!(!described.contains(CREDENTIAL), "the credential survived");
        assert!(
            !described.contains("something unexpected"),
            "the unrecognised message was forwarded rather than discarded"
        );
        assert_eq!(described, "found string, expected the declared type");
    }

    #[test]
    fn a_key_only_message_is_kept_because_keys_are_permitted() {
        // C-E3 permits the key; the error already carries one. Discarding these would make
        // "missing field `port`" read as "found table, expected the declared type", which is worse
        // and no safer.
        let described = Constraint::from_decoder("missing field `port`", "table").describe();
        assert_eq!(described, "missing field `port`");
    }

    #[test]
    fn no_constraint_variant_can_hold_a_runtime_value() {
        // The structural claim, asserted by construction: every variant below is built from
        // shapes, bounds, or statics. `Decoder` is the only one holding a `String`, and it is
        // reachable only through the stripping constructor above.
        let variants = [
            Constraint::Missing,
            Constraint::WrongShape {
                found: "string",
                expected: "u16",
            },
            Constraint::OutOfRange {
                minimum: 1,
                maximum: 65535,
            },
            Constraint::TooShort { minimum: 12 },
            Constraint::TooLong { maximum: 256 },
            Constraint::TooLarge {
                maximum_bytes: 1024,
            },
            Constraint::Rule("must be a valid hostname"),
        ];

        for constraint in &variants {
            assert!(
                !constraint.describe().contains(CREDENTIAL),
                "a constraint variant rendered the credential"
            );
        }

        // And the source itself: `Decoder` is the only `String`-carrying variant.
        let source = include_str!("context.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("split always yields one part");
        assert_eq!(
            production.matches("(String)").count(),
            1,
            "a second String-carrying variant appeared, which would bypass the stripping"
        );
    }

    #[test]
    fn the_constructor_produces_a_configuration_error() {
        let error = configuration(
            "database.password",
            "environment",
            "a secret string",
            &Constraint::TooShort { minimum: 12 },
        );

        assert_eq!(error.category(), ErrorCategory::Configuration);
        let rendered = error.to_string();
        assert!(rendered.contains("database.password"), "the key is missing");
        assert!(rendered.contains("environment"), "the layer is missing");
        assert!(
            rendered.contains("at least 12"),
            "the constraint is missing"
        );
        assert!(!rendered.contains(CREDENTIAL));
    }
}
