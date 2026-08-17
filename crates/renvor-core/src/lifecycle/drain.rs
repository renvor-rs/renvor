//! Draining in-flight work under a bounded budget, and refusing new work once shutdown begins.
//!
//! # Zero is a budget, not a special case
//!
//! C-L5 and FR-042 fix three things about a zero budget: it means *skip the drain and stop
//! immediately*, it is **not** invalid, and it is **not** "wait forever". The fourth is the one
//! that is easy to get wrong — a zero budget with work in flight must report that work as
//! outstanding **exactly as a timed-out drain would**.
//!
//! So [`WorkGate::drain`] has no `if budget.is_zero()` branch. Zero flows through the same
//! [`tokio::time::timeout`] the thirty-second default flows through, and reaches the same
//! [`DrainOutcome::Incomplete`] by the same route. A separate fast path would be the obvious
//! implementation and would be one edit away from returning `Clean` for the case the requirement
//! exists to prevent.
//!
//! # Why a permit rather than a counter
//!
//! Work is represented by [`WorkPermit`], which decrements on drop. An author who takes a permit
//! and returns early — including through `?` on an unrelated error — still releases it. A raw
//! increment/decrement pair would require every path to remember the decrement, and the path that
//! forgets is the error path, which is the one that runs when the drain matters most.
//!
//! # The race that a plain atomic would lose
//!
//! Between "is the gate closed?" and "increment the counter", a concurrent shutdown can slip in,
//! and a permit gets granted after shutdown began. Both operations therefore happen inside one
//! [`tokio::sync::watch::Sender::send_if_modified`] closure, which runs under the channel's own
//! lock — the check and the increment are one indivisible step, and the same closure form makes
//! [`WorkGate::close`] able to report whether *it* was the call that closed the gate.

use core::fmt;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;

use crate::error::KernelError;

/// The documented default drain budget (FR-006, FR-042, C-L5).
///
/// A named constant rather than a literal at its single use site, because the requirement calls
/// this value "documented" and a number that exists only inside a function body is not.
pub const DEFAULT_DRAIN_BUDGET: Duration = Duration::from_secs(30);

/// How a drain ended.
///
/// Two variants, and deliberately no third for "finished but we are not sure". FR-007 prohibits
/// reporting an incomplete drain as clean, and the surest way to keep that prohibition is to give
/// the ambiguous answer nowhere to live.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DrainOutcome {
    /// Every unit of in-flight work finished inside the budget.
    ///
    /// The default, so an application that never had work in flight starts from the truthful
    /// answer rather than from an `Incomplete` nobody measured.
    #[default]
    Clean,
    /// The budget elapsed with work still in flight.
    Incomplete {
        /// How many units of work were still outstanding when the budget ran out.
        outstanding: u32,
    },
}

impl DrainOutcome {
    /// Whether the drain completed.
    #[must_use]
    pub const fn is_clean(self) -> bool {
        matches!(self, Self::Clean)
    }

    /// How much work was still outstanding. Zero when clean.
    #[must_use]
    pub const fn outstanding(self) -> u32 {
        match self {
            Self::Clean => 0,
            Self::Incomplete { outstanding } => outstanding,
        }
    }
}

impl fmt::Display for DrainOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clean => f.write_str("drained cleanly"),
            Self::Incomplete { outstanding } => write!(
                f,
                "drain ended with {outstanding} unit(s) of work still outstanding"
            ),
        }
    }
}

/// The gate's whole state, in one value so it can be read and written indivisibly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GateState {
    outstanding: u32,
    closed: bool,
}

/// Admits in-flight work while the application is running, and refuses it once shutdown begins.
///
/// Cheap to clone: every clone shares one gate, which is what lets a provider hold on to it and
/// still be refused by a shutdown started elsewhere.
#[derive(Clone, Debug)]
pub struct WorkGate {
    state: Arc<watch::Sender<GateState>>,
}

impl Default for WorkGate {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkGate {
    /// Creates an open gate with no work in flight.
    #[must_use]
    pub fn new() -> Self {
        let (sender, _) = watch::channel(GateState {
            outstanding: 0,
            closed: false,
        });
        Self {
            state: Arc::new(sender),
        }
    }

    /// Admits one unit of work.
    ///
    /// # Errors
    ///
    /// Returns [`KernelError::ShuttingDown`] naming `operation` once shutdown has begun. FR-006
    /// requires the rejection to be an error: work is neither silently dropped nor silently
    /// accepted, because both leave the caller believing something that is not true.
    pub fn begin(&self, operation: impl Into<String>) -> Result<WorkPermit, KernelError> {
        let mut granted = false;
        self.state.send_if_modified(|state| {
            if state.closed {
                return false;
            }
            state.outstanding = state.outstanding.saturating_add(1);
            granted = true;
            true
        });

        if granted {
            Ok(WorkPermit {
                state: Arc::clone(&self.state),
            })
        } else {
            Err(KernelError::ShuttingDown {
                operation: operation.into(),
            })
        }
    }

    /// Refuses all further work.
    ///
    /// Returns `true` if this call is the one that closed the gate, and `false` if it was already
    /// closed. That distinction is what makes shutdown idempotent without a separate flag for
    /// somebody to forget to check (FR-008).
    pub fn close(&self) -> bool {
        let mut closed_by_this_call = false;
        self.state.send_if_modified(|state| {
            if state.closed {
                return false;
            }
            state.closed = true;
            closed_by_this_call = true;
            true
        });
        closed_by_this_call
    }

    /// How many units of work are in flight.
    #[must_use]
    pub fn outstanding(&self) -> u32 {
        self.state.borrow().outstanding
    }

    /// Whether the gate has been closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.state.borrow().closed
    }

