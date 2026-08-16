//! The assembled application and its phase machine.
//!
//! # Going backwards is unrepresentable, not merely rejected
//!
//! V-A1 and FR-001 require that no ordering exists in which a later phase runs before an earlier
//! one. The usual way to satisfy that is a setter plus a guard, which is a rule: it holds until
//! someone writes the one call site that skips the guard.
//!
//! [`PhaseCursor`] has **no setter and no target argument**. Its only mutating method is
//! [`PhaseCursor::advance`], which consults [`LifecyclePhase::next`]. A caller cannot name a phase
//! to move to, so it cannot name an earlier one — or skip one. The guarantee is a missing
//! parameter rather than a check.
//!
//! # The observed sequence is public API, not instrumentation
//!
//! FR-002 requires the phase sequence to be inspectable by a test **without instrumenting the
//! kernel's internals**. [`PhaseLog`] is that inspection point: an author or a test takes a handle
//! from the builder *before* starting, and reads it afterwards — including after a failure, when
//! there is no [`Application`] left to ask. A test-only channel would satisfy the letter of FR-002
//! and leave real operators without the same view.
//!
//! # Boot consumes the application
//!
//! [`Application::boot`] takes `self` by value and returns a new one. A failed boot therefore
//! yields **no** `Application` at all, so a half-started application cannot be observed, stored,
//! or used — the rollback is not merely performed, the wreckage is unreachable.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::cancel::CancelScope;
use crate::error::KernelError;
use crate::lifecycle::LifecyclePhase;
use crate::lifecycle::rollback::{BootFailure, RollbackReport, roll_back};
use crate::observe::RunIdentifier;
use crate::provider::{
    InitContext, InitialisationOrder, ProviderId, ProviderRegistry, ResolutionReport,
};
use crate::state::TypedStateMap;

/// The documented default drain budget (C-L5).
///
/// Stated as a constant rather than a literal at the one place it is used, because C-L5 calls it
/// "documented" and a documented value that only exists inside a function body is not.
pub const DEFAULT_DRAIN_BUDGET: Duration = Duration::from_secs(30);

/// A shared, readable record of the phases a run entered.
///
/// Cloneable and cheap: the builder hands one out before starting, and the same underlying record
/// is written by the builder, the boot loop, and rollback. Reading it never blocks on the
/// application still being alive.
#[derive(Clone, Debug, Default)]
pub struct PhaseLog {
    entries: Arc<Mutex<Vec<LifecyclePhase>>>,
}

impl PhaseLog {
    /// Creates an empty log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The phases entered so far, in order.
    ///
    /// A poisoned lock is recovered from rather than propagated. The data behind it is a `Vec` of
    /// `Copy` enums, so a panic elsewhere cannot have left it in a half-written state, and SC-004
    /// requires **0** panics in ordinary use — including in the path a test uses to find out what
    /// went wrong.
    #[must_use]
    pub fn entries(&self) -> Vec<LifecyclePhase> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Appends a phase.
    fn record(&self, phase: LifecyclePhase) {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(phase);
    }
}

/// Walks the phases forward, one step at a time, recording each.
///
/// See the module documentation: this type has no way to express a backwards or skipping
/// transition, which is how FR-001 is enforced.
#[derive(Debug)]
pub struct PhaseCursor {
    current: LifecyclePhase,
    log: PhaseLog,
}

impl PhaseCursor {
    /// Starts at the first phase and records it.
    #[must_use]
    pub fn start(log: PhaseLog) -> Self {
        let current = LifecyclePhase::first();
        log.record(current);
        Self { current, log }
    }

    /// The phase the run is in.
    #[must_use]
    pub const fn current(&self) -> LifecyclePhase {
        self.current
    }

    /// Moves to the next phase and records it.
    ///
    /// At the final phase this is a no-op and returns it unchanged — running off the end is not a
    /// failure, it is simply the end.
    pub fn advance(&mut self) -> LifecyclePhase {
        if let Some(next) = self.current.next() {
            self.current = next;
            self.log.record(next);
        }
        self.current
    }

    /// The log this cursor writes to.
    #[must_use]
    pub fn log(&self) -> PhaseLog {
        self.log.clone()
    }
}

