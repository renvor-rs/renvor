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
//! # `Load` and `Validate` are bounded, and how
//!
//! Both phases call **author-supplied code**, and `build` is synchronous — so a configuration
//! source reading a hung network mount would block the process for ever. FR-025 and C-L7 prohibit
//! exactly that, and an earlier version of this file was in breach: the defect was found by
//! building the failure-injection harness, not by reading the requirement.
//!
//! `tokio::time::timeout` cannot fix it. A blocking call inside an async block never yields, so
//! the timer never gets to fire; bounding a *synchronous* call needs a second thread whatever
//! else changes. Making `build` async would therefore have churned every call site **and still**
//! needed `spawn_blocking` underneath.
//!
//! So each source call runs on its own thread, and the kernel waits on a channel with
//! [`std::sync::mpsc::Receiver::recv_timeout`]. The signature does not change, and the kernel's
//! wait is bounded.
//!
//! **The honest cost**: a thread that never returns is **leaked**. It cannot be cancelled — no
//! Rust API can interrupt a blocked thread — so the process keeps one stuck thread per hung
//! source. That is strictly better than the alternative, which was keeping the *whole application*
//! stuck, and it is stated here rather than discovered. It is recorded as an open item in
//! `governance/phase-002-evidence.md` and it does not go away in this phase.
//!
//! Because threads leak, the process can eventually run out of them — so the **spawn itself** has
//! to be fallible. [`std::thread::spawn`] panics when the operating system refuses; the builder
//! form returns that refusal, and this module uses the builder form. Reaching a thread ceiling is
//! a foreseeable consequence of the leak above, which makes an infallible spawn here the exact
//! wrong choice rather than a theoretical one.
//!
//! **The unplanned benefit**: a source that **panics** kills only its own thread, dropping the
//! channel sender. The wait ends with a disconnect rather than an unwind through `build`, so a
//! panicking configuration source is a **failure** instead of a process abort — which is what
//! C-L9 asks for, obtained here as a property of the mechanism rather than as a second feature.
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
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use crate::config_port::ConfigSource;
use crate::error::{ErrorCategory, KernelError, context};
use crate::lifecycle::application::{
    Application, DEFAULT_BUILD_DEADLINE, DEFAULT_PROVIDER_DEADLINE, PhaseCursor, PhaseLog,
    deadline_millis,
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
    /// `Arc` rather than `Box` because each source is handed to a worker thread for the duration
    /// of its bounded call, and a thread needs an owned `'static` handle.
    config_sources: Vec<Arc<dyn ConfigSource>>,
    registry: ProviderRegistry,
    drain_budget: Option<Duration>,
    provider_deadline: Option<Duration>,
    build_deadline: Option<Duration>,
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
            build_deadline: None,
            entropy: Box::new(OsEntropy::new()),
            phases: PhaseLog::new(),
        }
    }

    /// Appends a configuration source. Order is precedence order (FR-044).
    #[must_use]
    pub fn with_config_source(mut self, source: Arc<dyn ConfigSource>) -> Self {
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

    /// Overrides how long each configuration source is given to load or to validate.
    ///
    /// FR-025 and C-L7 require the bound to exist; no artifact names a value, so the default is
    /// Renvor's choice and is overridable — see [`DEFAULT_BUILD_DEADLINE`].
    #[must_use]
    pub const fn with_build_deadline(mut self, deadline: Duration) -> Self {
        self.build_deadline = Some(deadline);
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

    /// Reads every source's name once, each read bounded.
    ///
    /// A name that cannot be read inside the deadline degrades to the registration position. That
    /// is **not** the silent fallback FR-022 prohibits: nothing about the configuration is being
    /// guessed, no layer is being skipped, and the deadline was enforced. Only the *label* on a
    /// diagnostic degrades — and refusing to assemble an application because an accessor was slow
    /// would punish the author for a defect that has not yet affected anything they configured.
    fn source_names(&self, deadline: Duration) -> Vec<String> {
        self.config_sources
            .iter()
            .enumerate()
            .map(|(position, source)| {
                let handle = Arc::clone(source);
                bounded_call(deadline, move || handle.name().to_owned())
                    .unwrap_or_else(|_| format!("<at registration position {position}>"))
            })
            .collect()
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
    pub fn build(mut self) -> Result<Application, BuildError> {
        // The run identifier is generated **first**, because FR-043 requires every emitted record
        // to carry it and the very first phase span is emitted by `PhaseCursor::start`. Generating
        // it after `Load` would leave the records a startup failure produces unattributed — which
        // are the ones anybody actually reads.
        // `EntropySource::fill` is author-supplied too, and it is the one callback with a
        // *routine* reason to block: a source reading `/dev/random` on a freshly-booted machine
        // with a shallow entropy pool waits, by design, for as long as it takes.
        let deadline = self.build_deadline.unwrap_or(DEFAULT_BUILD_DEADLINE);
        let entropy = core::mem::replace(&mut self.entropy, Box::new(OsEntropy::new()));
        let run_id = bounded_call(deadline, move || RunIdentifier::generate(entropy.as_ref()))
            .map_err(|failure| entropy_failure(failure, deadline))??;

        let mut cursor = PhaseCursor::start(self.phases.clone(), run_id);

        // Load — in declaration order, because precedence is that order (FR-044) — and each call
        // bounded, because it runs author code the kernel cannot otherwise interrupt.
        //
        // `ConfigSource::name` is an author method too, so it is read **once, inside a bound**,
        // and the answer reused. Calling it from the parent to label an error would be an
        // unbounded call on the failure path — the path that runs precisely when the source is
        // misbehaving — and calling it per phase would ask a misbehaving source twice.
        let names = self.source_names(deadline);

        for (source, name) in self.config_sources.iter().zip(&names) {
            let handle = Arc::clone(source);
            bounded_call(deadline, move || handle.load())
                .map_err(|failure| source_failure(failure, deadline, name, "load"))??;
        }

        // Validate.
        cursor.advance();
        for (source, name) in self.config_sources.iter().zip(&names) {
            let handle = Arc::clone(source);
            bounded_call(deadline, move || handle.validate())
                .map_err(|failure| source_failure(failure, deadline, name, "validate"))??;
        }

        // Register — bounded for the same reason `Load` and `Validate` are, and it is the same
        // reason: `Provider::id`, `provides`, and `dependencies` are **author-supplied methods**
        // the resolver calls synchronously. They look like field reads, but "looks like a field
        // read" is not a bound, and FR-025 prohibits an unbounded wait in any kernel-owned path
        // regardless of how unlikely the author is to block in an accessor.
        //
        // `examined` is shared with the worker so that a provider which panics or hangs while
        // being asked can still be named — by registration position, since asking for its `id` is
        // exactly what failed.
        cursor.advance();
        let examined = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&examined);
        let registry = self.registry;
        let (registry, resolved) = bounded_call(deadline, move || {
            let outcome = registry.resolve_tracking(&counter);
            (registry, outcome)
        })
        .map_err(|failure| register_failure(failure, deadline, examined.load(Ordering::Relaxed)))?;
        let (order, report) = resolved?;

        Ok(Application::new(
            cursor,
            registry,
            order,
            report,
            run_id,
            self.drain_budget.unwrap_or(DEFAULT_DRAIN_BUDGET),
            self.provider_deadline.unwrap_or(DEFAULT_PROVIDER_DEADLINE),
        ))
    }
}

/// Why a bounded call produced no value.
///
/// Deliberately **not** a [`KernelError`]. Every phase that bounds an author callback has to
/// attribute the failure to *itself* — a hung provider declaration is not a configuration error —
/// and returning a ready-made `KernelError` from here would force one phase's wording onto all of
/// them. Each call site maps these three cases to its own diagnostic.
#[derive(Debug)]
enum BoundedFailure {
    /// The host refused the worker thread.
    SpawnRefused(std::io::Error),
    /// The deadline elapsed with no answer.
    Deadline,
    /// The sender was dropped without sending, so the worker unwound: the call **panicked**.
    Panicked,
}

/// Runs one synchronous author callback on a worker thread, waiting no longer than `deadline`.
///
/// See the module documentation for why this is a thread rather than a `tokio::time::timeout`,
/// and what it costs.
///
/// # Errors
///
/// Returns [`BoundedFailure`], which the caller converts into a phase-appropriate diagnostic.
fn bounded_call<T: Send + 'static>(
    deadline: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, BoundedFailure> {
    bounded_call_with(spawn_worker, deadline, work)
}

/// Converts a bounded-call failure into an entropy diagnostic.
///
/// Never [`BuildError::Entropy`]: that variant means *the source said no*, which is a different
/// fact from *the source never answered*. Collapsing them would report a hung `/dev/random` as a
/// refusal it never made.
fn entropy_failure(failure: BoundedFailure, deadline: Duration) -> KernelError {
    let subject = "generate the run identifier from the entropy source".to_owned();
    match failure {
        BoundedFailure::SpawnRefused(cause) => KernelError::ResourceUnavailable {
            resource: "an operating-system thread",
            operation: subject,
            cause: cause.to_string(),
        },
        BoundedFailure::Deadline => KernelError::DeadlineExceeded {
            operation: subject,
            deadline_ms: deadline_millis(deadline),
        },
        BoundedFailure::Panicked => KernelError::ResourceUnavailable {
            resource: "entropy",
            operation: subject,
            cause: "the entropy source panicked while being called".to_owned(),
        },
    }
}

/// Converts a bounded-call failure into a `Load`/`Validate` diagnostic naming the source.
fn source_failure(
    failure: BoundedFailure,
    deadline: Duration,
    source: &str,
    call: &'static str,
) -> KernelError {
    match failure {
        BoundedFailure::SpawnRefused(cause) => KernelError::ResourceUnavailable {
            resource: "an operating-system thread",
            operation: format!("{call} configuration source `{source}`"),
            cause: cause.to_string(),
        },
        BoundedFailure::Deadline => KernelError::DeadlineExceeded {
            operation: format!("{call} configuration source `{source}`"),
            deadline_ms: deadline_millis(deadline),
        },
        BoundedFailure::Panicked => context::configuration(
            source,
            source,
            "a configuration source that returns",
            &context::Constraint::Rule("the source panicked while being called"),
        ),
    }
}

/// Converts a bounded-call failure into a `Register` diagnostic.
///
/// `examined` is the registration position the resolver had reached, so a panicking or hanging
/// provider is named by **where it was registered** even though its own `id` could not be asked
/// for — asking is what failed.
fn register_failure(failure: BoundedFailure, deadline: Duration, examined: u32) -> KernelError {
    let subject = format!(
        "declare the capabilities of the provider at registration position {examined} during \
         Register"
    );
    match failure {
        BoundedFailure::SpawnRefused(cause) => KernelError::ResourceUnavailable {
            resource: "an operating-system thread",
            operation: subject,
            cause: cause.to_string(),
        },
        BoundedFailure::Deadline => KernelError::DeadlineExceeded {
            operation: subject,
            deadline_ms: deadline_millis(deadline),
        },
        BoundedFailure::Panicked => KernelError::ProviderInit {
            provider: format!("<at registration position {examined}>"),
            source: Box::new(crate::provider::Panicked::describing(
                "the provider panicked while declaring its capabilities during Register",
            )),
        },
    }
}

/// How a bounded call obtains its worker thread.
///
/// A function pointer rather than a direct call, because a test cannot make the operating system
/// refuse a thread — not without `libc` and an `unsafe` `setrlimit`, and this workspace forbids
/// both. This is the seam: the production path passes [`spawn_worker`], and a test passes a
/// spawner that always refuses, which exercises the same code the real refusal would.
type Spawner = fn(Box<dyn FnOnce() + Send + 'static>) -> std::io::Result<()>;

/// Starts a worker thread and **deliberately drops its handle**.
///
/// [`std::thread::Builder::spawn`] rather than [`std::thread::spawn`]: the free function
/// **panics** when the operating system refuses, which would take down a process that had every
/// right to be told "no threads available" as an error. The builder form returns that refusal.
///
/// Dropping the handle is the no-join behaviour, unchanged: joining a thread that never returns is
/// the unbounded wait the caller exists to remove. Naming the thread costs nothing and makes a
/// leaked one identifiable in a debugger, which matters because leaking is a known outcome here.
fn spawn_worker(work: Box<dyn FnOnce() + Send + 'static>) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("renvor-config-source".to_owned())
        .spawn(work)
        .map(drop)
}

/// [`bounded_call`] with the spawner supplied, so the refusal path is reachable from a test.
fn bounded_call_with<T: Send + 'static>(
    spawn: Spawner,
    deadline: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, BoundedFailure> {
    let (sender, receiver) = channel();

    if let Err(cause) = spawn(Box::new(move || {
        // A send failure means the receiver already timed out and gave up. Nothing to report to,
        // and nothing to do about it.
        drop(sender.send(work()));
    })) {
        // Fail closed. The alternative — carrying on without the callback's answer — is the silent
        // fallback FR-022 prohibits, and it would boot an application on something nobody read.
        return Err(BoundedFailure::SpawnRefused(cause));
    }

    match receiver.recv_timeout(deadline) {
        Ok(value) => Ok(value),
        Err(RecvTimeoutError::Timeout) => Err(BoundedFailure::Deadline),
        Err(RecvTimeoutError::Disconnected) => Err(BoundedFailure::Panicked),
    }
}

#[cfg(test)]
mod tests {
    use super::{ApplicationBuilder, BuildError, DEFAULT_DRAIN_BUDGET, DEFAULT_PROVIDER_DEADLINE};
    use crate::config_port::{ConfigSource, SourceLayer};
    use crate::error::{ErrorCategory, KernelError};
    use crate::lifecycle::LifecyclePhase;
    use crate::observe::{EntropySource, EntropyUnavailable, FixedEntropy};
    use std::sync::Arc;
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
        fn ok() -> Arc<dyn ConfigSource> {
            Arc::new(Self {
                layer: SourceLayer::Defaults,
                load_fails: false,
                validate_fails: false,
            })
        }
        fn unreadable() -> Arc<dyn ConfigSource> {
            Arc::new(Self {
                layer: SourceLayer::File("missing.toml".into()),
                load_fails: true,
                validate_fails: false,
            })
        }
        fn invalid() -> Arc<dyn ConfigSource> {
            Arc::new(Self {
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

    /// A source that never returns from `load`, standing in for a hung network mount.
    #[derive(Debug)]
    struct HangingSource;

    impl ConfigSource for HangingSource {
        fn name(&self) -> &str {
            "hanging-source"
        }
        fn load(&self) -> Result<(), KernelError> {
            // Deliberately unbounded and uninterruptible: no Rust API can cancel this thread,
            // which is exactly the situation the deadline exists for.
            loop {
                std::thread::park();
            }
        }
    }

    /// A source that panics instead of answering.
    #[derive(Debug)]
    struct PanickingSource;

    impl ConfigSource for PanickingSource {
        fn name(&self) -> &str {
            "panicking-source"
        }
        fn load(&self) -> Result<(), KernelError> {
            panic!("this configuration source is deliberately broken");
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
    fn a_hanging_source_is_bounded_rather_than_blocking_the_process() {
        // FR-025 and C-L7. Before this bound existed, `build` blocked for ever here — the defect
        // was found by building the failure-injection harness, and this is the test that would
        // have found it first.
        let started = std::time::Instant::now();

        let error = deterministic()
            .with_build_deadline(Duration::from_millis(120))
            .with_config_source(Arc::new(HangingSource))
            .build()
            .expect_err("a source that never returns must not hang the build");

        assert_eq!(error.category(), Some(ErrorCategory::DeadlineExceeded));
        let rendered = error.to_string();
        assert!(rendered.contains("hanging-source"), "names it: {rendered}");
        assert!(rendered.contains("load"), "names the call: {rendered}");
        assert!(rendered.contains("120"), "names the deadline: {rendered}");

        // Reaching this at all is the assertion; the timing bound catches a deadline that is
        // honoured in name only.
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the build took {:?}, so the deadline is not being enforced",
            started.elapsed()
        );
    }

    #[test]
    fn a_panicking_source_is_a_failure_rather_than_a_process_abort() {
        // Obtained as a property of the mechanism rather than as a second feature: the source's
        // thread unwinds, dropping the channel sender, so the wait ends with a disconnect instead
        // of an unwind through `build`. Reaching the assertions proves the panic was contained.
        let error = deterministic()
            .with_config_source(Arc::new(PanickingSource))
            .build()
            .expect_err("a panicking source must fail the build");

        assert_eq!(error.category(), Some(ErrorCategory::Configuration));
        let rendered = error.to_string();
        assert!(rendered.contains("panicking-source"), "{rendered}");
        assert!(
            rendered.contains("panicked"),
            "and says what happened: {rendered}"
        );
    }

    #[test]
    fn a_prompt_source_is_untouched_by_the_deadline() {
        // POSITIVE CONTROL for both tests above: the bound discriminates rather than failing every
        // build. Without this, a deadline of zero would satisfy the hang test.
        let application = deterministic()
            .with_build_deadline(Duration::from_millis(120))
            .with_config_source(Source::ok())
            .build()
            .expect("a source that answers immediately is unaffected");
        assert_eq!(application.phase(), LifecyclePhase::Register);
    }

    #[test]
    fn the_build_deadline_has_a_default_that_is_overridable() {
        // The value is Renvor's choice, so what matters is that it is stated in one place and can
        // be replaced.
        assert_eq!(
            super::DEFAULT_BUILD_DEADLINE,
            Duration::from_secs(10),
            "shorter than the provider deadline: reading a file is not opening a pool"
        );
        deterministic()
            .with_build_deadline(Duration::from_millis(1))
            .build()
            .expect("an override is accepted");
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

    #[test]
    fn a_refused_worker_thread_is_reported_rather_than_panicking() {
        // T113. `std::thread::spawn` panics when the operating system refuses a thread, and this
        // module leaks one thread per hung source — so running out is a foreseeable consequence of
        // the design, not a hypothetical. The refusal is reachable only through the spawner seam,
        // because making the real OS refuse would need `libc` and an `unsafe` `setrlimit`.
        fn always_refuses(_work: Box<dyn FnOnce() + Send + 'static>) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::OutOfMemory,
                "simulated thread-table exhaustion",
            ))
        }

        let deadline = Duration::from_secs(30);
        let failure =
            super::bounded_call_with(always_refuses, deadline, || Ok::<(), KernelError>(()))
                .expect_err("a refused thread must be an error, not a panic");
        let error = super::source_failure(failure, deadline, "application configuration", "load");

        // Typed: its own category, because the host refusing a thread is neither a kernel defect
        // nor a deadline that elapsed. Reporting it as either would print a false statement.
        assert_eq!(error.category(), ErrorCategory::ResourceUnavailable);
        assert!(
            !error.is_kernel_defect(),
            "an exhausted host is not a defect in Renvor"
        );

        // Attributed: the message names the source and the call, so an operator knows which one.
        let rendered = error.to_string();
        for expected in ["application configuration", "load", "thread"] {
            assert!(
                rendered.contains(expected),
                "{expected} missing: {rendered}"
            );
        }
        assert!(
            rendered.contains("simulated thread-table exhaustion"),
            "the operating system's own reason must survive: {rendered}"
        );
    }

    #[test]
    fn the_real_spawner_succeeds_and_the_deadline_still_bounds_it() {
        // POSITIVE CONTROL for the test above, in both halves. Without the first assertion,
        // `bounded_call_with` could be refusing everything; without the second, the seam could
        // have quietly replaced the bounded wait with an unbounded one.
        let value = super::bounded_call_with(super::spawn_worker, Duration::from_secs(30), || 7_u8)
            .expect("the real spawner supplies a thread");
        assert_eq!(value, 7, "the worker's result comes back");

        let deadline = Duration::from_millis(10);
        let failure = super::bounded_call_with(super::spawn_worker, deadline, || {
            std::thread::sleep(Duration::from_secs(30));
            0_u8
        })
        .expect_err("a source that outlives its deadline is cut off");
        let error = super::source_failure(failure, deadline, "application configuration", "load");
        assert_eq!(error.category(), ErrorCategory::DeadlineExceeded);
    }

    #[test]
    fn every_bounded_failure_maps_to_a_distinct_diagnostic_at_each_phase() {
        // T114. The point of `BoundedFailure` is that one phase's wording is not forced onto
        // another's, so this asserts both mappings side by side rather than one in isolation.
        let deadline = Duration::from_secs(1);
        let cases = [
            (
                super::BoundedFailure::SpawnRefused(std::io::Error::other("refused")),
                ErrorCategory::ResourceUnavailable,
            ),
            (
                super::BoundedFailure::Deadline,
                ErrorCategory::DeadlineExceeded,
            ),
            (
                super::BoundedFailure::Panicked,
                ErrorCategory::Configuration,
            ),
        ];
        for (failure, expected) in cases {
            let error = super::source_failure(failure, deadline, "a-source", "load");
            assert_eq!(error.category(), expected, "{error}");
            assert!(error.to_string().contains("a-source"), "{error}");
        }

        // The same three failures at Register produce Register's diagnostics, and the panic case
        // is a PROVIDER failure rather than a configuration one — which is the whole reason
        // `BoundedFailure` exists instead of a ready-made `KernelError`.
        let register = [
            (
                super::BoundedFailure::SpawnRefused(std::io::Error::other("refused")),
                ErrorCategory::ResourceUnavailable,
            ),
            (
                super::BoundedFailure::Deadline,
                ErrorCategory::DeadlineExceeded,
            ),
            (super::BoundedFailure::Panicked, ErrorCategory::ProviderInit),
        ];
        for (failure, expected) in register {
            let error = super::register_failure(failure, deadline, 3);
            assert_eq!(error.category(), expected, "{error}");
            assert!(
                error.to_string().contains('3'),
                "the registration position must survive: {error}"
            );
        }
    }
}
