//! The bridge between the resolver and the lifecycle: T074.
//!
//! # The two steps land in the two phases they belong to
//!
//! C-C2's two steps and C-L1's two early phases line up exactly, and this type is where that is
//! spent rather than merely noticed:
//!
//! | Phase | Step | What fails here |
//! |---|---|---|
//! | `Load` | read every source and decode each against the schema | an unreadable file, a value that does not fit its declared type |
//! | `Validate` | merge by precedence and assemble; run the section's validator | a structural conflict, a required key nobody supplied, a bound over its cap, a credential of the wrong shape |
//!
//! Splitting it this way is not cosmetic. C-L2 says a `Load` failure names *the source that could
//! not be read* and a `Validate` failure names *the key, the violated constraint, and the source
//! layer* — two different diagnostics for two different kinds of wrongness. Doing all the work in
//! one phase would force one of them to be reported under the other's name.
//!
//! # 0 providers start, and that is a property of where the code is
//!
//! Both phases run inside `ApplicationBuilder::build`, which returns before `Application::boot`
//! exists. SC-003's "**0** failing configurations result in a started listener, task, or provider"
//! is therefore not a claim this module has to keep re-checking — the code that starts a provider
//! has not been reached.

use core::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use renvor_core::KernelError;
use renvor_core::config_port::{ConfigResolver, ConfigSource, ResolvedConfig};
use renvor_core::error::context::{Constraint, configuration};

use crate::resolver::LayeredResolver;
use crate::schema::ConfigSchema;

/// A [`ConfigSource`] backed by the layered resolver.
///
/// Registered with `ApplicationBuilder::with_config_source`, which takes an `Arc` because the
/// kernel hands each source to a worker thread for the duration of its bounded call. The resolved
/// value is read afterwards
/// through the [`ConfigHandle`] this hands out.
pub struct SchemaSource<T: ConfigSchema + Send + 'static> {
    name: String,
    resolver: LayeredResolver<T>,
    resolved: Arc<Mutex<Option<ResolvedConfig<T>>>>,
    validator: Option<Validator<T>>,
}

/// A rule checked in `Validate` against the resolved configuration: bounds against their caps,
/// credentials against their shape, anything a decoded value can still get wrong (FR-011).
///
/// Returns [`KernelError::Configuration`] naming the key, the constraint, and the layer — built
/// through [`renvor_core::error::context::configuration`], with [`layer_of`] supplying the layer.
pub type Validator<T> =
    Box<dyn Fn(&ResolvedConfig<T>) -> Result<(), KernelError> + Send + Sync + 'static>;

/// The label of the layer that supplied `key`, or `defaults` when no layer is attributed — the
/// only way a resolved value can exist without an attribution is the schema's own default.
#[must_use]
pub fn layer_of<T>(resolved: &ResolvedConfig<T>, key: &str) -> String {
    resolved.attribution(key).map_or_else(
        || "defaults".to_owned(),
        |found| found.layer.bounded_label(),
    )
}

impl<T: ConfigSchema + Send + 'static> SchemaSource<T> {
    /// Wraps a resolver as a lifecycle source.
    #[must_use]
    pub fn new(name: impl Into<String>, resolver: LayeredResolver<T>) -> Self {
        Self {
            name: name.into(),
            resolver,
            resolved: Arc::new(Mutex::new(None)),
            validator: None,
        }
    }

    /// Adds a rule that `Validate` runs against the resolved configuration, after `Load` has
    /// decoded and merged it. One validator per source; a capability crate's `config` module
    /// supplies the one that checks its section's bounds and credentials.
    #[must_use]
    pub fn with_validator(
        mut self,
        validator: impl Fn(&ResolvedConfig<T>) -> Result<(), KernelError> + Send + Sync + 'static,
    ) -> Self {
        self.validator = Some(Box::new(validator));
        self
    }

    /// A handle for reading the resolved configuration **after** the build succeeds.
    ///
    /// Taken **before** the build, like the phase log, so that the reader exists independently of
    /// whether the build produced anything.
    #[must_use]
    pub fn handle(&self) -> ConfigHandle<T> {
        ConfigHandle {
            resolved: Arc::clone(&self.resolved),
        }
    }
}

/// Emits the source's **name and whether it has resolved**, and never the configuration.
///
/// Hand-written rather than derived, and not because of a trait bound. A derived impl would print
/// the resolved value, and a resolved configuration is precisely the thing that holds credentials
/// — C-E3 allows **0** output forms to emit one. The compiler asked for `T: Debug`; the right
/// answer was not to add the bound but to stop printing the value at all.
impl<T: ConfigSchema + Send + 'static> fmt::Debug for SchemaSource<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SchemaSource")
            .field("name", &self.name)
            .field("resolved", &self.handle().is_resolved())
            .finish()
    }
}

