//! The clock port: an instant a capability can be handed, and a test can move.
//!
//! # Why the kernel has a clock port at all
//!
//! Phase 009 gave the authentication domain its own `Clock` so that expiry could be evaluated
//! against an injected instant — *"a test moves time instead of waiting for it, and production
//! reads the real clock through the same trait"*. Phase 010's job worker needs the same thing for
//! `run_at` and lease expiry, and `renvor-jobs` cannot depend on the authentication domain to get
//! it. So the port moves down to the kernel, where every capability can reach it.
//!
//! # Why `SystemTime`, not a date-time library
//!
//! This crate carries no `chrono` and no `time`, and adding one for a single method would put a
//! date-time library into every consumer's graph. [`std::time::SystemTime`] is enough to say
//! *when*; a store that needs a calendar type converts at its own boundary, where it already
//! binds one. The authentication domain's chrono-based port is unchanged this phase and is
//! recorded as a limitation to unify later, not silently duplicated.
//!
//! # Wall time, deliberately
//!
//! [`std::time::Instant`] would be the monotonic choice, and it is the wrong one here: a scheduled
//! job's `run_at` is written to a database and read back by another process, so it has to be an
//! instant two processes can agree on. That is wall time. Relative bounds — timeouts, TTLs in a
//! memory substitute — use `tokio::time`, which is monotonic and pausable, and do not come through
//! this port.

use core::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

/// Something that can say what time it is.
///
/// `Send + Sync` because a worker asks from whichever task claims a job.
pub trait Clock: fmt::Debug + Send + Sync {
    /// The current instant, as this clock sees it.
    fn now(&self) -> SystemTime;
}

/// The production clock: the operating system's wall clock, and nothing else.
///
/// No fields and a zero-argument constructor, for the reason `OsEntropy` has the same shape —
/// there is nowhere for a test to reach in and make production time fake.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl SystemClock {
    /// Creates the production clock. Takes **0** inputs.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// A clock a test sets and moves.
///
/// Available outside `cfg(test)` on purpose, like `FixedEntropy`: an author testing their own
/// scheduled job needs to move the clock rather than wait for it.
#[derive(Debug)]
pub struct FixedClock {
    now: Mutex<SystemTime>,
}

impl FixedClock {
    /// Creates a clock stopped at `at`.
    #[must_use]
    pub const fn new(at: SystemTime) -> Self {
        Self {
            now: Mutex::new(at),
        }
    }

    /// Creates a clock stopped at the Unix epoch plus `seconds`.
    ///
    /// The usual way in for a test that wants a readable instant rather than a real one.
    #[must_use]
    pub fn at_unix_seconds(seconds: u64) -> Self {
        Self::new(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
    }

    /// Moves the clock to `at`. Backwards is permitted, so a test can provoke a regression.
    pub fn set(&self, at: SystemTime) {
        *self.now.lock().unwrap_or_else(PoisonError::into_inner) = at;
    }

    /// Moves the clock forward by `by`.
    pub fn advance(&self, by: Duration) {
        let mut now = self.now.lock().unwrap_or_else(PoisonError::into_inner);
        *now += by;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> SystemTime {
        *self.now.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A shared clock is itself a clock, so one instance can be held by a worker and a test.
impl<T> Clock for Arc<T>
where
    T: Clock + ?Sized,
{
    fn now(&self) -> SystemTime {
        (**self).now()
    }
}

#[cfg(test)]
mod tests {
    use super::{Clock, FixedClock, SystemClock};
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    #[test]
    fn the_production_clock_reads_wall_time() {
        let before = SystemTime::now();
        let read = SystemClock::new().now();
        let after = SystemTime::now();
        assert!(
            read >= before && read <= after,
            "the clock read is not wall time"
        );
    }

    #[test]
    fn a_fixed_clock_does_not_move_on_its_own() {
        let clock = FixedClock::at_unix_seconds(1_000);
        let first = clock.now();
        let second = clock.now();
        assert_eq!(first, second, "a fixed clock advanced without being asked");
        assert_eq!(
            first,
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_000),
            "the fixed clock is not at the instant it was given"
        );
    }

    #[test]
    fn advance_and_set_move_the_fixed_clock_in_both_directions() {
        let clock = FixedClock::at_unix_seconds(1_000);
        clock.advance(Duration::from_secs(60));
        assert_eq!(
            clock.now(),
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_060)
        );
        // Backwards is allowed: a test that wants to prove a store refuses a regressed clock has
        // to be able to regress one.
        clock.set(SystemTime::UNIX_EPOCH + Duration::from_secs(5));
        assert_eq!(clock.now(), SystemTime::UNIX_EPOCH + Duration::from_secs(5));
    }

    #[test]
    fn a_shared_clock_reads_through_to_the_one_instance() {
        // POSITIVE CONTROL for the `Arc` impl: moving the shared instance moves what the clone
        // reads, so a worker holding an `Arc<dyn Clock>` sees the test's advance.
        let clock: Arc<dyn Clock> = Arc::new(FixedClock::at_unix_seconds(10));
        let held = Arc::clone(&clock);
        let before = held.now();
        // `Arc<dyn Clock>` cannot be advanced through the trait; downcast is not offered, so the
        // test holds the concrete type too.
        let concrete = Arc::new(FixedClock::at_unix_seconds(10));
        let shared: Arc<dyn Clock> = Arc::clone(&concrete) as Arc<dyn Clock>;
        concrete.advance(Duration::from_secs(1));
        assert_eq!(shared.now(), before + Duration::from_secs(1));
    }
}
