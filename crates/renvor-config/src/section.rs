//! Naming a key, its layer, and its constraint from inside a section's validator (FR-011).
//!
//! A capability's typed section checks its values against their caps in the kernel's Validate
//! phase and refuses with a diagnostic that names **3 of 3** — the key, the violated constraint,
//! and the layer that supplied the value (C-C8). Every section needs the same three moves: join
//! the key to the table prefix it sits under, look up the layer that won that key, and build the
//! kernel's bounded configuration error. This type is those three moves, written once.

use renvor_core::KernelError;
use renvor_core::config_port::ResolvedConfig;
use renvor_core::error::context::{Constraint, configuration};

use crate::source::layer_of;

/// The keys of one section of a resolved configuration, under a table prefix.
///
/// `prefix` is empty when the section is the whole schema and `"cache"` when it is the `[cache]`
/// table of a larger one, so the diagnostic says `cache.port` where the operator wrote it.
pub struct SectionKeys<'a, T> {
    prefix: &'a str,
    resolved: &'a ResolvedConfig<T>,
}

impl<'a, T> SectionKeys<'a, T> {
    /// The keys under `prefix` in `resolved`.
    #[must_use]
    pub const fn new(prefix: &'a str, resolved: &'a ResolvedConfig<T>) -> Self {
        Self { prefix, resolved }
    }

    /// The full key `name` sits at: `name`, or `prefix.name`.
    #[must_use]
    pub fn key(&self, name: &str) -> String {
        if self.prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{}.{name}", self.prefix)
        }
    }

    /// The layer that supplied `name`, or `defaults`.
    #[must_use]
    pub fn layer(&self, name: &str) -> String {
        layer_of(self.resolved, &self.key(name))
    }

    /// A refusal of `name` for `constraint`, naming the key, the constraint, and the layer.
    #[must_use]
    pub fn refuse(
        &self,
        name: &str,
        expected: &'static str,
        constraint: &Constraint,
    ) -> KernelError {
        configuration(self.key(name), self.layer(name), expected, constraint)
    }

    /// Checks `value` against an inclusive range, refusing by name outside it.
    ///
    /// # Errors
    ///
    /// [`KernelError::Configuration`] with [`Constraint::OutOfRange`].
    pub fn range(
        &self,
        name: &str,
        value: u128,
        minimum: u128,
        maximum: u128,
    ) -> Result<(), KernelError> {
        if value < minimum || value > maximum {
            return Err(self.refuse(
                name,
                "an integer",
                &Constraint::OutOfRange {
                    minimum: i128::try_from(minimum).unwrap_or(i128::MAX),
                    maximum: i128::try_from(maximum).unwrap_or(i128::MAX),
                },
            ));
        }
        Ok(())
    }

    /// Refuses `name` with a stated rule.
    #[must_use]
    pub fn rule(&self, name: &str, expected: &'static str, rule: &'static str) -> KernelError {
        self.refuse(name, expected, &Constraint::Rule(rule))
    }
}

#[cfg(test)]
mod tests {
    use super::SectionKeys;
    use renvor_core::config_port::{Attribution, Presence, ResolvedConfig, SourceLayer};

    fn resolved() -> ResolvedConfig<()> {
        ResolvedConfig::new(
            (),
            vec![(
                "cache.port".to_owned(),
                Attribution {
                    layer: SourceLayer::Environment,
                    presence: Presence::Present,
                },
            )],
        )
    }

    #[test]
    fn keys_join_the_prefix_and_name_the_winning_layer() {
        let resolved = resolved();
        let keys = SectionKeys::new("cache", &resolved);
        assert_eq!(keys.key("port"), "cache.port");
        assert_eq!(keys.layer("port"), "environment");
        assert_eq!(keys.layer("host"), "defaults");
        let bare = SectionKeys::new("", &resolved);
        assert_eq!(bare.key("port"), "port");
    }

    #[test]
    fn a_range_refusal_names_all_three() {
        let resolved = resolved();
        let keys = SectionKeys::new("cache", &resolved);
        let error = keys.range("port", 70_000, 1, 65_535).unwrap_err();
        let rendered = error.to_string();
        assert!(rendered.contains("`cache.port`"), "{rendered}");
        assert!(rendered.contains("environment"), "{rendered}");
        assert!(rendered.contains("between 1 and 65535"), "{rendered}");
        assert!(keys.range("port", 65_535, 1, 65_535).is_ok());
    }
}