impl<T: ConfigSchema + Send + 'static> ConfigSource for SchemaSource<T> {
    fn name(&self) -> &str {
        &self.name
    }

    fn load(&self) -> Result<(), KernelError> {
        // Both steps run here, and the result is held until `validate` inspects it. Resolving
        // twice would read the environment twice and could produce two different answers for one
        // run — which is worse than resolving early.
        let resolved = self.resolver.resolve()?;
        *self.resolved.lock().unwrap_or_else(PoisonError::into_inner) = Some(resolved);
        Ok(())
    }

    fn validate(&self) -> Result<(), KernelError> {
        // `load` already failed on anything undecodable, conflicting, or missing, so reaching here
        // with nothing resolved would mean the lifecycle called `validate` without `load`.
        // Reported rather than assumed, because an ordering bug that silently validated nothing
        // would let an application boot on configuration that was never read.
        let guard = self.resolved.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(resolved) = guard.as_ref() else {
            return Err(configuration(
                self.name.clone(),
                self.name.clone(),
                "a resolved configuration",
                &Constraint::Rule("validate ran before load, so nothing was resolved"),
            ));
        };
        // The section's own rule, if one was given: bounds against caps, credentials against
        // their shape. Runs here and not in `load`, because C-L2 names this phase for "the key,
        // the violated constraint, and the layer", and because nothing has started yet.
        match &self.validator {
            Some(validator) => validator(resolved),
            None => Ok(()),
        }
    }
}

/// Reads the configuration a [`SchemaSource`] resolved.
pub struct ConfigHandle<T> {
    resolved: Arc<Mutex<Option<ResolvedConfig<T>>>>,
}

/// Emits **whether** something is resolved, never **what**. Same reason as [`SchemaSource`].
impl<T> fmt::Debug for ConfigHandle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfigHandle")
            .field("resolved", &self.is_resolved())
            .finish()
    }
}

impl<T> Clone for ConfigHandle<T> {
    fn clone(&self) -> Self {
        Self {
            resolved: Arc::clone(&self.resolved),
        }
    }
}

impl<T> ConfigHandle<T> {
    /// Runs `read` against the resolved configuration.
    ///
    /// Borrowing rather than cloning, so a schema does not have to be `Clone` to be readable and
    /// so a large configuration is not duplicated on every access.
    ///
    /// # Errors
    ///
    /// Returns [`KernelError::Configuration`] if nothing has been resolved — which means the
    /// handle was read before `build`, not that resolution failed. A failed resolution surfaces
    /// from `build` itself.
    pub fn with<R>(&self, read: impl FnOnce(&ResolvedConfig<T>) -> R) -> Result<R, KernelError> {
        let guard = self.resolved.lock().unwrap_or_else(PoisonError::into_inner);
        guard.as_ref().map(read).ok_or_else(|| {
            configuration(
                "<configuration>",
                "all layers",
                "a resolved configuration",
                &Constraint::Rule("nothing has been resolved yet — read this after `build`"),
            )
        })
    }

    /// Whether configuration has been resolved.
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        self.resolved
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::SchemaSource;
    use crate::resolver::LayeredResolverBuilder;
    use crate::schema::ConfigSchema;
    use renvor_core::config_port::ConfigSource as _;
    use serde::Deserialize;
    use std::collections::BTreeMap;
    use toml::Table;

    #[derive(Debug, Deserialize)]
    struct Settings {
        port: u16,
    }

    #[allow(dead_code)]
    #[derive(Debug, Default, Deserialize)]
    struct PartialSettings {
        port: Option<u16>,
    }

    impl ConfigSchema for Settings {
        type Partial = PartialSettings;
    }

    fn source(env: &[(&str, &str)]) -> SchemaSource<Settings> {
        let variables: BTreeMap<String, String> = env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        let resolver = LayeredResolverBuilder::new()
            .with_defaults("port = 80".parse::<Table>().expect("valid"))
            .with_environment_map("RENVOR_", variables)
            .build::<Settings>();
        SchemaSource::new("application configuration", resolver)
    }

