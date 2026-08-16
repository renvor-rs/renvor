//! Injecting a failure at a chosen lifecycle phase, and running the application that results.
//!
//! # What "injectable at each of the 7 phases" means concretely
//!
//! C-L9 and SC-009 require failure to be injectable at every phase, with all three behaviours.
//! The phases are not alike, so "inject a failure" means something different in each, and stating
//! it as a table is the only way a reader can check the claim.
//!
//! **All 21 combinations execute.** Not 21 enum values with 11 that the harness quietly ignored,
//! which is what this file used to contain — `Load`, `Validate`, `Register`, `Ready`, and `Drain`
//! each accepted a `Behaviour` and then failed the same way regardless of it.
//!
//! | Phase | What author code is broken | `Fail` | `Panic` | `Hang` |
//! |---|---|---|---|---|
//! | `Load` | `ConfigSource::load` | returns an error | unwinds the worker | blocks past the build deadline |
//! | `Validate` | `ConfigSource::validate` | returns an error | unwinds the worker | blocks past the build deadline |
//! | `Register` | `Provider::dependencies` | names an unprovided capability | unwinds the worker | blocks past the build deadline |
//! | `Boot` | `Provider::initialise` | returns an error | panics **after an await** | never resolves |
//! | `Ready` | `ReadinessContributor::readiness` | reports not-ready | panics, attributed | blocks past the readiness deadline |
//! | `Drain` | a task holding a work permit | releases it and returns | releases it by **unwinding** | keeps it |
//! | `Stop` | `Provider::stop` | returns an error | panics **after an await** | never resolves |
//!
//! `Drain` is the row worth reading twice. `Fail` and `Panic` both *complete* the drain, because
//! the permit is released either way — by an ordinary early return and by RAII during unwinding.
//! `drain.rs` claims exactly that in prose ("an author who takes a permit and returns early …
//! still releases it") and nothing tested the unwinding half until this did.
//!
//! # Three gaps this harness found, all now closed
//!
//! **1. `Load` and `Validate` were unbounded.** They call author-supplied code *synchronously*,
//! and `ApplicationBuilder::build` is not `async`, so there was no deadline around either: a
//! configuration source reading a hung network mount blocked the process indefinitely.
//! `tests/deadlines.rs` enumerated "three kernel-owned waits" and missed these two, because it
//! searched for `.await` and a synchronous call that never returns is not an await.
//!
//! **2. A panicking provider was not contained.** [`Behaviour::Panic`] at `Boot` or `Stop` unwound
//! through the kernel and ended the process. It is now caught by
//! [`renvor_core::provider::contain`], which polls the provider's already-boxed future inside
//! `catch_unwind` — no new dependency, and no `unsafe`, because `Pin<Box<_>>` is `Unpin`.
//!
//! **3. `Register` and `Ready` were unbounded, and both were found by making the other 11
//! combinations real.** `Provider::dependencies` and `ReadinessContributor::readiness` are
//! author methods the kernel called synchronously with no deadline; writing a provider that
//! blocked in an accessor was enough to hang `build` for ever. The kernel-owned wait inventory
//! went from five to eight as a result.
//!
//! Every one of the three was found by writing this harness rather than by reading the
//! requirements, which is the argument for building the injection surface before claiming the
//! phase is complete.

use core::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use renvor_core::config_port::{ConfigSource, SourceLayer};
use renvor_core::error::context::{Constraint, configuration};
use renvor_core::provider::ProviderFuture;
use renvor_core::{
    ApplicationBuilder, CapabilityId, InitContext, KernelError, LifecyclePhase, Liveness, Provider,
    ProviderId, Readiness, ReadinessContributor, ReadinessReport,
};

use crate::injection::{Behaviour, FailureInjectionPoint};

/// How far a harness run got, and how it ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The application reached `Ready`, carrying the readiness report the injection produced.
    ///
    /// Reaching `Ready` **is** the success condition (C-L2), so a failure injected here shows up
    /// in health rather than in the lifecycle. The report is carried so a test can assert *which*
    /// contributor was blamed and *how* — a bare `Ready` marker could not tell a panicking
    /// contributor from a hung one.
    ReadyDegraded(ReadinessReport),
    /// The run failed during `Load`, `Validate`, or `Register`.
    BuildFailed(String),
    /// The run failed during `Boot`, having rolled back.
    BootFailed(String),
    /// The run reached `Ready` and then shut down; the drain was incomplete.
    DrainIncomplete(u32),
    /// The run reached `Ready` and shut down cleanly.
    Stopped,
    /// The injection point cannot be honoured, and why.
    ///
    /// Retained after the last case that used it was fixed. A harness that can only report success
    /// has nowhere to put "this cannot be tested", and the next unreachable combination would be
    /// silently dropped instead of named.
    NotInjectable(String),
}

