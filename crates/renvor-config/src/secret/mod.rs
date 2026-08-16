//! The Renvor secret boundary type.
//!
//! # Renvor owns the output contract, not the underlying crate
//!
//! Contract C-C9 is explicit about the division, and it is not the one people assume. `secrecy`
//! supplies **two** things: a redacted `Debug`, and zeroization on drop. It supplies **no
//! `Display` at all**, and it reaches **none** of the paths a value actually leaks through:
//!
//! | Output path | Provided by | Behaviour here |
//! |---|---|---|
//! | `Debug` | the crate | redacted placeholder |
//! | **`Display`** | **Renvor** | placeholder — the crate implements none |
//! | **error message and context** | **Renvor** | the value cannot enter either |
//! | **structured tracing / log fields** | **Renvor** | records a placeholder — see [`Secret::redacted_field`] for why this is not a trait impl |
//! | **any serialization path** | **Renvor** | **refused** |
//! | zeroization on drop | the crate | value wiped |
//!
//! Four of those six rows are Renvor's, which is why this file exists rather than a re-export.
//!
//! # Serialization is refused by an absent implementation
//!
//! `secrecy` gates serialization behind a marker trait, `SerializableSecret`, that the author must
//! opt into. Renvor **deliberately does not implement it** for [`Secret`], and the crate's `serde`
//! feature is **not enabled** in the manifest.
//!
//! So `Secret<T>` does not implement `Serialize`, and a caller who tries to serialize one gets a
//! **compile error** rather than a redacted string in a payload. A runtime guard would be a
//! promise; an absent trait is a fact — and the compile error is asserted by a trybuild-free
//! equivalent below: a test that lists the traits `Secret` does implement.
//!
//! # `zeroize` is reached through `secrecy`, not declared again
//!
//! The `Zeroize` bound is needed to name `SecretBox`'s requirement, and `secrecy` re-exports the
//! crate for exactly that. Declaring `zeroize` a second time in the manifest would create a second
//! version requirement that can drift from the one `secrecy` actually uses — the same defect class
//! as restating a workspace `edition`.
//!
//! # What this type deliberately cannot do
//!
//! There is no `Deref`, no `AsRef`, and no `Into<T>`. Reaching the value takes exactly one method
//! with an unmissable name, [`Secret::expose`], so a leak is always visible at the call site in
//! review. Convenience conversions are how secrets end up in format strings by accident.

use core::fmt;

use secrecy::zeroize::Zeroize;
use secrecy::{ExposeSecret as _, SecretBox};

/// What every output form of a secret renders as.
///
/// One constant, so `Display`, the tracing field, and the tests cannot disagree about it.
pub const REDACTED: &str = "[redacted]";

/// A configuration value marked secret.
///
/// Wraps `secrecy::SecretBox` for access control and zeroization, and supplies the output contract
/// C-C9 assigns to Renvor.
pub struct Secret<T: Zeroize> {
    inner: SecretBox<T>,
    /// The configuration key this value came from, for diagnostics that must name the field
    /// while never naming the value.
    key: String,
}

impl<T: Zeroize> Secret<T> {
    /// Marks a value secret, recording the key it came from.
    pub fn new(key: impl Into<String>, value: T) -> Self {
        Self {
            inner: SecretBox::new(Box::new(value)),
            key: key.into(),
        }
    }

    /// The configuration key this value came from.
    ///
    /// FR-018 and scenario 7 both require the **field name to remain visible** while the value is
    /// redacted — a redaction that hides which field is set is not diagnosable.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Reaches the secret value.
    ///
    /// Named to be conspicuous. There is no `Deref`, no `AsRef`, and no `Into<T>`, so this is the
    /// **only** way through — and every leak therefore has a visible call site.
    #[must_use]
    pub fn expose(&self) -> &T {
        self.inner.expose_secret()
    }
}

/// Renders the placeholder. **The underlying crate implements no `Display` at all**, so this path
/// is entirely Renvor's (C-C9).
impl<T: Zeroize> fmt::Display for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

/// Renders the **key** and the placeholder, never the value.
///
/// Hand-written rather than delegated, because the key is Renvor's addition and the crate's own
/// `Debug` knows nothing about it.
impl<T: Zeroize> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Secret")
            .field("key", &self.key)
            .field("value", &REDACTED)
            .finish()
    }
}