    /// Closes the gate and waits, up to `budget`, for in-flight work to finish.
    ///
    /// Zero takes the same route as every other budget — see the module documentation for why
    /// there is no fast path.
    pub async fn drain(&self, budget: Duration) -> DrainOutcome {
        self.close();

        // Not a zero-budget shortcut: this is "there is nothing to wait for", which is true at any
        // budget and keeps the outcome from depending on how `timeout` orders its first poll.
        if self.outstanding() == 0 {
            return DrainOutcome::Clean;
        }

        let mut receiver = self.state.subscribe();
        match tokio::time::timeout(budget, receiver.wait_for(|state| state.outstanding == 0)).await
        {
            Ok(Ok(_)) => DrainOutcome::Clean,
            _ => DrainOutcome::Incomplete {
                outstanding: self.outstanding(),
            },
        }
    }
}

/// One unit of in-flight work. Releases the work on drop.
///
/// Deliberately carries no method to release early: `drop` is the only release, so there is one
/// path to get right rather than two that can disagree.
#[derive(Debug)]
pub struct WorkPermit {
    state: Arc<watch::Sender<GateState>>,
}

impl Drop for WorkPermit {
    fn drop(&mut self) {
        self.state.send_if_modified(|state| {
            let before = state.outstanding;
            state.outstanding = state.outstanding.saturating_sub(1);
            before != state.outstanding
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_DRAIN_BUDGET, DrainOutcome, WorkGate};
    use crate::error::ErrorCategory;
    use std::time::Duration;

    #[test]
    fn the_default_budget_is_the_documented_thirty_seconds() {
        assert_eq!(DEFAULT_DRAIN_BUDGET, Duration::from_secs(30));
    }

    #[test]
    fn a_permit_releases_on_drop_including_on_an_early_return() {
        let gate = WorkGate::new();

        // The early-return shape: the permit is taken, something else fails, and nothing in the
        // failing path mentions the permit.
        fn handle_a_request(gate: &WorkGate) -> Result<(), &'static str> {
            let _permit = gate.begin("request").expect("the gate is open");
            assert_eq!(gate.outstanding(), 1);
            Err("an unrelated failure")
        }

        assert!(handle_a_request(&gate).is_err());
        assert_eq!(gate.outstanding(), 0, "the permit released anyway");
    }

    #[test]
    fn work_is_refused_once_the_gate_closes() {
        // FR-006: an error, not a silent drop and not a silent acceptance.
        let gate = WorkGate::new();
        assert!(gate.close());

        let error = gate
            .begin("enqueue")
            .expect_err("new work must be refused after shutdown begins");
        assert_eq!(error.category(), ErrorCategory::ShuttingDown);
        assert!(error.to_string().contains("enqueue"), "{error}");

        // POSITIVE CONTROL: the same call succeeds on an open gate, so the refusal is about the
        // gate's state rather than about `begin` always failing.
        assert!(WorkGate::new().begin("enqueue").is_ok());
    }

    #[test]
    fn closing_twice_reports_which_call_did_it() {
        // The mechanism behind idempotent shutdown (FR-008).
        let gate = WorkGate::new();
        assert!(gate.close(), "the first call closes the gate");
        assert!(!gate.close(), "the second observes it was already closed");
        assert!(gate.is_closed());
    }

    #[tokio::test]
    async fn an_idle_gate_drains_cleanly() {
        let gate = WorkGate::new();
        assert_eq!(gate.drain(DEFAULT_DRAIN_BUDGET).await, DrainOutcome::Clean);
    }

    #[tokio::test(start_paused = true)]
    async fn an_over_budget_drain_is_reported_as_incomplete() {
        // SC-006, with 0 real elapsed time: the runtime's clock is paused and auto-advances only
        // when nothing is runnable, so the "one second" below costs nothing.
        let gate = WorkGate::new();
        let _permit = gate.begin("long request").expect("the gate is open");

        let outcome = gate.drain(Duration::from_secs(1)).await;
        assert_eq!(outcome, DrainOutcome::Incomplete { outstanding: 1 });
        assert!(!outcome.is_clean(), "0 over-budget drains report clean");
    }

    #[tokio::test(start_paused = true)]
    async fn a_zero_budget_with_work_in_flight_reports_it_as_outstanding() {
        // FR-042's fourth clause: choosing an immediate stop must never silently read as a clean
        // one. Same variant, same code path as the timeout above.
        let gate = WorkGate::new();
        let _first = gate.begin("a").expect("open");
        let _second = gate.begin("b").expect("open");

        let outcome = gate.drain(Duration::ZERO).await;
        assert_eq!(outcome, DrainOutcome::Incomplete { outstanding: 2 });
    }

    #[tokio::test(start_paused = true)]
    async fn a_zero_budget_with_nothing_in_flight_is_clean() {
        // POSITIVE CONTROL for the test above: zero is a budget, not a verdict. Without this, an
        // implementation that always reported `Incomplete` for a zero budget would pass.
        let gate = WorkGate::new();
        assert_eq!(gate.drain(Duration::ZERO).await, DrainOutcome::Clean);
    }

    #[tokio::test(start_paused = true)]
    async fn work_that_finishes_inside_the_budget_drains_cleanly() {
        // POSITIVE CONTROL for the over-budget test: the drain genuinely waits and genuinely
        // notices completion, rather than reporting whatever the count was when it started.
        let gate = WorkGate::new();
        let permit = gate.begin("short request").expect("open");

        let releaser = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            drop(permit);
        });

        assert_eq!(
            gate.drain(Duration::from_secs(30)).await,
            DrainOutcome::Clean
        );
        releaser.await.expect("the releaser finishes");
    }
}