/// What a harness run observed.
#[derive(Debug)]
pub struct HarnessRun {
    /// The phases the run actually entered.
    pub phases: Vec<LifecyclePhase>,
    /// How it ended.
    pub outcome: Outcome,
    /// Whether the injected failure fired at all.
    pub fired: bool,
    /// Every failure reported by `Stop`, rendered.
    ///
    /// Carried separately from [`Self::outcome`] because a stop failure does **not** change the
    /// shutdown's outcome — C-L4 requires shutdown to continue and report every failure — so
    /// without this the `Stop` injections would be indistinguishable from a clean run.
    pub stop_failures: Vec<String>,
}

impl HarnessRun {
    /// Whether the run reached the given phase.
    #[must_use]
    pub fn reached(&self, phase: LifecyclePhase) -> bool {
        self.phases.contains(&phase)
    }
}

/// How long a `Hang` injection into a **synchronous** callback actually blocks.
///
/// Real elapsed time, unlike the async hangs: a blocked thread cannot be advanced by
/// `tokio::time::pause`, because it is not waiting on the runtime's clock. So the kernel's
/// deadline is set very short in these tests and the hang only has to outlast it. Long enough to
/// be unambiguous, short enough that the whole 21-combination sweep stays under a second.
const SYNCHRONOUS_HANG: Duration = Duration::from_millis(250);

/// The deadline used for the synchronous phases, chosen well under [`SYNCHRONOUS_HANG`].
///
/// A real wall-clock deadline, because `recv_timeout` is an operating-system wait rather than a
/// Tokio timer — `start_paused` does not advance it, and cannot: the blocked worker is a thread,
/// not a task.
const SYNCHRONOUS_DEADLINE: Duration = Duration::from_millis(40);

/// Runs the injected behaviour inside a synchronous author callback.
///
/// One function for all three, so `Load`, `Validate`, and `Register` cannot drift into meaning
/// different things by the same name.
fn behave(behaviour: Behaviour, fired: &Arc<Mutex<bool>>, what: &str) {
    *fired.lock().unwrap_or_else(PoisonError::into_inner) = true;
    match behaviour {
        Behaviour::Fail => {}
        Behaviour::Panic => panic!("this panic was injected by the test harness at {what}"),
        Behaviour::Hang => std::thread::sleep(SYNCHRONOUS_HANG),
    }
}

/// A configuration source that fails, panics, or blocks on demand.
#[derive(Debug)]
struct InjectingSource {
    phase: LifecyclePhase,
    behaviour: Behaviour,
    fired: Arc<Mutex<bool>>,
}

impl InjectingSource {
    /// Runs the injected behaviour if this is the phase it belongs to.
    ///
    /// `Fail` returns the error; `Panic` and `Hang` never get as far as returning one, which is
    /// the point — the kernel has to notice on its own.
    fn fire(&self, phase: LifecyclePhase, what: &str) -> Result<(), KernelError> {
        if self.phase != phase {
            return Ok(());
        }
        behave(self.behaviour, &self.fired, what);
        Err(configuration(
            "injected.key",
            SourceLayer::File("injected-source".into()).label(),
            "an injected failure",
            &Constraint::Rule("this failure was injected by the test harness"),
        ))
    }
}

impl ConfigSource for InjectingSource {
    fn name(&self) -> &str {
        "injected-source"
    }

    fn load(&self) -> Result<(), KernelError> {
        self.fire(LifecyclePhase::Load, "load")
    }

    fn validate(&self) -> Result<(), KernelError> {
        self.fire(LifecyclePhase::Validate, "validate")
    }
}

/// A provider that misbehaves while **declaring** itself, for `Register` injection.
///
/// Separate from [`InjectingProvider`] because the failure happens in a different kind of method:
/// `dependencies` is a synchronous accessor the resolver calls, not a future the kernel awaits.
struct DeclaringProvider {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    behaviour: Behaviour,
    fired: Arc<Mutex<bool>>,
}

impl fmt::Debug for DeclaringProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Hand-written: the derived form would call `dependencies()` and fire the injection while
        // formatting, which is not the phase under test.
        f.debug_struct("DeclaringProvider")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl Provider for DeclaringProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }
    fn provides(&self) -> &[CapabilityId] {
        &self.provides
    }

    fn dependencies(&self) -> &[CapabilityId] {
        behave(self.behaviour, &self.fired, "register");
        // Reached only by `Fail`: the other two never return from `behave` in a usable state.
        // A capability nobody offers is the kernel's own Register refusal, which is what a
        // Register failure actually is.
        &self.provides[1..]
    }

    fn initialise<'a>(&'a self, _context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

