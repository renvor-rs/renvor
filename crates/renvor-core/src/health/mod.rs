//! Liveness and readiness: two questions with two answers.
//!
//! # Neither is derived from the other
//!
//! FR-026 requires the two to be independent, and the reason is operational rather than
//! architectural. *"Is this process alive?"* decides whether an orchestrator **restarts** it.
//! *"Should it receive work?"* decides whether a load balancer **routes** to it. Deriving one from
//! the other collapses two decisions into one, and the collapse is always in the damaging
//! direction: an application that is draining is **alive and should not be restarted**, but is
//! **not ready and should receive no new work** (FR-027).
//!
//! So [`HealthState`] holds liveness as its own value, computes readiness from its contributors,
//! and has **no** code path where one reads the other. That is asserted by a test that drives them
//! into disagreement, not merely stated here.
//!
//! # Draining answers readiness without touching liveness
//!
//! [`HealthState::begin_draining`] forces readiness to not-ready and leaves liveness exactly where
//! it was. A drain that also reported not-alive would invite a restart during the shutdown it was
//! trying to complete cleanly.

pub mod contributor;

use core::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

pub use contributor::{ContributorFault, ContributorVerdict, ReadinessContributor};

/// Whether the process is alive.
///
/// A deliberately coarse answer: liveness decides restarts, and a restart is not the response to
/// most problems.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Liveness {
    /// The process is running and should not be restarted.
    #[default]
    Alive,
    /// The process is unrecoverable and should be restarted.
    Dead,
}

/// Whether the application should receive work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Readiness {
    /// Route work here.
    #[default]
    Ready,
    /// Do not route work here. Says nothing about whether the process should be restarted.
    NotReady,
}

impl fmt::Display for Liveness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Alive => "alive",
            Self::Dead => "dead",
        })
    }
}

impl fmt::Display for Readiness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ready => "ready",
            Self::NotReady => "not ready",
        })
    }
}

/// The full readiness answer: the verdict, and every contributor that produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadinessReport {
    /// The overall answer.
    pub readiness: Readiness,
    /// Whether the application is draining, which forces not-ready (FR-027).
    pub draining: bool,
    /// What each contributor said, individually identified (FR-028).
    pub contributors: Vec<ContributorVerdict>,
}

impl ReadinessReport {
    /// The contributors that reported not-ready — the answer to "why not?".
    #[must_use]
    pub fn blocking(&self) -> Vec<&ContributorVerdict> {
        self.contributors
            .iter()
            .filter(|verdict| verdict.readiness == Readiness::NotReady)
            .collect()
    }

    /// The contributors that panicked rather than answering.
    #[must_use]
    pub fn panicking(&self) -> Vec<&ContributorVerdict> {
        self.contributors
            .iter()
            .filter(|verdict| verdict.panicked())
            .collect()
    }
}

/// How long one readiness contributor may take before it is reported as timed out.
///
/// **This is Renvor's number, not a requirement's.** No artifact fixes it. Five seconds is chosen
/// to sit below the interval of a typical container readiness probe, so a hung contributor is
/// reported as hung rather than as a probe that never answered — but the value is Renvor's
/// judgement and is recorded as such in `governance/phase-002-evidence.md`.
pub const DEFAULT_READINESS_DEADLINE: Duration = Duration::from_secs(5);

/// Liveness and readiness for one application.
///
/// Cheap to clone: every clone shares one state, so a provider can hold it and a probe handler can
/// read it.
#[derive(Clone, Debug, Default)]
pub struct HealthState {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    liveness: Mutex<Liveness>,
    draining: Mutex<bool>,
    contributors: Mutex<Vec<Arc<dyn ReadinessContributor>>>,
    readiness_deadline: Mutex<Duration>,
    /// Readiness workers this state has outstanding. **Per state, not per process**: a global
    /// counter let one application's hung contributor refuse another's probes (W-005 N13).
    outstanding: contributor::Outstanding,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            liveness: Mutex::default(),
            draining: Mutex::default(),
            contributors: Mutex::default(),
            readiness_deadline: Mutex::new(DEFAULT_READINESS_DEADLINE),
            outstanding: contributor::Outstanding::default(),
        }
    }
}

