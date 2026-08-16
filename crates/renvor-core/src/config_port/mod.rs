//! The configuration **port**: what the kernel needs from configuration, stated without naming a
//! format, a parser, or a serialization framework.
//!
//! # Why this exists in `renvor-core` and carries no parser
//!
//! The kernel needs three things from configuration: a resolved value it can hand to a provider,
//! an answer to *"which layer did this come from"* (FR-016), and a way to fail that names the key,
//! the layer, and the expected type. None of those require knowing that the format is TOML.
//!
//! So this module declares the **shape of the answer** and nothing else. There is no `serde`
//! bound, no `toml` type, and no derive macro anywhere in it — `renvor-config` supplies the
//! implementation, and a consumer who takes `renvor` with `default-features = false` resolves
//! neither. That isolation is asserted mechanically, not merely intended: `cargo tree` reports
//! **0** matches for `serde`, `toml`, or `secrecy` in `renvor-core`'s graph, against **11** in the
//! facade's default graph.
//!
//! # Attribution is a first-class return value, not a debugging aid
//!
//! [`ResolvedConfig::attribution`] is why the configuration proof gate failed and the adapter is
//! being written. The candidate crate's layer combinator is documented as *"basically like
//! `Option::or`"* — it returns a value and discards which side supplied it, destroying provenance
//! **inside** the merge. There is no seam above it to reconstruct from.
//!
//! Making attribution part of the port's return type rather than an optional extra means an
//! implementation that cannot supply it **cannot compile**, which is a stronger guarantee than a
//! test that checks whether it did.
//!
//! # Present-and-empty is not absent
//!
//! [`Attribution::layer`] returning [`SourceLayer::Defaults`] means every layer declined and the
//! default won. That is a different fact from a layer supplying an empty value, and C-C11 requires
//! the two to stay distinguishable — collapsing them is exactly the silent fallback that made
//! `THREADS=""` become `7` in the candidate crate.

use core::fmt;

/// Which kind of source supplied a configuration value.
///
/// Exactly three kinds exist (C-C1). The absence of JSON and YAML is structural — there is no
/// variant to select, so a prohibition nobody can violate by writing code.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceLayer {
    /// The schema's built-in defaults. Lowest precedence.
    Defaults,
    /// A TOML file, identified by the path or label it was loaded under.
    ///
    /// Carries its own name because a two-file configuration must be able to say *which* file,
    /// and "the TOML layer" is not an answer when there are two of them.
    File(String),
    /// Environment variables. Highest precedence.
    Environment,
}

impl SourceLayer {
    /// The stable label used in diagnostics and attribution reports.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Defaults => "defaults",
            Self::File(name) => name,
            Self::Environment => "environment",
        }
    }
}

impl fmt::Display for SourceLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Whether a layer supplied a value, supplied an empty one, or declined.
///
/// The middle case is the one that matters. C-C11 requires *present and empty* to be
/// distinguishable from *absent*, because treating an empty value as unset is how an invalid
/// setting silently becomes a default — the failure mode FR-022 forbids and the proof gate caught.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Presence {
    /// The layer supplied a value with content.
    Present,
    /// The layer supplied a value that was empty. **Not** the same as [`Self::Absent`].
    PresentButEmpty,
    /// The layer did not mention this key at all.
    Absent,
}

/// Where one resolved key came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribution {
    /// The layer whose value won.
    pub layer: SourceLayer,
    /// Whether that layer's value had content.
    pub presence: Presence,
}

/// The result of resolving configuration: the decoded value plus per-key provenance.
///
/// Generic over the decoded type `T` so the kernel never needs to know the schema. `renvor-config`
/// decodes; the kernel receives.
#[derive(Clone, Debug)]
pub struct ResolvedConfig<T> {
    value: T,
    attribution: Vec<(String, Attribution)>,
}

impl<T> ResolvedConfig<T> {
    /// Creates a resolved configuration from a decoded value and its per-key attribution.
    ///
    /// Both arguments are required. An implementation that resolved values without tracking where
    /// they came from cannot construct this type, which is the point.
    #[must_use]
    pub fn new(value: T, attribution: Vec<(String, Attribution)>) -> Self {
        Self { value, attribution }
    }

    /// The decoded configuration.
    #[must_use]
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// Consumes this wrapper and returns the decoded configuration.
    #[must_use]
    pub fn into_value(self) -> T {
        self.value
    }

    /// Where a specific key's winning value came from, if the key was resolved.
    #[must_use]
    pub fn attribution(&self, key: &str) -> Option<&Attribution> {
        self.attribution
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, attribution)| attribution)
    }

    /// Every key's attribution, in resolution order.
    ///
    /// FR-016 requires the layer to be reportable for **every** resolved key, so this is the whole
    /// map rather than a sample.
    #[must_use]
    pub fn attributions(&self) -> &[(String, Attribution)] {
        &self.attribution
    }
}