/// A provider that finished initialising, together with the scope its work runs under.
///
/// Position, name, and scope live in **one** record rather than three parallel vectors, because
/// three collections indexed in lockstep are three chances to fall out of step — and the thing
/// they encode is the reverse order C-L3 says a test must be able to trust.
#[derive(Debug)]
pub struct InitialisedProvider {
    position: u32,
    id: ProviderId,
    scope: CancelScope,
}

impl InitialisedProvider {
    /// This provider's registration position.
    #[must_use]
    pub const fn position(&self) -> u32 {
        self.position
    }

    /// This provider's name.
    #[must_use]
    pub const fn id(&self) -> &ProviderId {
        &self.id
    }

    /// The cancellation scope this provider's work runs under.
    #[must_use]
    pub const fn scope(&self) -> &CancelScope {
        &self.scope
    }
}

/// An assembled application.
///
/// Produced by [`crate::lifecycle::ApplicationBuilder::build`] in the `Register` phase, and moved
/// to `Ready` by [`Application::boot`].
#[derive(Debug)]
pub struct Application {
    cursor: PhaseCursor,
    state: TypedStateMap,
    registry: ProviderRegistry,
    order: InitialisationOrder,
    report: ResolutionReport,
    initialised: Vec<InitialisedProvider>,
    cancel: CancelScope,
    run_id: RunIdentifier,
    drain_budget: Duration,
}

impl Application {
    /// Assembles an application that has completed `Register`.
    pub(crate) fn new(
        cursor: PhaseCursor,
        registry: ProviderRegistry,
        order: InitialisationOrder,
        report: ResolutionReport,
        run_id: RunIdentifier,
        drain_budget: Duration,
    ) -> Self {
        Self {
            cursor,
            state: TypedStateMap::new(),
            registry,
            order,
            report,
            initialised: Vec::new(),
            cancel: CancelScope::root(),
            run_id,
            drain_budget,
        }
    }

    /// The phase this application is in.
    #[must_use]
    pub const fn phase(&self) -> LifecyclePhase {
        self.cursor.current()
    }

    /// The phases this run entered, in order (FR-002).
    #[must_use]
    pub fn phases(&self) -> Vec<LifecyclePhase> {
        self.cursor.log().entries()
    }

    /// The identifier fixed for this run (FR-043).
    #[must_use]
    pub const fn run_id(&self) -> &RunIdentifier {
        &self.run_id
    }

    /// The registered providers.
    #[must_use]
    pub const fn registry(&self) -> &ProviderRegistry {
        &self.registry
    }

    /// The resolved initialisation order.
    ///
    /// This is what the resolver *decided*. What actually happened is [`Self::initialised`], and
    /// C-L3 exists because the two can differ when boot stops early.
    #[must_use]
    pub const fn initialisation_order(&self) -> &InitialisationOrder {
        &self.order
    }

    /// What resolution cost, measured (SC-021).
    #[must_use]
    pub const fn resolution_report(&self) -> &ResolutionReport {
        &self.report
    }

    /// The providers that actually initialised, in the order they did.
    #[must_use]
    pub fn initialised(&self) -> &[InitialisedProvider] {
        &self.initialised
    }

    /// The application's typed state.
    #[must_use]
    pub const fn state(&self) -> &TypedStateMap {
        &self.state
    }

    /// The root cancellation scope.
    #[must_use]
    pub const fn cancel(&self) -> &CancelScope {
        &self.cancel
    }

    /// The drain budget this application will honour (C-L5).
    #[must_use]
    pub const fn drain_budget(&self) -> Duration {
        self.drain_budget
    }