/// A readiness contributor that fails, panics, or blocks on demand, for `Ready` injection.
#[derive(Debug)]
struct InjectingContributor {
    behaviour: Behaviour,
    fired: Arc<Mutex<bool>>,
}

impl ReadinessContributor for InjectingContributor {
    fn name(&self) -> &str {
        "injected-contributor"
    }

    fn readiness(&self) -> Readiness {
        behave(self.behaviour, &self.fired, "ready");
        Readiness::NotReady
    }
}

/// Registers [`InjectingContributor`] during Boot, which is how a real one arrives.
#[derive(Debug)]
struct ContributingProvider {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    behaviour: Behaviour,
    fired: Arc<Mutex<bool>>,
}

impl Provider for ContributingProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }
    fn provides(&self) -> &[CapabilityId] {
        &self.provides
    }
    fn initialise<'a>(&'a self, context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
        Box::pin(async move {
            context.register_readiness(Arc::new(InjectingContributor {
                behaviour: self.behaviour,
                fired: Arc::clone(&self.fired),
            }));
            Ok(())
        })
    }
}

/// A provider that fails, or never returns, on demand.
#[derive(Debug)]
struct InjectingProvider {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    dependencies: Vec<CapabilityId>,
    at_boot: Option<Behaviour>,
    at_stop: Option<Behaviour>,
    fired: Arc<Mutex<bool>>,
}

impl InjectingProvider {
    fn mark(&self) {
        *self.fired.lock().unwrap_or_else(PoisonError::into_inner) = true;
    }
}

impl Provider for InjectingProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }
    fn provides(&self) -> &[CapabilityId] {
        &self.provides
    }
    fn dependencies(&self) -> &[CapabilityId] {
        &self.dependencies
    }

    fn initialise<'a>(&'a self, _context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
        Box::pin(async move {
            match self.at_boot {
                Some(Behaviour::Fail) => {
                    self.mark();
                    Err("this failure was injected by the test harness".into())
                }
                Some(Behaviour::Hang) => {
                    self.mark();
                    std::future::pending::<()>().await;
                    unreachable!("a pending future never resolves")
                }
                Some(Behaviour::Panic) => {
                    self.mark();
                    // After a yield, so the panic happens on a *later* poll — the case a
                    // `catch_unwind` around the call site could never reach.
                    tokio::task::yield_now().await;
                    panic!("this panic was injected by the test harness at boot");
                }
                None => Ok(()),
            }
        })
    }

    fn stop(&self) -> ProviderFuture<'_> {
        Box::pin(async move {
            match self.at_stop {
                Some(Behaviour::Fail) => {
                    self.mark();
                    Err("this stop failure was injected by the test harness".into())
                }
                Some(Behaviour::Hang) => {
                    self.mark();
                    std::future::pending::<()>().await;
                    unreachable!("a pending future never resolves")
                }
                Some(Behaviour::Panic) => {
                    self.mark();
                    tokio::task::yield_now().await;
                    panic!("this panic was injected by the test harness at stop");
                }
                None => Ok(()),
            }
        })
    }
}

/// Builds and runs an application with one injected failure.
#[derive(Debug)]
pub struct Harness {
    point: FailureInjectionPoint,
    provider_deadline: Duration,
    drain_budget: Duration,
}

impl Harness {
    /// Prepares a run that injects `point`.
    ///
    /// The deadlines are short because every test using this runs under a paused clock, where a
    /// short deadline and a long one cost the same: nothing.
    #[must_use]
    pub const fn injecting(point: FailureInjectionPoint) -> Self {
        Self {
            point,
            provider_deadline: Duration::from_secs(2),
            drain_budget: Duration::from_secs(2),
        }
    }

    /// Overrides the provider deadline.
    #[must_use]
    pub const fn with_provider_deadline(mut self, deadline: Duration) -> Self {
        self.provider_deadline = deadline;
        self
    }

    /// Overrides the drain budget.
    #[must_use]
    pub const fn with_drain_budget(mut self, budget: Duration) -> Self {
        self.drain_budget = budget;
        self
    }

