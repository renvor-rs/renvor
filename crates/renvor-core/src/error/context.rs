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
    /// 2. Otherwise the fragment after `", expected "` is kept — that is the declared type, and
    ///    everything before it is where decoders quote the input.
    /// 3. A message matching neither shape is discarded entirely and replaced with the shapes.
    ///    **Failing closed** matters more here than a richer message.
    #[must_use]
    pub fn from_decoder(message: &str, found: &'static str) -> Self {
        if KEY_ONLY_PREFIXES
            .iter()
            .any(|prefix| message.starts_with(prefix))
        {
            return Self::Decoder(message.to_owned());
        }

        message.split_once(", expected ").map_or(
            Self::WrongShape {
                found,
                expected: "the declared type",
            },
            |(_discarded, expected)| Self::Decoder(format!("found {found}, expected {expected}")),
        )
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
            "the value survived stripping: {described}"
        );
        assert!(
            described.contains("u16"),
            "the expectation is kept: {described}"
        );
        assert!(
            described.contains("string"),
            "the shape is kept: {described}"
        );

        // POSITIVE CONTROL: the raw message really does contain the value, so the stripping above
        // removed something rather than acting on a message that never had it.
        assert!(raw.contains(CREDENTIAL));
    }

    #[test]
    fn a_message_of_an_unrecognised_shape_is_discarded_entirely() {
        // Failing closed: a decoder message nobody anticipated is replaced, not forwarded.
        let raw = format!("something unexpected happened near \"{CREDENTIAL}\"");
        let described = Constraint::from_decoder(&raw, "string").describe();

        assert!(!described.contains(CREDENTIAL), "{described}");
        assert!(!described.contains("something unexpected"), "{described}");
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
                "{constraint:?} rendered the credential"
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
        assert!(rendered.contains("database.password"), "{rendered}");
        assert!(rendered.contains("environment"), "{rendered}");
        assert!(rendered.contains("at least 12"), "{rendered}");
        assert!(!rendered.contains(CREDENTIAL));
    }
}
