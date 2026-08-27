//! The domain clock: an instant a test can move rather than wait for.
//!
//! # Why this is not `renvor_testkit::clock::TestClock`
//!
//! That type exists and this is deliberately **not** a duplicate of it. `TestClock` drives
//! **tokio's virtual time** so a deadline fires without real elapsed time, and it says of itself:
//!
//! > *"There is no `now()` here… a clock that could be read is an invitation to write the
//! > assertion the requirement prohibits."*
//!
//! That is the right design for a *timer*. It is the wrong design for authentication, which does
//! not wait for deadlines — it **compares stored instants**. A session's absolute expiry is
//! `created_at + ttl <= now`, and evaluating it requires reading a clock.
//!
//! So the two coexist: `TestClock` for anything that awaits, [`Clock`] for anything that compares.
//! A reviewer who reads this as duplication should read this paragraph instead.
//!
//! # Production reads the real clock through the same trait
//!
//! There is no `cfg(test)` branch anywhere in this module. [`SystemClock`] is what production
//! constructs and [`FixedClock`] is what a test constructs, and both satisfy [`Clock`] — so the
//! code under test is the code that ships.

use chrono::{DateTime, Duration, Utc};

/// A source of the current instant.
///
/// Every expiry comparison in this crate goes through this trait. Nothing calls `Utc::now()`
/// directly, which is what makes an expiry test deterministic instead of timing-dependent.
pub trait Clock: Send + Sync + core::fmt::Debug {
    /// The current instant, in UTC.
    ///
    /// UTC is not a formatting preference. `contracts/database-portability.md` §2 requires storing
    /// UTC and converting at the edge, because MySQL's `TIMESTAMP` converts on read using the
    /// *session* zone — so the same row read by two sessions otherwise yields two values.
    fn now(&self) -> DateTime<Utc>;
}

/// The production clock: the operating system's wall clock, and nothing else.
///
/// Has **no fields**, so there is nowhere for an offset, a skew, or a "for testing" override to be
/// stored. The same structural argument `renvor_core::observe::entropy::OsEntropy` makes.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl SystemClock {
    /// Creates the production clock. Takes **0** inputs, deliberately.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A clock a test controls.
///
/// Available outside `cfg(test)` on purpose, for the same reason
/// `renvor_core::observe::entropy::FixedEntropy` is: an application author writing a deterministic
/// test of their own expiry behaviour needs one too.
#[derive(Clone, Debug)]
pub struct FixedClock {
    now: std::sync::Arc<std::sync::Mutex<DateTime<Utc>>>,
}

impl FixedClock {
    /// Creates a clock reading `now`.
    #[must_use]
    pub fn at(now: DateTime<Utc>) -> Self {
        Self {
            now: std::sync::Arc::new(std::sync::Mutex::new(now)),
        }
    }

    /// Moves the clock forward by `duration`, costing **0** real time.
    ///
    /// # Panics
    ///
    /// If the lock is poisoned by a panic in another thread while the clock was held.
    pub fn advance(&self, duration: Duration) {
        let mut now = self
            .now
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *now += duration;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self
            .now
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::{Clock, FixedClock, SystemClock};
    use chrono::{Duration, TimeZone as _, Utc};

    #[test]
    fn a_fixed_clock_does_not_move_on_its_own() {
        // The property that makes an expiry test deterministic: reading twice yields the same
        // instant, so an assertion cannot pass or fail depending on how long the test took.
        let clock = FixedClock::at(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
        let first = clock.now();
        let second = clock.now();
        assert_eq!(first, second);
    }

    #[test]
    fn advancing_costs_no_real_time() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let clock = FixedClock::at(start);
        let before = std::time::Instant::now();
        clock.advance(Duration::days(365));
        let elapsed = before.elapsed();

        assert_eq!(clock.now(), start + Duration::days(365));
        assert!(
            elapsed < std::time::Duration::from_millis(100),
            "advancing a year must not wait a year: took {elapsed:?}"
        );
    }

    #[test]
    fn the_production_clock_reads_a_real_instant() {
        // POSITIVE CONTROL. Without this, a `SystemClock` that returned the UNIX epoch would
        // satisfy every other test in this module while making every expiry permanently in the
        // future.
        let clock = SystemClock::new();
        let now = clock.now();
        let epoch = Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap();
        assert!(
            now > epoch + Duration::days(365 * 50),
            "the clock returned {now}"
        );
    }

    #[test]
    fn both_clocks_satisfy_the_same_port() {
        // The seam is a trait implemented differently, NOT a global a test can set — so the code
        // under test is the code that ships.
        fn read(clock: &dyn Clock) -> chrono::DateTime<Utc> {
            clock.now()
        }
        let fixed = FixedClock::at(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
        assert_eq!(read(&fixed), fixed.now());
        assert!(read(&SystemClock::new()) > Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap());
    }
}
