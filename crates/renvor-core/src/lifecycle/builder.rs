//! Assembly: the surface an author hands configuration sources, providers, and entropy to.
//!
//! # Two outcomes, never three
//!
//! [`ApplicationBuilder::build`] runs `Load`, `Validate`, and `Register`, and returns either an
//! [`Application`] or a diagnostic. There is no partially-built application, no warning-and-carry-
//! on, and no degraded mode — User Story 1's whole claim is that an author gets a running
//! application or a reason, and a third outcome is what that claim excludes.
//!
//! Because build fails **before** `Boot`, FR-017's guarantee is structural: **0** providers can
//! have started, since the only code that starts one lives in [`Application::boot`], which this
//! function has not reached.
//!
//! # Why entropy failure is not a `KernelError`
//!
//! [`crate::error::KernelError`]'s `Internal` category means *a defect in Renvor*. An operating
//! system that refuses to supply random bytes — a locked-down sandbox, an exhausted file-descriptor
//! table — is not that. Reporting it as `Internal` would tell an author their framework is broken
//! when their environment is, which is the exact misdirection that category's documentation warns
//! against.
//!
//! So [`BuildError`] has two variants, and the taxonomy keeps meaning what contract C-E1 says it
//! means. Widening `Internal` to cover an environment failure would have been the smaller diff and
//! the worse diagnostic.

use core::fmt;
use std::time::Duration;

use crate::config_port::ConfigSource;
use crate::error::{ErrorCategory, KernelError};
use crate::lifecycle::application::{
    Application, DEFAULT_PROVIDER_DEADLINE, PhaseCursor, PhaseLog,
};
use crate::lifecycle::drain::DEFAULT_DRAIN_BUDGET;
use crate::observe::{EntropySource, EntropyUnavailable, OsEntropy, RunIdentifier};
use crate::provider::{Provider, ProviderRegistry};

/// Why an application could not be assembled.
///
/// Distinct from [`crate::lifecycle::BootFailure`], which is raised after providers have started
/// and therefore carries a rollback report. Nothing has started when this is returned.
#[derive(Debug)]
#[non_exhaustive]
pub enum BuildError {
    /// A `Load`, `Validate`, or `Register` failure.
    Kernel(KernelError),
    /// The entropy source could not supply the bytes the run identifier needs.
    ///
    /// An environment failure, not a kernel defect — see the module documentation.
    Entropy(EntropyUnavailable),
}

impl BuildError {
    /// The kernel error's category, when this is a kernel error.
    ///
    /// Returns `None` for [`Self::Entropy`], which deliberately has no category: inventing one
    /// would put an environment failure into a taxonomy that describes kernel and author
    /// failures.
    #[must_use]
    pub const fn category(&self) -> Option<ErrorCategory> {
        match self {
            Self::Kernel(error) => Some(error.category()),
            Self::Entropy(_) => None,
        }
    }

    /// The kernel error, when this is one.
    #[must_use]
    pub const fn kernel(&self) -> Option<&KernelError> {
        match self {
            Self::Kernel(error) => Some(error),
            Self::Entropy(_) => None,
        }
    }
}

impl From<KernelError> for BuildError {
    fn from(error: KernelError) -> Self {
        Self::Kernel(error)
    }
}

impl From<EntropyUnavailable> for BuildError {
    fn from(error: EntropyUnavailable) -> Self {
        Self::Entropy(error)
    }
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kernel(error) => write!(f, "application could not be assembled: {error}"),
            Self::Entropy(error) => write!(
                f,
                "application could not be assembled: the run identifier needs entropy: {error}"
            ),
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Kernel(error) => Some(error),
            Self::Entropy(error) => Some(error),
        }
    }
}

/// Collects configuration sources, providers, and entropy, then assembles an [`Application`].
///
/// Configuration sources are kept in **declaration order** because that order is load-bearing
/// (FR-044): it decides precedence, and a set would silently discard it.
#[derive(Debug)]
pub struct ApplicationBuilder {
    config_sources: Vec<Box<dyn ConfigSource>>,
    registry: ProviderRegistry,
    drain_budget: Option<Duration>,
    provider_deadline: Option<Duration>,
    entropy: Box<dyn EntropySource>,
    phases: PhaseLog,
}

