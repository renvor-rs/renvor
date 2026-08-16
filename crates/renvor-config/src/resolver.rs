//! The typed resolver: the two steps of C-C2, wired together.
//!
//! ```text
//!  defaults ─┐
//!  file 1   ─┤ step 1: decode each, independently, against the schema   (C-C3)
//!  file 2   ─┤ ─────────────────────────────────────────────────────────────
//!  env      ─┘ step 2: merge by precedence, attributing every key       (C-C4, C-C5, C-C6)
//!                                    │
//!                                    ▼
//!                    ResolvedConfig<T>  =  value + per-key attribution
//! ```
//!
//! # Failing is the point of each step
//!
//! Step 1 fails on a value that cannot decode, naming the key, the layer, and the expectation.
//! Step 2 fails on a structural conflict, naming the key and **both** layers. Assembly fails on a
//! required key nobody supplied, naming the key. **0** of those three produce a started
//! application (SC-003), because `ApplicationBuilder::build` runs them before `Boot` exists.

use std::collections::BTreeMap;

use renvor_core::KernelError;
use renvor_core::config_port::{Attribution, ConfigResolver, ResolvedConfig, SourceLayer};
use renvor_core::error::context::{Constraint, configuration};
use toml::Table;

use crate::layer::decode::decode_source;
use crate::layer::env::{read_environment, read_process_environment};
use crate::layer::file::FileLayer;
use crate::layer::merge::{DecodedLayer, merge_layers};
use crate::schema::ConfigSchema;

/// Builds a [`LayeredResolver`] from ordered sources.
///
/// Sources are added lowest-precedence first and the order is kept, because that order **is** the
/// precedence (FR-044). A set would discard it silently, which is why this holds a vector.
#[derive(Debug, Default)]
pub struct LayeredResolverBuilder {
    defaults: Option<Table>,
    files: Vec<FileLayer>,
    env_prefix: Option<String>,
    env_override: Option<BTreeMap<String, String>>,
}

impl LayeredResolverBuilder {
    /// Creates a builder with no sources.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Supplies the built-in defaults layer, the lowest precedence.
    #[must_use]
    pub fn with_defaults(mut self, defaults: Table) -> Self {
        self.defaults = Some(defaults);
        self
    }

    /// Appends a TOML file. Later files override earlier ones (C-C4).
    #[must_use]
    pub fn with_file(mut self, file: FileLayer) -> Self {
        self.files.push(file);
        self
    }

    /// Enables the environment layer, the highest precedence, reading variables with `prefix`.
    #[must_use]
    pub fn with_environment(mut self, prefix: impl Into<String>) -> Self {
        self.env_prefix = Some(prefix.into());
        self
    }

    /// Supplies the environment explicitly instead of reading the process environment.
    ///
    /// This exists because `std::env::set_var` is `unsafe` in edition 2024 and this workspace
    /// declares `unsafe_code = "forbid"`. A test cannot mutate the process environment here, so
    /// the resolver accepts one — which also makes environment tests independent of each other and
    /// safe to run in parallel, a property the mutating approach never had.
    #[must_use]
    pub fn with_environment_map(
        mut self,
        prefix: impl Into<String>,
        variables: BTreeMap<String, String>,
    ) -> Self {
        self.env_prefix = Some(prefix.into());
        self.env_override = Some(variables);
        self
    }

    /// Freezes the source list into a resolver for schema `T`.
    #[must_use]
    pub fn build<T: ConfigSchema>(self) -> LayeredResolver<T> {
        LayeredResolver {
            defaults: self.defaults,
            files: self.files,
            env_prefix: self.env_prefix,
            env_override: self.env_override,
            schema: core::marker::PhantomData,
        }
    }
}

/// Resolves configuration for one schema from ordered layers.
#[derive(Debug)]
pub struct LayeredResolver<T: ConfigSchema> {
    defaults: Option<Table>,
    files: Vec<FileLayer>,
    env_prefix: Option<String>,
    env_override: Option<BTreeMap<String, String>>,
    schema: core::marker::PhantomData<fn() -> T>,
}

impl<T: ConfigSchema> LayeredResolver<T> {
    /// Collects and decodes every source, lowest precedence first.
    ///
    /// # Errors
    ///
    /// Returns [`KernelError::Configuration`] for a source that cannot be read or cannot decode.
    fn decoded_layers(&self) -> Result<Vec<DecodedLayer>, KernelError> {
        let mut layers = Vec::with_capacity(self.files.len() + 2);

        if let Some(defaults) = &self.defaults {
            decode_source::<T::Partial>(defaults, &SourceLayer::Defaults)?;
            layers.push(DecodedLayer::new(SourceLayer::Defaults, defaults.clone()));
        }

        for file in &self.files {
            // `None` is an absent optional file: it contributes no layer at all, which is a
            // different fact from a layer that contributed no keys (C-C11).
            if let Some(table) = file.read()? {
                let layer = file.source_layer();
                decode_source::<T::Partial>(&table, &layer)?;
                layers.push(DecodedLayer::new(layer, table));
            }
        }

        if let Some(prefix) = &self.env_prefix {
            let variables = match &self.env_override {
                Some(supplied) => supplied.clone(),
                // Fallible, and deliberately so: reading the real environment is the one place
                // this resolver touches something it did not receive as an argument, and the
                // standard library's Unicode-or-panic reader is not safe to use there.
                None => read_process_environment(prefix)?,
            };
            // The environment layer decodes each variable as it reads it, so it arrives already
            // checked and needs no second pass.
            let table = read_environment::<T::Partial>(prefix, &variables)?;
            layers.push(DecodedLayer::new(SourceLayer::Environment, table));
        }

        Ok(layers)
    }
}