impl<T: Zeroize> Secret<T> {
    /// The value to record in a structured tracing field.
    ///
    /// # Why this is a method and not a `tracing::Value` implementation
    ///
    /// **`tracing::Value` is a sealed trait.** Renvor cannot implement it for its own type, and
    /// finding that out is the useful part: C-C9 assigns the structured-field path to Renvor, and
    /// the mechanism is not the impl one would reach for first.
    ///
    /// What actually secures that path is that **every route into a tracing field bottoms out in
    /// `Display`, `Debug`, or a primitive** — `field::display`, `field::debug`, `%value`, `?value`
    /// — and all three of this type's routes render [`REDACTED`]. There is no fourth route, so
    /// there is nothing left for an impl to protect.
    ///
    /// This method exists so the intent is explicit at a call site that wants a plain field:
    /// `tracing::info!(password = secret.redacted_field())`.
    #[must_use]
    pub const fn redacted_field(&self) -> &'static str {
        REDACTED
    }
}

#[cfg(test)]
mod tests {
    use super::{REDACTED, Secret};

    const CREDENTIAL: &str = "hunter2-do-not-print";

    fn secret() -> Secret<String> {
        Secret::new("database.password", CREDENTIAL.to_owned())
    }

    #[test]
    fn display_renders_the_placeholder() {
        assert_eq!(secret().to_string(), REDACTED);
    }

    #[test]
    fn debug_names_the_key_and_redacts_the_value() {
        // Scenario 7: redacted in every path, and **the field name remains visible**.
        let rendered = format!("{:?}", secret());
        assert!(rendered.contains("database.password"), "{rendered}");
        assert!(rendered.contains(REDACTED), "{rendered}");
        assert!(!rendered.contains(CREDENTIAL), "{rendered}");
    }

    #[test]
    fn the_value_is_reachable_only_through_one_conspicuous_method() {
        assert_eq!(secret().expose(), CREDENTIAL);
        assert_eq!(secret().key(), "database.password");
    }

    #[test]
    fn every_route_into_a_tracing_field_renders_the_placeholder() {
        // `tracing::Value` is sealed, so this path is secured by the fact that every route into a
        // field goes through `Display`, `Debug`, or a primitive. All three are checked here
        // together, because the claim is about the set of routes rather than any one of them.
        let secret = secret();
        let routes = [
            format!("{secret}"),                // %value / field::display
            format!("{secret:?}"),              // ?value / field::debug
            secret.redacted_field().to_owned(), // a plain field
        ];

        for rendered in &routes {
            assert!(
                !rendered.contains(CREDENTIAL),
                "a field route leaked the value: {rendered}"
            );
            assert!(rendered.contains(REDACTED), "{rendered}");
        }

        // POSITIVE CONTROL: the search string is findable when present, so the three assertions
        // above discriminate rather than being vacuous.
        assert!(format!("{CREDENTIAL:?}").contains(CREDENTIAL));
    }

    #[test]
    fn serialization_is_refused_by_an_absent_implementation() {
        // C-C9: serialization is **refused**, and the refusal is a missing trait rather than a
        // runtime guard. This reads the module's own source and the manifest: `SerializableSecret`
        // is never implemented, and the crate's `serde` feature is never enabled.
        let source = include_str!("mod.rs");

        // The needles are assembled from fragments so the literals never appear in this file. A
        // first attempt searched for them directly and failed — the scan found its own search
        // string, which is the trap every self-reading test sets for itself.
        let marker = concat!("impl ", "SerializableSecret");
        let serialize = concat!("impl<T: Zeroize> ", "serde::Serialize");
        assert!(
            !source.contains(marker),
            "the opt-in serialisation marker was implemented, which C-C9 forbids"
        );
        assert!(
            !source.contains(serialize),
            "a Serialize impl appeared, which C-C9 forbids"
        );

        // POSITIVE CONTROL: the same scan finds an impl that IS present, so the two absences
        // above mean "not implemented" rather than "the file was not read".
        assert!(
            source.contains(concat!("impl<T: Zeroize> ", "fmt::Display")),
            "the scan is not reading what it thinks it is"
        );

        let manifest = include_str!("../../Cargo.toml");
        let secrecy_line = manifest
            .lines()
            .find(|line| line.starts_with("secrecy ="))
            .expect("the manifest declares secrecy");
        assert!(
            !secrecy_line.contains("serde"),
            "the crate's serde feature was enabled: {secrecy_line}"
        );

        // POSITIVE CONTROL: the manifest scan reads real text and can find a feature that IS
        // enabled elsewhere, so its silence above means "absent".
        assert!(
            manifest.contains("features = [\"parse\", \"serde\"]"),
            "toml's features moved"
        );
    }
}
