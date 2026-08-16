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
            return Self::Decoder(message.to_owned());
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
/// variant is `#[non_exhaustive]`.
#[must_use]
pub fn configuration(
    key: impl Into<String>,
    layer: impl Into<String>,
    expected_type: &'static str,
    constraint: &Constraint,
) -> KernelError {
    KernelError::Configuration {
        key: key.into(),
        constraint: constraint.describe(),
        layer: layer.into(),
        expected_type,
    }
}

#[cfg(test)]
mod tests {
    use super::{Constraint, configuration};
    use crate::error::ErrorCategory;

    const CREDENTIAL: &str = "hunter2-do-not-print";

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
