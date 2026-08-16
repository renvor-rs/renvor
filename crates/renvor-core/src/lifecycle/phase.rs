//! The seven lifecycle phases and their ordering invariant.
//!
//! # Why the ordering is a type property, not a runtime check
//!
//! FR-001 requires that a backwards transition be **unrepresentable**, not merely rejected. Those
//! are different guarantees. A rejected transition is a runtime branch that has to be written
//! correctly at every call site and tested at every call site; an unrepresentable one cannot be
//! written down.
//!
//! The mechanism here is deliberately small: phases are a totally ordered enum, and the **only**
//! way to advance is [`LifecyclePhase::next`], which walks forward one step and returns [`None`]
//! at the end. There is no `set_phase`, no `From<u8>`, and no public constructor that takes a
//! position. A caller cannot express "go back to `Boot`" because no function accepts a phase as a
//! destination.
//!
//! [`PartialOrd`] is derived so a test can assert the observed sequence is monotonic (SC-001)
//! without instrumenting internals — the assertion a reader most wants is `observed.is_sorted()`,
//! and that requires the ordering to be public.

use core::fmt;

/// One phase of the application lifecycle, in the order the kernel runs them.
///
/// The declaration order **is** the contract order (C-L1):
///
/// ```text
/// Load → Validate → Register → Boot → Ready → Drain → Stop
/// ```
///
/// The derived [`Ord`] follows declaration order, so `Load < Validate < … < Stop` holds and a
/// monotonicity assertion is a one-liner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LifecyclePhase {
    /// Configuration sources are read. Nothing has started yet.
    Load,
    /// Configuration is decoded and checked. **0** providers booted, **0** listeners opened.
    Validate,
    /// Providers are registered and their dependency graph resolved. **0** providers booted.
    Register,
    /// Providers are initialised in dependency order.
    Boot,
    /// The success condition: every provider is initialised and the application is serving.
    Ready,
    /// Outstanding work is given a bounded opportunity to finish.
    Drain,
    /// Providers are stopped in reverse **actual initialisation** order.
    Stop,
}

impl LifecyclePhase {
    /// Every phase, in contract order.
    ///
    /// Kept as the single source of truth for "how many phases are there" — SC-009 requires
    /// failure injection at **7 of 7**, and a test that hardcodes `7` separately would not notice
    /// an eighth phase being added.
    pub const ALL: [Self; 7] = [
        Self::Load,
        Self::Validate,
        Self::Register,
        Self::Boot,
        Self::Ready,
        Self::Drain,
        Self::Stop,
    ];

    /// The first phase. There is no way to begin anywhere else.
    #[must_use]
    pub const fn first() -> Self {
        Self::Load
    }

    /// The phase that follows this one, or [`None`] at [`Self::Stop`].
    ///
    /// This is the **only** way to move between phases. Advancement is therefore always exactly
    /// one step forward, which is what makes a backwards transition unrepresentable rather than
    /// merely wrong.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self {
            Self::Load => Some(Self::Validate),
            Self::Validate => Some(Self::Register),
            Self::Register => Some(Self::Boot),
            Self::Boot => Some(Self::Ready),
            Self::Ready => Some(Self::Drain),
            Self::Drain => Some(Self::Stop),
            Self::Stop => None,
        }
    }

    /// The phase's position in the sequence, starting at zero.
    ///
    /// Exposed for diagnostics and for tests that assert monotonicity numerically. It is
    /// deliberately **not** paired with a `from_position` constructor: turning a number back into
    /// a phase is precisely the escape hatch that would let a caller jump backwards.
    #[must_use]
    pub const fn position(self) -> u8 {
        match self {
            Self::Load => 0,
            Self::Validate => 1,
            Self::Register => 2,
            Self::Boot => 3,
            Self::Ready => 4,
            Self::Drain => 5,
            Self::Stop => 6,
        }
    }

    /// The stable, lowercase name used in diagnostics and span names.
    ///
    /// Returned as `&'static str` rather than built with `format!` so it can appear in a
    /// `#[error(...)]` message and a tracing field without allocating.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Load => "load",
            Self::Validate => "validate",
            Self::Register => "register",
            Self::Boot => "boot",
            Self::Ready => "ready",
            Self::Drain => "drain",
            Self::Stop => "stop",
        }
    }
}

impl fmt::Display for LifecyclePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::LifecyclePhase as P;

    #[test]
    fn the_declared_order_is_the_contract_order() {
        // C-L1: Load -> Validate -> Register -> Boot -> Ready -> Drain -> Stop.
        assert_eq!(
            P::ALL,
            [
                P::Load,
                P::Validate,
                P::Register,
                P::Boot,
                P::Ready,
                P::Drain,
                P::Stop
            ]
        );
        assert_eq!(P::ALL.len(), 7, "SC-009 injects failure at 7 of 7 phases");
    }

    #[test]
    fn walking_next_from_first_visits_every_phase_once_in_order() {
        let mut walked = Vec::new();
        let mut phase = Some(P::first());
        while let Some(current) = phase {
            walked.push(current);
            phase = current.next();
        }
        assert_eq!(walked, P::ALL.to_vec());
        assert_eq!(P::Stop.next(), None, "Stop is terminal");
    }

    #[test]
    fn the_ordering_is_strictly_increasing_so_a_test_can_assert_monotonicity() {
        // SC-001 asserts 0 runs observe a different order. That assertion is only writable
        // without instrumenting internals if the ordering is public and total.
        for pair in P::ALL.windows(2) {
            assert!(
                pair[0] < pair[1],
                "{:?} must precede {:?}",
                pair[0],
                pair[1]
            );
            assert!(pair[0].position() < pair[1].position());
        }

        // POSITIVE CONTROL: `Ord` discriminates in BOTH directions. Without this, an impl that
        // returned `Less` unconditionally would satisfy the loop above. Written against `cmp`
        // rather than `!(a > b)` so the assertion names the ordering it is checking instead of
        // relying on the negation of a different operator.
        use core::cmp::Ordering;
        assert_eq!(P::Load.cmp(&P::Stop), Ordering::Less);
        assert_eq!(P::Stop.cmp(&P::Load), Ordering::Greater);
        assert_eq!(P::Boot.cmp(&P::Boot), Ordering::Equal);
    }

    #[test]
    fn phase_names_are_distinct() {
        // Span names and diagnostics key off these. Two phases sharing a name would make an
        // emitted-span assertion ambiguous rather than wrong, which is harder to notice.
        let mut names: Vec<&str> = P::ALL.iter().map(|p| p.as_str()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "two phases share a name");
    }
}