    /// Runs the application to whatever end the injected failure produces.
    ///
    /// Requires a paused runtime for the `Hang` behaviours — `#[tokio::test(start_paused = true)]`.
    pub async fn run(self) -> HarnessRun {
        let phase = self.point.phase();
        let behaviour = self.point.behaviour();
        let fired = Arc::new(Mutex::new(false));

        let mut builder = ApplicationBuilder::new()
            .with_entropy(Box::new(renvor_core::observe::FixedEntropy::new(vec![
                42;
                32
            ])))
            .with_provider_deadline(self.provider_deadline)
            .with_build_deadline(SYNCHRONOUS_DEADLINE)
            .with_drain_budget(self.drain_budget);

        if matches!(phase, LifecyclePhase::Load | LifecyclePhase::Validate) {
            builder = builder.with_config_source(std::sync::Arc::new(InjectingSource {
                phase,
                behaviour,
                fired: Arc::clone(&fired),
            }));
        }

        // `Register` injects into `Provider::dependencies`, which the resolver calls
        // synchronously. `Fail` returns a capability nobody offers — the kernel's own refusal, and
        // what a Register failure actually is — while `Panic` and `Hang` never return at all.
        if phase == LifecyclePhase::Register {
            builder = builder.with_provider(Box::new(DeclaringProvider {
                id: ProviderId::new("declaring"),
                provides: vec![
                    CapabilityId::new("declared"),
                    CapabilityId::new("nobody-offers-this"),
                ],
                behaviour,
                fired: Arc::clone(&fired),
            }));
        }

        // `Ready` injects into a readiness contributor, registered during Boot as a real one is.
        if phase == LifecyclePhase::Ready {
            builder = builder.with_provider(Box::new(ContributingProvider {
                id: ProviderId::new("contributing"),
                provides: vec![CapabilityId::new("contributing")],
                behaviour,
                fired: Arc::clone(&fired),
            }));
        }

        builder = builder.with_provider(Box::new(InjectingProvider {
            id: ProviderId::new("injected"),
            provides: vec![CapabilityId::new("injected")],
            dependencies: Vec::new(),
            at_boot: (phase == LifecyclePhase::Boot).then_some(behaviour),
            at_stop: (phase == LifecyclePhase::Stop).then_some(behaviour),
            fired: Arc::clone(&fired),
        }));

        let phases = builder.phase_log();
        let read = |fired: &Arc<Mutex<bool>>| *fired.lock().unwrap_or_else(PoisonError::into_inner);

        let application = match builder.build() {
            Ok(application) => application,
            Err(error) => {
                return HarnessRun {
                    phases: phases.entries(),
                    fired: read(&fired),
                    stop_failures: Vec::new(),
                    outcome: Outcome::BuildFailed(error.to_string()),
                };
            }
        };

        let mut application = match application.boot().await {
            Ok(application) => application,
            Err(failure) => {
                return HarnessRun {
                    phases: phases.entries(),
                    fired: read(&fired),
                    stop_failures: Vec::new(),
                    outcome: Outcome::BootFailed(failure.origin().to_string()),
                };
            }
        };

        if phase == LifecyclePhase::Ready {
            // A failure "at Ready" is not a lifecycle failure — reaching Ready is the success
            // condition (C-L2). It is a **health** failure, so that is what is injected, and the
            // contributor registered during Boot is what fires. Asking for readiness is what runs
            // the injected behaviour, so the report is read here rather than discarded.
            application
                .health()
                .set_readiness_deadline(SYNCHRONOUS_DEADLINE);
            let report = application.health().readiness();
            application.health().set_liveness(Liveness::Dead);
            return HarnessRun {
                phases: phases.entries(),
                fired: read(&fired),
                stop_failures: Vec::new(),
                outcome: Outcome::ReadyDegraded(report),
            };
        }

        // Drain injection.
        //
        // All three behaviours run inside a task that holds a work permit, which is the only
        // author-controlled thing the drain waits on. `Hang` keeps the permit and the drain gives
        // up; `Fail` and `Panic` release it — by RAII on the way out and by RAII during unwinding
        // respectively — and the drain completes. That the permit survives an unwind is a claim
        // `drain.rs` makes in prose and nothing tested until this existed.
        let held = (phase == LifecyclePhase::Drain).then(|| {
            *fired.lock().unwrap_or_else(PoisonError::into_inner) = true;
            let permit = application
                .work()
                .begin("injected work")
                .expect("the gate is open before shutdown");
            match behaviour {
                // Kept, so the drain has to time out on its own.
                Behaviour::Hang => Some(permit),
                // Dropped on an ordinary early return.
                Behaviour::Fail => {
                    drop(permit);
                    None
                }
                // Dropped by unwinding, caught here so the harness survives to report.
                Behaviour::Panic => {
                    let caught =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                            let _permit = permit;
                            panic!("this panic was injected by the test harness at drain");
                        }));
                    assert!(caught.is_err(), "the injected drain panic must have fired");
                    None
                }
            }
        });

        let report = application.shutdown().await;
        let stop_failures: Vec<String> = report
            .stop()
            .failures()
            .iter()
            .map(ToString::to_string)
            .collect();
        let outcome = match report.drain().outstanding() {
            0 => Outcome::Stopped,
            outstanding => Outcome::DrainIncomplete(outstanding),
        };
        drop(held);

        HarnessRun {
            phases: phases.entries(),
            fired: read(&fired),
            stop_failures,
            outcome,
        }
    }
}
