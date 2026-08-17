//! Failure injection: what to break, where, and how.
//!
//! # Three behaviours, because they fail differently
//!
//! C-L9 requires `Fail`, `Panic`, and `Hang` at each phase, and the three are not variations on a
//! theme — each exercises a different guarantee:
//!
//! | Behaviour | Exercises |
//! |---|---|
//! | [`Behaviour::Fail`] | the error path: rollback order, diagnostics, "0 providers reach Ready" |
//! | [`Behaviour::Panic`] | that a misbehaving provider is a **failure**, not a process abort |
//! | [`Behaviour::Hang`] | deadline enforcement — **without real elapsed time** (FR-031) |
//!
//! `Hang` is the one that would be untestable without deterministic time. A provider that never
//! returns, under a paused runtime, costs 0 real seconds and still proves the kernel's deadline
//! fires.

use core::fmt;

use renvor_core::LifecyclePhase;

/// How an injected failure behaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Behaviour {
    /// Return an error. The ordinary failure path.
    Fail,
    /// Panic. Proves a misbehaving provider is a failure rather than a process abort.
    Panic,
    /// Never return. Proves the kernel's deadline fires, at 0 real elapsed time.
    Hang,
}

impl Behaviour {
    /// Every behaviour, for coverage tests that must not hard-code the list.
    pub const ALL: [Self; 3] = [Self::Fail, Self::Panic, Self::Hang];
}

impl fmt::Display for Behaviour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Fail => "fail",
            Self::Panic => "panic",
            Self::Hang => "hang",
        })
    }
}

/// Where to inject a failure, and what it should do.
///
/// Carries a [`LifecyclePhase`] rather than a string, so a point naming a phase that does not
/// exist is unrepresentable rather than a test that silently never fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FailureInjectionPoint {
    phase: LifecyclePhase,
    behaviour: Behaviour,
}

impl FailureInjectionPoint {
    /// Injects `behaviour` at `phase`.
    #[must_use]
    pub const fn new(phase: LifecyclePhase, behaviour: Behaviour) -> Self {
        Self { phase, behaviour }
    }

    /// The phase this point fires in.
    #[must_use]
    pub const fn phase(self) -> LifecyclePhase {
        self.phase
    }

    /// What it does when it fires.
    #[must_use]
    pub const fn behaviour(self) -> Behaviour {
        self.behaviour
    }

    /// Every phase-and-behaviour combination.
    ///
    /// Built from [`LifecyclePhase::ALL`] and [`Behaviour::ALL`] rather than written out, so a
    /// new phase or behaviour is covered the moment it is added — SC-009 asks for **7 of 7**
    /// phases, and a hand-written list is how that quietly becomes 6 of 7.
    #[must_use]
    pub fn every_combination() -> Vec<Self> {
        LifecyclePhase::ALL
            .into_iter()
            .flat_map(|phase| {
                Behaviour::ALL
                    .into_iter()
                    .map(move |behaviour| Self::new(phase, behaviour))
            })
            .collect()
    }
}

impl fmt::Display for FailureInjectionPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}", self.behaviour, self.phase)
    }
}

#[cfg(test)]
mod tests {
    use super::{Behaviour, FailureInjectionPoint};
    use renvor_core::LifecyclePhase;

    #[test]
    fn every_combination_covers_every_phase_and_every_behaviour() {
        let points = FailureInjectionPoint::every_combination();
        assert_eq!(points.len(), 7 * 3, "7 phases times 3 behaviours");

        for phase in LifecyclePhase::ALL {
            for behaviour in Behaviour::ALL {
                assert!(
                    points.contains(&FailureInjectionPoint::new(phase, behaviour)),
                    "{behaviour} at {phase} is not covered"
                );
            }
        }
    }

    #[test]
    fn a_point_reads_back_what_it_was_given() {
        let point = FailureInjectionPoint::new(LifecyclePhase::Boot, Behaviour::Hang);
        assert_eq!(point.phase(), LifecyclePhase::Boot);
        assert_eq!(point.behaviour(), Behaviour::Hang);
        assert_eq!(point.to_string(), "hang at boot");
    }
}