impl Default for ApplicationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplicationBuilder {
    /// Creates a builder with the production entropy source and no sources or providers.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config_sources: Vec::new(),
            registry: ProviderRegistry::new(),
            drain_budget: None,
            provider_deadline: None,
            entropy: Box::new(OsEntropy::new()),
            phases: PhaseLog::new(),
        }
    }

    /// Appends a configuration source. Order is precedence order (FR-044).
    #[must_use]
    pub fn with_config_source(mut self, source: Box<dyn ConfigSource>) -> Self {
        self.config_sources.push(source);
        self
    }

    /// Registers a provider. Registration order is **not** initialisation order.
    #[must_use]
    pub fn with_provider(mut self, provider: Box<dyn Provider>) -> Self {
        self.registry.register(provider);
        self
    }

    /// Overrides the drain budget.
    ///
    /// `Duration::ZERO` is a **valid** configuration meaning *skip the drain, stop immediately*.
    /// It is neither rejected nor read as "wait forever" (C-L5).
    #[must_use]
    pub const fn with_drain_budget(mut self, budget: Duration) -> Self {
        self.drain_budget = Some(budget);
        self
    }

    /// Overrides how long each provider is given to initialise or to stop.
    ///
    /// FR-025 and C-L7 require the bound to exist; its default value is Renvor's choice rather
    /// than the specification's, which is why it is overridable and why the constant says so.
    #[must_use]
    pub const fn with_provider_deadline(mut self, deadline: Duration) -> Self {
        self.provider_deadline = Some(deadline);
        self
    }

    /// Replaces the entropy source, so a test can fix the run identifier.
    #[must_use]
    pub fn with_entropy(mut self, entropy: Box<dyn EntropySource>) -> Self {
        self.entropy = entropy;
        self
    }

    /// A handle on the phase record, valid **before** the run starts.
    ///
    /// This is the FR-002 inspection point. Taking it here rather than from the finished
    /// application is the whole point: a run that fails during `Validate` never produces an
    /// application, and its phase sequence is exactly the thing a test then wants to read.
    #[must_use]
    pub fn phase_log(&self) -> PhaseLog {
        self.phases.clone()
    }

    /// Runs `Load`, `Validate`, and `Register`.
    ///
    /// # Errors
    ///
    /// - `Load`: the source that could not be read is named; nothing has started (C-L2).
    /// - `Validate`: the key, the violated constraint, and the source layer are named. **0**
    ///   providers are booted and **0** listeners opened (FR-017).
    /// - `Register`: a ceiling breach, an ambiguous capability, a missing dependency, and a cycle
    ///   each produce a distinct diagnostic; **0** cases reach `Boot` (SC-005).
    /// - [`BuildError::Entropy`] if the run identifier's entropy source fails.
    pub fn build(self) -> Result<Application, BuildError> {
        // The run identifier is generated **first**, because FR-043 requires every emitted record
        // to carry it and the very first phase span is emitted by `PhaseCursor::start`. Generating
        // it after `Load` would leave the records a startup failure produces unattributed — which
        // are the ones anybody actually reads.
        let run_id = RunIdentifier::generate(self.entropy.as_ref())?;

        let mut cursor = PhaseCursor::start(self.phases.clone(), run_id);

        // Load — in declaration order, because precedence is that order (FR-044).
        for source in &self.config_sources {
            source.load()?;
        }

        // Validate.
        cursor.advance();
        for source in &self.config_sources {
            source.validate()?;
        }

        // Register.
        cursor.advance();
        let (order, report) = self.registry.resolve()?;

        Ok(Application::new(
            cursor,
            self.registry,
            order,
            report,
            run_id,
            self.drain_budget.unwrap_or(DEFAULT_DRAIN_BUDGET),
            self.provider_deadline.unwrap_or(DEFAULT_PROVIDER_DEADLINE),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{ApplicationBuilder, BuildError, DEFAULT_DRAIN_BUDGET, DEFAULT_PROVIDER_DEADLINE};
    use crate::config_port::{ConfigSource, SourceLayer};
    use crate::error::{ErrorCategory, KernelError};
    use crate::lifecycle::LifecyclePhase;
    use crate::observe::{EntropySource, EntropyUnavailable, FixedEntropy};
    use std::time::Duration;

    /// A source that succeeds, fails to load, or fails to validate — one knob each, so a test
    /// never has to wonder which one fired.
    #[derive(Debug)]
    struct Source {
        layer: SourceLayer,
        load_fails: bool,
        validate_fails: bool,
    }

    impl Source {
        fn ok() -> Box<dyn ConfigSource> {
            Box::new(Self {
                layer: SourceLayer::Defaults,
                load_fails: false,
                validate_fails: false,
            })
        }
        fn unreadable() -> Box<dyn ConfigSource> {
            Box::new(Self {
                layer: SourceLayer::File("missing.toml".into()),
                load_fails: true,
                validate_fails: false,
            })
        }
        fn invalid() -> Box<dyn ConfigSource> {
            Box::new(Self {
                layer: SourceLayer::Environment,
                load_fails: false,
                validate_fails: true,
            })
        }
    }

    impl ConfigSource for Source {
        fn name(&self) -> &str {
            self.layer.label()
        }
        fn load(&self) -> Result<(), KernelError> {
            if self.load_fails {
                return Err(KernelError::Configuration {
                    key: "<file>".into(),
                    constraint: "the file could not be read".into(),
                    layer: self.layer.label().to_owned(),
                    expected_type: "toml document",
                });
            }
            Ok(())
        }
        fn validate(&self) -> Result<(), KernelError> {
            if self.validate_fails {
                return Err(KernelError::Configuration {
                    key: "server.threads".into(),
                    constraint: "must be at least 1".into(),
                    layer: self.layer.label().to_owned(),
                    expected_type: "u16",
                });
            }
            Ok(())
        }
    }

    /// A source that always reports unavailability, standing in for a sandbox that blocks the
    /// operating-system CSPRNG. `FixedEntropy` cannot play this part — it cycles its bytes and
    /// always succeeds — which is why [`EntropyUnavailable::new`] had to become public.
    #[derive(Debug)]
    struct RefusingEntropy;

    impl EntropySource for RefusingEntropy {
        fn fill(&self, _destination: &mut [u8]) -> Result<(), EntropyUnavailable> {
            Err(EntropyUnavailable::new("the sandbox blocks getrandom"))
        }
    }

    fn deterministic() -> ApplicationBuilder {
        ApplicationBuilder::new().with_entropy(Box::new(FixedEntropy::new(vec![7; 32])))
    }

    #[test]
    fn an_empty_application_assembles_and_reaches_register() {
        let builder = deterministic();
        let log = builder.phase_log();
        let application = builder.build().expect("nothing can fail here");

        assert_eq!(application.phase(), LifecyclePhase::Register);
        assert_eq!(
            log.entries(),
            vec![
                LifecyclePhase::Load,
                LifecyclePhase::Validate,
                LifecyclePhase::Register
            ]
        );
        assert_eq!(application.drain_budget(), DEFAULT_DRAIN_BUDGET);
        assert_eq!(application.provider_deadline(), DEFAULT_PROVIDER_DEADLINE);
    }

    #[test]
    fn an_unreadable_source_fails_in_load_and_goes_no_further() {
        // C-L2: nothing has started, and the phase record stops at Load.
        let builder = deterministic().with_config_source(Source::unreadable());
        let log = builder.phase_log();
        let error = builder.build().expect_err("an unreadable source must fail");

        assert_eq!(error.category(), Some(ErrorCategory::Configuration));
        assert!(error.to_string().contains("missing.toml"), "{error}");
        assert_eq!(
            log.entries(),
            vec![LifecyclePhase::Load],
            "Validate must not be entered after Load failed"
        );
    }

    #[test]
    fn an_invalid_value_fails_in_validate_and_never_reaches_register() {
        // FR-017: invalid configuration prevents Boot; 0 providers start. Register is not even
        // entered, so "0 providers booted" is a consequence of the phase record, not a claim.
        let builder = deterministic().with_config_source(Source::invalid());
        let log = builder.phase_log();
        let error = builder.build().expect_err("an invalid value must fail");

        assert_eq!(error.category(), Some(ErrorCategory::Configuration));
        let rendered = error.to_string();
        assert!(rendered.contains("server.threads"), "key: {rendered}");
        assert!(rendered.contains("at least 1"), "constraint: {rendered}");
        assert!(rendered.contains("environment"), "layer: {rendered}");
        assert_eq!(
            log.entries(),
            vec![LifecyclePhase::Load, LifecyclePhase::Validate]
        );
    }

    #[test]
    fn sources_are_loaded_in_declaration_order() {
        // FR-044: order is precedence order. The second source is the one that fails, so reaching
        // its error at all proves the first was attempted before it.
        let builder = deterministic()
            .with_config_source(Source::ok())
            .with_config_source(Source::unreadable());
        let error = builder.build().expect_err("the second source fails");
        assert!(error.to_string().contains("missing.toml"), "{error}");
    }

    #[test]
    fn a_zero_drain_budget_is_accepted_as_a_valid_configuration() {
        // C-L5: zero means "skip the drain", not "invalid" and not "wait forever".
        let application = deterministic()
            .with_drain_budget(Duration::ZERO)
            .build()
            .expect("zero is valid");
        assert_eq!(application.drain_budget(), Duration::ZERO);
    }

    #[test]
    fn an_entropy_failure_is_not_reported_as_a_kernel_defect() {
        // The point of the assertion is the *absence* of a category: an environment failure must
        // not enter a taxonomy that describes kernel and author failures.
        let error = ApplicationBuilder::new()
            .with_entropy(Box::new(RefusingEntropy))
            .build()
            .expect_err("a refusing entropy source must fail the build");

        assert!(matches!(error, BuildError::Entropy(_)));
        assert_eq!(error.category(), None);
        assert!(error.kernel().is_none());

        // POSITIVE CONTROL: a kernel failure *does* report a category, so the `None` above
        // discriminates rather than always holding.
        let kernel = deterministic()
            .with_config_source(Source::invalid())
            .build()
            .expect_err("an invalid value fails");
        assert_eq!(kernel.category(), Some(ErrorCategory::Configuration));
    }

    #[test]
    fn the_run_identifier_is_fixed_by_the_entropy_source() {
        // Determinism for tests (SC-019), and the wiring that FR-043 requires: one identifier per
        // run, from the entropy port.
        let first = deterministic().build().expect("builds").run_id().encode();
        let second = deterministic().build().expect("builds").run_id().encode();
        assert_eq!(first, second, "the same entropy yields the same identifier");

        let different = ApplicationBuilder::new()
            .with_entropy(Box::new(crate::observe::FixedEntropy::new(vec![9; 32])))
            .build()
            .expect("builds")
            .run_id()
            .encode();
        assert_ne!(first, different, "different entropy yields a different one");
    }
}