    /// Initialises every provider in dependency order, reaching `Ready`.
    ///
    /// Each provider is initialised inside a [`crate::cancel::ProviderScope`], which cancels
    /// itself if this function returns before the provider succeeds — so an early return cannot
    /// leave a provider half-initialised with its work still running (FR-024, C-L8).
    ///
    /// # Errors
    ///
    /// Returns [`BootFailure`] if any provider fails or cancellation arrives. Before returning,
    /// **every already-initialised provider is stopped in reverse actual initialisation order**
    /// (FR-004, C-L3), and every rollback failure is reported alongside the original (FR-005,
    /// C-L4). `Ready` is not reached and no `Application` is returned.
    pub async fn boot(mut self) -> Result<Self, BootFailure> {
        self.cursor.advance();

        let entries: Vec<(u32, ProviderId)> = self.order.entries().to_vec();
        for (position, id) in entries {
            if let Some(origin) = self.cancellation_check() {
                return Err(self.unwind(origin).await);
            }

            // The guard cancels on drop unless `complete()` is reached, so every path out of this
            // loop body other than success cancels this provider's scope.
            let guard = self.cancel.provider(id.as_str());

            let Some(provider) = self.registry.get(position) else {
                // Unreachable: the position came from the resolver, over this same registry.
                // Reported rather than asserted, because SC-004 permits 0 panics and a crash here
                // would replace a diagnostic with a stack trace.
                let origin = KernelError::DependencyMissing {
                    dependent: id.to_string(),
                    capability: format!("<registration position {position} is not registered>"),
                };
                return Err(self.unwind(origin).await);
            };

            let outcome = {
                let mut context = InitContext::new(&id, &mut self.state, guard.scope());
                provider.initialise(&mut context).await
            };

            match outcome {
                Ok(()) => {
                    let scope = guard.complete();
                    self.initialised.push(InitialisedProvider {
                        position,
                        id,
                        scope,
                    });
                }
                Err(source) => {
                    let origin = KernelError::ProviderInit {
                        provider: id.to_string(),
                        source,
                    };
                    drop(guard);
                    return Err(self.unwind(origin).await);
                }
            }
        }

        // Cancellation between the last provider and Ready still counts (C-L8).
        if let Some(origin) = self.cancellation_check() {
            return Err(self.unwind(origin).await);
        }

        self.cursor.advance();
        Ok(self)
    }

    /// Whether cancellation has been signalled, as the error it becomes.
    fn cancellation_check(&self) -> Option<KernelError> {
        self.cancel.is_cancelled().then(|| KernelError::Cancelled {
            phase: self.cursor.current(),
        })
    }

    /// Rolls back everything initialised and packages the failure.
    async fn unwind(mut self, origin: KernelError) -> BootFailure {
        let report: RollbackReport = roll_back(&self.registry, &mut self.initialised).await;
        BootFailure::new(origin, report, self.cursor.log().entries())
    }
}

#[cfg(test)]
mod tests {
    use super::{PhaseCursor, PhaseLog};
    use crate::lifecycle::LifecyclePhase;

    #[test]
    fn a_cursor_records_the_phase_it_starts_in() {
        let log = PhaseLog::new();
        let cursor = PhaseCursor::start(log.clone());
        assert_eq!(cursor.current(), LifecyclePhase::Load);
        assert_eq!(log.entries(), vec![LifecyclePhase::Load]);
    }

    #[test]
    fn a_cursor_can_only_move_forward_one_phase_at_a_time() {
        // FR-001 / V-A1. There is no argument to `advance`, so there is no way to ask for an
        // earlier phase or to skip one; this test records the resulting sequence.
        let log = PhaseLog::new();
        let mut cursor = PhaseCursor::start(log.clone());
        for _ in 0..LifecyclePhase::ALL.len() {
            cursor.advance();
        }
        assert_eq!(log.entries(), LifecyclePhase::ALL.to_vec());
    }

    #[test]
    fn advancing_past_the_final_phase_stays_there_and_records_nothing_more() {
        let log = PhaseLog::new();
        let mut cursor = PhaseCursor::start(log.clone());
        while cursor.current() != LifecyclePhase::Stop {
            cursor.advance();
        }
        let settled = log.entries();

        assert_eq!(cursor.advance(), LifecyclePhase::Stop);
        assert_eq!(cursor.advance(), LifecyclePhase::Stop);
        assert_eq!(
            log.entries(),
            settled,
            "running off the end must not duplicate the final phase"
        );
    }

    #[test]
    fn a_log_handle_taken_early_sees_phases_recorded_later() {
        // This is the mechanism behind FR-002: a test holds the handle from before the run and
        // reads it afterwards, including when the run failed and there is nothing left to ask.
        let log = PhaseLog::new();
        let observer = log.clone();
        assert!(observer.entries().is_empty());

        let mut cursor = PhaseCursor::start(log);
        cursor.advance();
        assert_eq!(
            observer.entries(),
            vec![LifecyclePhase::Load, LifecyclePhase::Validate]
        );
    }
}