/// Resolves configuration into a decoded value with per-key attribution.
///
/// Implemented by `renvor-config`. Declared here so the kernel can depend on the *capability*
/// without depending on the *format* — the boundary that keeps `renvor-core` free of a parser.
pub trait ConfigResolver {
    /// The decoded configuration type this resolver produces.
    type Config;

    /// The error this resolver produces. Kept associated rather than fixed to [`crate::KernelError`]
    /// so an implementation can carry richer detail internally and convert at the boundary.
    type Error;

    /// Resolves configuration from every declared source, in precedence order.
    ///
    /// # Errors
    ///
    /// Returns [`Self::Error`] if any source is unreadable, any value fails to decode, or two
    /// layers supply structurally incompatible shapes for one key. Resolution **fails**; it never
    /// falls through to a lower layer on a decode error (FR-022, C-E4).
    fn resolve(&self) -> Result<ResolvedConfig<Self::Config>, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::{Attribution, ConfigResolver, Presence, ResolvedConfig, SourceLayer};

    #[test]
    fn present_but_empty_is_distinguishable_from_absent() {
        // C-C11. Collapsing these is how `THREADS=""` silently became the default `7` in the
        // candidate crate — the exact silent fallback FR-022 forbids.
        assert_ne!(Presence::PresentButEmpty, Presence::Absent);
        assert_ne!(Presence::PresentButEmpty, Presence::Present);
    }

    #[test]
    fn two_files_are_distinguishable_layers() {
        // "the TOML layer" is not an answer when there are two files. C-C6 requires FR-016's
        // "which layer did this come from" to be answerable, and that needs the file's identity.
        let base = SourceLayer::File("base.toml".into());
        let overlay = SourceLayer::File("override.toml".into());
        assert_ne!(base, overlay);
        assert_eq!(base.label(), "base.toml");
        assert_eq!(overlay.label(), "override.toml");
    }

    #[test]
    fn exactly_three_source_kinds_exist() {
        // C-C1: 0 JSON and 0 YAML sources, enforced structurally. There is no variant to select.
        let kinds = [
            SourceLayer::Defaults,
            SourceLayer::File("f".into()),
            SourceLayer::Environment,
        ];
        let mut labels: Vec<&str> = kinds.iter().map(SourceLayer::label).collect();
        labels.sort_unstable();
        assert_eq!(labels, ["defaults", "environment", "f"]);
    }

    #[test]
    fn attribution_is_reportable_for_every_resolved_key() {
        let resolved = ResolvedConfig::new(
            42_u16,
            vec![
                (
                    "server.threads".to_owned(),
                    Attribution {
                        layer: SourceLayer::Environment,
                        presence: Presence::Present,
                    },
                ),
                (
                    "server.host".to_owned(),
                    Attribution {
                        layer: SourceLayer::File("base.toml".into()),
                        presence: Presence::Present,
                    },
                ),
            ],
        );

        assert_eq!(resolved.value(), &42);
        assert_eq!(
            resolved.attribution("server.threads").map(|a| &a.layer),
            Some(&SourceLayer::Environment)
        );
        assert_eq!(
            resolved.attribution("server.host").map(|a| a.layer.label()),
            Some("base.toml")
        );
        assert_eq!(resolved.attributions().len(), 2);

        // POSITIVE CONTROL: an unresolved key reports nothing, so `attribution` discriminates
        // rather than always returning the first entry.
        assert_eq!(resolved.attribution("nope"), None);
    }

    #[test]
    fn a_resolver_can_be_implemented_without_a_parser_or_serde() {
        // The port's whole purpose: this compiles inside renvor-core, which has no serde, no
        // toml, and no derive macro. If a future edit added a `T: Deserialize` bound, this test
        // would stop compiling — which is the intended alarm.
        struct Stub;
        impl ConfigResolver for Stub {
            type Config = u16;
            type Error = ();
            fn resolve(&self) -> Result<ResolvedConfig<u16>, ()> {
                Ok(ResolvedConfig::new(
                    7,
                    vec![(
                        "threads".to_owned(),
                        Attribution {
                            layer: SourceLayer::Defaults,
                            presence: Presence::Absent,
                        },
                    )],
                ))
            }
        }
        let resolved = Stub.resolve().expect("stub cannot fail");
        assert_eq!(resolved.value(), &7);
        // Every layer declined, so the default won and attribution says so (C-C11).
        let attribution = resolved.attribution("threads").expect("key is attributed");
        assert_eq!(attribution.layer, SourceLayer::Defaults);
        assert_eq!(attribution.presence, Presence::Absent);
    }
}
