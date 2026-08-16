//! Injecting a failure at a chosen lifecycle phase, and running the application that results.
//!
//! # What "injectable at each of the 7 phases" means concretely
//!
//! C-L9 and SC-009 require failure to be injectable at every phase. The phases are not alike, so
//! "inject a failure" means something different in each, and stating it as a table is the only way
//! a reader can check the claim:
//!
//! | Phase | What is injected | Bounded by |
//! |---|---|---|
//! | `Load` | a configuration source that fails to read | **nothing — see below** |
//! | `Validate` | a configuration source that rejects a value | **nothing — see below** |
//! | `Register` | a provider depending on a capability nobody offers | the work budget (counted, not timed) |
//! | `Boot` | a provider that fails, panics, or never returns | the provider deadline |
//! | `Ready` | liveness driven to `Dead` once `Ready` is reached | n/a — reaching `Ready` is the success condition |
//! | `Drain` | in-flight work that outlives the drain budget | the drain budget |
//! | `Stop` | a provider that fails or never returns while stopping | the provider deadline |
//!
//! # Two gaps this harness found, reported rather than papered over
//!
//! **1. `Load` and `Validate` are unbounded.** They call author-supplied code
//! *synchronously*, and `ApplicationBuilder::build` is not `async`, so there is no deadline around
//! either. A configuration source reading a hung network mount blocks the process indefinitely.
//!
//! `tests/deadlines.rs` enumerates "three kernel-owned waits" and bounds each. It missed these two
//! because it looked for `.await` — and a synchronous call that never returns is not an await. The
//! claim there is correct about what it checked and **incomplete about the set**; both are recorded
//! in `governance/phase-002-evidence.md`.
//!
//! **2. A panicking provider is not contained.** [`Behaviour::Panic`] at `Boot` or `Stop` unwinds
//! through the kernel and ends the process. Catching it needs one of two things Renvor does not
//! have: a `'static` future (ruled out by `InitContext` borrowing the state map) or
//! `futures::FutureExt::catch_unwind` (a new dependency in a phase whose inventory is a recorded
//! gate). Readiness contributors **are** contained, because they are synchronous.
//!
//! Neither gap is worked around here. [`Harness::run`] refuses a `Panic` point with a diagnostic
//! that says which requirement is unmet, which is the failing-loudly this project prefers to a
//! harness that appears to cover something it does not.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use renvor_core::config_port::{ConfigSource, SourceLayer};
use renvor_core::error::context::{Constraint, configuration};
use renvor_core::provider::ProviderFuture;
use renvor_core::{
    ApplicationBuilder, CapabilityId, InitContext, KernelError, LifecyclePhase, Liveness, Provider,
    ProviderId,
};

use crate::injection::{Behaviour, FailureInjectionPoint};

/// How far a harness run got, and how it ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The application reached `Ready`.
    Ready,
    /// The run failed during `Load`, `Validate`, or `Register`.
    BuildFailed(String),
    /// The run failed during `Boot`, having rolled back.
    BootFailed(String),
    /// The run reached `Ready` and then shut down; the drain was incomplete.
    DrainIncomplete(u32),
    /// The run reached `Ready` and shut down cleanly.
    Stopped,
    /// The injection point cannot be honoured, and why.
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
}

impl HarnessRun {
    /// Whether the run reached the given phase.
    #[must_use]
    pub fn reached(&self, phase: LifecyclePhase) -> bool {
        self.phases.contains(&phase)
    }
}

/// A configuration source that fails on demand, for `Load` and `Validate` injection.
#[derive(Debug)]
struct InjectingSource {
    phase: LifecyclePhase,
    fired: Arc<Mutex<bool>>,
}

impl InjectingSource {
    fn fire(&self) -> KernelError {
        *self.fired.lock().unwrap_or_else(PoisonError::into_inner) = true;
        configuration(
            "injected.key",
            SourceLayer::File("injected-source".into()).label(),
            "an injected failure",
            &Constraint::Rule("this failure was injected by the test harness"),
        )
    }
}

impl ConfigSource for InjectingSource {
    fn name(&self) -> &str {
        "injected-source"
    }

    fn load(&self) -> Result<(), KernelError> {
        if self.phase == LifecyclePhase::Load {
            return Err(self.fire());
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), KernelError> {
        if self.phase == LifecyclePhase::Validate {
            return Err(self.fire());
        }
        Ok(())
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
                // `Panic` is refused before a run starts; see the module documentation.
                Some(Behaviour::Panic) | None => Ok(()),
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
                Some(Behaviour::Panic) | None => Ok(()),
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

        if behaviour == Behaviour::Panic
            && matches!(phase, LifecyclePhase::Boot | LifecyclePhase::Stop)
        {
            // Refused rather than faked. See the module documentation: containing a panic across
            // an await needs a `'static` future or a new dependency, and pretending otherwise
            // would make SC-009 read as covered when it is not.
            return HarnessRun {
                phases: Vec::new(),
                fired: false,
                outcome: Outcome::NotInjectable(format!(
                    "a panicking provider is not contained at {phase}: the kernel would unwind \
                     through the process. See governance/phase-002-evidence.md"
                )),
            };
        }

        let mut builder = ApplicationBuilder::new()
            .with_entropy(Box::new(renvor_core::observe::FixedEntropy::new(vec![
                42;
                32
            ])))
            .with_provider_deadline(self.provider_deadline)
            .with_drain_budget(self.drain_budget);

        if matches!(phase, LifecyclePhase::Load | LifecyclePhase::Validate) {
            builder = builder.with_config_source(Box::new(InjectingSource {
                phase,
                fired: Arc::clone(&fired),
            }));
        }

        // `Register` fails by declaring a dependency nobody offers — the kernel's own refusal
        // rather than a provider misbehaving, which is what a Register failure actually is.
        let dependencies = if phase == LifecyclePhase::Register {
            *fired.lock().unwrap_or_else(PoisonError::into_inner) = true;
            vec![CapabilityId::new("nobody-offers-this")]
        } else {
            Vec::new()
        };

        builder = builder.with_provider(Box::new(InjectingProvider {
            id: ProviderId::new("injected"),
            provides: vec![CapabilityId::new("injected")],
            dependencies,
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
                    outcome: Outcome::BootFailed(failure.origin().to_string()),
                };
            }
        };

        if phase == LifecyclePhase::Ready {
            // A failure "at Ready" is not a lifecycle failure — reaching Ready is the success
            // condition (C-L2). It is a health failure, so that is what is injected.
            *fired.lock().unwrap_or_else(PoisonError::into_inner) = true;
            application.health().set_liveness(Liveness::Dead);
            return HarnessRun {
                phases: phases.entries(),
                fired: true,
                outcome: Outcome::Ready,
            };
        }

        // Drain injection: work that nobody will release.
        let held = (phase == LifecyclePhase::Drain).then(|| {
            *fired.lock().unwrap_or_else(PoisonError::into_inner) = true;
            application
                .work()
                .begin("injected work")
                .expect("the gate is open before shutdown")
        });

        let outcome = {
            let report = application.shutdown().await;
            match report.drain().outstanding() {
                0 => Outcome::Stopped,
                outstanding => Outcome::DrainIncomplete(outstanding),
            }
        };
        drop(held);

        HarnessRun {
            phases: phases.entries(),
            fired: read(&fired),
            outcome,
        }
    }
}