impl HealthState {
    /// Creates a state that is alive, ready, and not draining.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the process is alive.
    ///
    /// Reads **only** the liveness value. It does not consult a contributor, the drain flag, or
    /// readiness — which is FR-026's independence, expressed as an absence of code.
    #[must_use]
    pub fn liveness(&self) -> Liveness {
        *self
            .inner
            .liveness
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Declares the process unrecoverable, or recovered.
    pub fn set_liveness(&self, liveness: Liveness) {
        *self
            .inner
            .liveness
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = liveness;
    }

    /// Registers a contributor to the readiness answer.
    pub fn register(&self, contributor: Arc<dyn ReadinessContributor>) {
        self.inner
            .contributors
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(contributor);
    }

    /// How long a contributor may take before it is reported as timed out.
    #[must_use]
    pub fn readiness_deadline(&self) -> Duration {
        *self
            .inner
            .readiness_deadline
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Replaces the readiness deadline.
    ///
    /// Settable rather than fixed at construction because the right value depends on the probe
    /// interval of whatever is asking, which the kernel does not know — and because a test that
    /// had to wait [`DEFAULT_READINESS_DEADLINE`] to prove a contributor hangs would be a test
    /// nobody runs.
    pub fn set_readiness_deadline(&self, deadline: Duration) {
        *self
            .inner
            .readiness_deadline
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = deadline;
    }

    /// Marks the application as draining: readiness becomes not-ready, liveness is untouched.
    pub fn begin_draining(&self) {
        *self
            .inner
            .draining
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = true;
    }

    /// Whether the application is draining.
    #[must_use]
    pub fn is_draining(&self) -> bool {
        *self
            .inner
            .draining
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Asks every contributor and assembles the answer.
    ///
    /// Every contributor is asked even after one reports not-ready. Short-circuiting would make
    /// the report name whichever check happened to be registered first, and FR-028 asks for all of
    /// them.
    #[must_use]
    pub fn readiness(&self) -> ReadinessReport {
        let contributors: Vec<Arc<dyn ReadinessContributor>> = self
            .inner
            .contributors
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();

        let verdicts: Vec<ContributorVerdict> = contributors
            .iter()
            .enumerate()
            .map(|(position, contributor)| {
                contributor::ask(
                    contributor,
                    position,
                    self.readiness_deadline(),
                    &self.inner.outstanding,
                )
            })
            .collect();

        let draining = self.is_draining();
        let readiness = if draining
            || verdicts
                .iter()
                .any(|verdict| verdict.readiness == Readiness::NotReady)
        {
            Readiness::NotReady
        } else {
            Readiness::Ready
        };

        ReadinessReport {
            readiness,
            draining,
            contributors: verdicts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HealthState, Liveness, Readiness, ReadinessContributor};
    use std::sync::Arc;

    struct Fixed {
        name: &'static str,
        readiness: Readiness,
    }

    impl ReadinessContributor for Fixed {
        fn name(&self) -> &str {
            self.name
        }
        fn readiness(&self) -> Readiness {
            self.readiness
        }
    }

    struct Panicking;

    impl ReadinessContributor for Panicking {
        fn name(&self) -> &str {
            "broken-check"
        }
        fn readiness(&self) -> Readiness {
            panic!("this contributor is deliberately broken");
        }
    }

    #[test]
    fn a_fresh_state_is_alive_and_ready() {
        let health = HealthState::new();
        assert_eq!(health.liveness(), Liveness::Alive);
        assert_eq!(health.readiness().readiness, Readiness::Ready);
    }

    #[test]
    fn liveness_and_readiness_can_disagree_in_both_directions() {
        // FR-026: neither derived from the other. Both directions, because deriving one from the
        // other would still allow one of them.
        let health = HealthState::new();

        // Alive but not ready — the ordinary case, and the one draining produces.
        health.register(Arc::new(Fixed {
            name: "db",
            readiness: Readiness::NotReady,
        }));
        assert_eq!(health.liveness(), Liveness::Alive);
        assert_eq!(health.readiness().readiness, Readiness::NotReady);

        // Dead but ready — nonsensical operationally, and reachable, which is the point: nothing
        // in the readiness path consults liveness.
        let other = HealthState::new();
        other.set_liveness(Liveness::Dead);
        assert_eq!(other.liveness(), Liveness::Dead);
        assert_eq!(other.readiness().readiness, Readiness::Ready);
    }

    #[test]
    fn draining_makes_readiness_negative_and_leaves_liveness_alone() {
        // FR-027. A drain that also reported not-alive would invite a restart during the shutdown
        // it is trying to finish cleanly.
        let health = HealthState::new();
        health.begin_draining();

        let report = health.readiness();
        assert!(report.draining);
        assert_eq!(report.readiness, Readiness::NotReady);
        assert_eq!(
            health.liveness(),
            Liveness::Alive,
            "draining must not affect liveness"
        );
    }

    #[test]
    fn every_contributor_is_identified_individually() {
        // FR-028. Asking all of them rather than stopping at the first negative is what makes the
        // report answer "why not?" rather than "which check is first in the list?".
        let health = HealthState::new();
        health.register(Arc::new(Fixed {
            name: "db",
            readiness: Readiness::Ready,
        }));
        health.register(Arc::new(Fixed {
            name: "cache",
            readiness: Readiness::NotReady,
        }));
        health.register(Arc::new(Fixed {
            name: "queue",
            readiness: Readiness::NotReady,
        }));

        let report = health.readiness();
        assert_eq!(report.contributors.len(), 3);
        assert_eq!(report.readiness, Readiness::NotReady);

        let blocking: Vec<&str> = report
            .blocking()
            .iter()
            .map(|verdict| verdict.name.as_str())
            .collect();
        assert_eq!(blocking, vec!["cache", "queue"], "both, not just the first");
    }

    #[test]
    fn a_panicking_contributor_is_caught_identified_and_treated_as_not_ready() {
        // The case that makes individual identification matter. The panic must not escape, the
        // contributor must be named, and the checks around it must still be asked.
        let health = HealthState::new();
        health.register(Arc::new(Fixed {
            name: "healthy-before",
            readiness: Readiness::Ready,
        }));
        health.register(Arc::new(Panicking));
        health.register(Arc::new(Fixed {
            name: "healthy-after",
            readiness: Readiness::Ready,
        }));

        let report = health.readiness();

        assert_eq!(report.readiness, Readiness::NotReady);
        assert_eq!(report.contributors.len(), 3, "the others were still asked");

        let panicking = report.panicking();
        assert_eq!(panicking.len(), 1);
        assert_eq!(panicking[0].name, "broken-check", "identified by name");
        assert_eq!(panicking[0].readiness, Readiness::NotReady);

        // A broken check must look different from a working negative answer.
        assert!(
            report
                .contributors
                .iter()
                .filter(|verdict| !verdict.panicked())
                .all(|verdict| verdict.readiness == Readiness::Ready),
            "a panic must not be blamed on the healthy contributors"
        );
    }

    #[test]
    fn a_healthy_contributor_set_reports_ready() {
        // POSITIVE CONTROL: readiness discriminates rather than always reporting not-ready.
        let health = HealthState::new();
        health.register(Arc::new(Fixed {
            name: "db",
            readiness: Readiness::Ready,
        }));
        let report = health.readiness();
        assert_eq!(report.readiness, Readiness::Ready);
        assert!(report.blocking().is_empty());
        assert!(report.panicking().is_empty());
    }
}