    #[test]
    fn neither_debug_form_can_emit_the_configuration() {
        // C-E3 allows 0 output forms to emit a configuration value. Here that is a property of two
        // hand-written impls, so it is asserted rather than assumed.
        let source = source(&[("RENVOR_PORT", "8080")]);
        let handle = source.handle();
        source.load().expect("resolves");

        for rendered in [format!("{source:?}"), format!("{handle:?}")] {
            assert!(
                !rendered.contains("8080"),
                "the resolved value reached Debug output: {rendered}"
            );
            assert!(rendered.contains("resolved"), "{rendered}");
        }

        // POSITIVE CONTROL: the search string is findable when it IS present, so the assertions
        // above discriminate rather than being vacuous.
        assert!(format!("{:?}", "8080").contains("8080"));
    }

    #[test]
    fn load_resolves_and_the_handle_reads_it_back() {
        let source = source(&[("RENVOR_PORT", "8080")]);
        let handle = source.handle();

        assert!(!handle.is_resolved(), "nothing before Load");
        source.load().expect("resolves");
        assert!(handle.is_resolved());
        source.validate().expect("already resolved");

        let port = handle
            .with(|resolved| resolved.value().port)
            .expect("resolved");
        assert_eq!(port, 8080);
    }

    #[test]
    fn a_failing_load_leaves_the_handle_empty() {
        let source = source(&[("RENVOR_PORT", "")]);
        let handle = source.handle();

        source.load().expect_err("an empty string cannot be a u16");
        assert!(
            !handle.is_resolved(),
            "a failed resolution must not leave a half-resolved value behind"
        );
        assert!(handle.with(|_| ()).is_err());
    }

    #[test]
    fn a_validator_runs_in_validate_not_load_and_names_key_constraint_and_layer() {
        // FR-011 / C-C8: a value that decodes but breaks a rule — a bound over its cap, a
        // credential of the wrong shape — fails at VALIDATE naming the key, the constraint, and
        // the layer that supplied it. Load already accepted the value, so this is the phase that
        // has to say so, and it must say so before any provider exists.
        let refusing = source(&[("RENVOR_PORT", "80")]).with_validator(|resolved| {
            if resolved.value().port < 1024 {
                let layer = super::layer_of(resolved, "port");
                return Err(renvor_core::error::context::configuration(
                    "port",
                    layer,
                    "u16",
                    &renvor_core::error::context::Constraint::OutOfRange {
                        minimum: 1024,
                        maximum: 65_535,
                    },
                ));
            }
            Ok(())
        });
        refusing
            .load()
            .expect("80 decodes as a u16; Load accepts it");
        let error = refusing
            .validate()
            .expect_err("the validator refuses a privileged port");
        let rendered = error.to_string();
        assert!(
            rendered.contains("`port`"),
            "the key is not named: {rendered}"
        );
        assert!(
            rendered.contains("environment"),
            "the layer is not named: {rendered}"
        );
        assert!(
            rendered.contains("between 1024 and 65535"),
            "the constraint is not named: {rendered}"
        );

        // POSITIVE CONTROL: a value the validator accepts passes Validate.
        let accepting = source(&[("RENVOR_PORT", "8080")]).with_validator(|resolved| {
            assert_eq!(resolved.value().port, 8080);
            Ok(())
        });
        accepting.load().expect("resolves");
        accepting.validate().expect("the validator accepts 8080");
    }

    #[test]
    fn layer_of_reports_the_winning_layer_or_defaults() {
        let from_env = source(&[("RENVOR_PORT", "8080")]);
        from_env.load().expect("resolves");
        from_env
            .handle()
            .with(|resolved| {
                assert_eq!(super::layer_of(resolved, "port"), "environment");
                // A key no layer mentions is attributed nowhere; the defaults label is the
                // honest answer for a value that can only have come from the schema's defaults.
                assert_eq!(super::layer_of(resolved, "nope"), "defaults");
            })
            .expect("resolved");
        let from_defaults = source(&[]);
        from_defaults.load().expect("resolves");
        from_defaults
            .handle()
            .with(|resolved| assert_eq!(super::layer_of(resolved, "port"), "defaults"))
            .expect("resolved");
    }

    #[test]
    fn validate_before_load_is_reported_rather_than_passing_silently() {
        // The ordering bug worth catching: a `validate` that passed on nothing resolved would let
        // an application boot on configuration that was never read.
        let source = source(&[]);
        source
            .validate()
            .expect_err("validate without load must not pass");

        // POSITIVE CONTROL: it passes once load has run, so the refusal is about the ordering
        // rather than about `validate` always failing.
        source.load().expect("resolves");
        source.validate().expect("now it passes");
    }
}