impl<T: ConfigSchema> ConfigResolver for LayeredResolver<T> {
    type Config = T;
    type Error = KernelError;

    fn resolve(&self) -> Result<ResolvedConfig<T>, KernelError> {
        let layers = self.decoded_layers()?;
        let merged = merge_layers(&layers)?;

        let attribution: Vec<(String, Attribution)> = merged.attribution.into_iter().collect();

        // Assembly. Every source has already been decoded and every conflict already refused, so
        // the only failure left here is a required key that nobody supplied — which C-C11 requires
        // to fail naming the key, with 0 silent substitutions of a zero value or an empty string.
        let value = merged.table.try_into::<T>().map_err(|error| {
            configuration(
                missing_key(&error).unwrap_or_else(|| "<schema>".to_owned()),
                "all layers",
                "a complete configuration",
                &Constraint::from_decoder(error.message(), "the merged configuration"),
            )
        })?;

        Ok(ResolvedConfig::new(value, attribution))
    }
}

/// Extracts the field name from a `missing field \`x\`` message.
///
/// Reading `serde`'s wording is not something to be proud of, and it is confined to this one
/// function so that a change in that wording degrades the **key** field rather than the error.
/// The full message is carried in the constraint either way, so nothing is lost when it fails.
fn missing_key(error: &toml::de::Error) -> Option<String> {
    let message = error.message();
    let after = message.split("missing field `").nth(1)?;
    let name = after.split('`').next()?;
    (!name.is_empty()).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{LayeredResolverBuilder, missing_key};
    use crate::schema::ConfigSchema;
    use renvor_core::ErrorCategory;
    use renvor_core::config_port::{ConfigResolver, SourceLayer};
    use serde::Deserialize;
    use std::collections::BTreeMap;
    use toml::Table;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Settings {
        host: String,
        port: u16,
    }

    /// The partial forms exist to be **decoded into**, never read from: proving a source fits the
    /// schema is their whole job. `dead_code` fires because nothing reads the fields back, which is
    /// the intended shape rather than an oversight.
    #[allow(dead_code)]
    #[derive(Debug, Default, Deserialize)]
    struct PartialSettings {
        host: Option<String>,
        port: Option<u16>,
    }

    impl ConfigSchema for Settings {
        type Partial = PartialSettings;
    }

    fn defaults() -> Table {
        "host = \"localhost\"\nport = 80"
            .parse::<Table>()
            .expect("valid")
    }

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn defaults_alone_resolve_and_are_attributed_to_the_defaults_layer() {
        let resolved = LayeredResolverBuilder::new()
            .with_defaults(defaults())
            .build::<Settings>()
            .resolve()
            .expect("defaults are complete");

        assert_eq!(
            resolved.value(),
            &Settings {
                host: "localhost".into(),
                port: 80
            }
        );
        assert_eq!(
            resolved.attribution("port").map(|a| &a.layer),
            Some(&SourceLayer::Defaults)
        );
    }

    #[test]
    fn the_environment_wins_over_the_defaults() {
        // C-C4: environment always wins.
        let resolved = LayeredResolverBuilder::new()
            .with_defaults(defaults())
            .with_environment_map("RENVOR_", env(&[("RENVOR_PORT", "8080")]))
            .build::<Settings>()
            .resolve()
            .expect("resolves");

        assert_eq!(resolved.value().port, 8080);
        assert_eq!(
            resolved.attribution("port").map(|a| &a.layer),
            Some(&SourceLayer::Environment)
        );
        assert_eq!(
            resolved.attribution("host").map(|a| &a.layer),
            Some(&SourceLayer::Defaults),
            "a key the environment did not set keeps its own provenance"
        );
    }

    #[test]
    fn a_required_key_nobody_supplied_fails_naming_it() {
        // C-C11's last row: 0 silent substitutions of a zero value, an empty string, or None.
        let partial = "host = \"localhost\"".parse::<Table>().expect("valid");
        let error = LayeredResolverBuilder::new()
            .with_defaults(partial)
            .build::<Settings>()
            .resolve()
            .expect_err("port has no value anywhere");

        assert_eq!(error.category(), ErrorCategory::Configuration);
        assert!(error.to_string().contains("port"), "{error}");
    }

    #[test]
    fn an_undecodable_environment_value_fails_before_anything_is_assembled() {
        let error = LayeredResolverBuilder::new()
            .with_defaults(defaults())
            .with_environment_map("RENVOR_", env(&[("RENVOR_PORT", "")]))
            .build::<Settings>()
            .resolve()
            .expect_err("an empty string cannot be a u16");

        assert_eq!(error.category(), ErrorCategory::Configuration);
        assert!(error.to_string().contains("port"), "{error}");
        assert!(
            error.to_string().contains("environment"),
            "the layer must be named: {error}"
        );
    }

    #[test]
    fn every_resolved_key_is_attributed() {
        // FR-016 says every key, so this asserts the count.
        let resolved = LayeredResolverBuilder::new()
            .with_defaults(defaults())
            .build::<Settings>()
            .resolve()
            .expect("resolves");
        assert_eq!(resolved.attributions().len(), 2);

        // POSITIVE CONTROL: a key that does not exist reports nothing.
        assert!(resolved.attribution("nope").is_none());
    }

    #[test]
    fn the_missing_key_reader_degrades_rather_than_guessing() {
        // If serde's wording changes, the key field degrades and the constraint still carries the
        // full message. This asserts the reader is precise today and honest about its input.
        let error = "port = \"x\""
            .parse::<Table>()
            .expect("valid")
            .try_into::<Settings>()
            .expect_err("wrong type");
        assert_eq!(
            missing_key(&error),
            None,
            "a type error names no missing key"
        );
    }
}
